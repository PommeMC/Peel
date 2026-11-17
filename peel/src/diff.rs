//! `peel diff`: compares a trace against a vanilla reference.

use std::io::{self, Write};
use std::path::PathBuf;

use anyhow::Result;
use peel_decode::diff::{DEFAULT_SHIFT_WINDOW, Diff, Finding, FindingKind, Options, Tolerance};
use peel_decode::fields::count;

use crate::cli;

#[derive(clap::Args)]
pub struct Args {
    /// The trace to check.
    trace: PathBuf,
    /// The vanilla reference trace to compare it against.
    #[arg(long, value_name = "REFERENCE")]
    against: PathBuf,
    /// Compare clientbound packets too, not only what the client sends.
    #[arg(long)]
    all: bool,
    /// Compare the trace's tick N+OFFSET with the reference's tick N.
    #[arg(long, default_value_t = 0, allow_negative_numbers = true)]
    offset: i64,
    /// Work the offset out: line up the first tick of each trace that sends
    /// this packet, such as `player_input`, among the packets compared.
    /// Repeat it to align on whichever of several comes first. Before that
    /// tick, packets that many ticks further apart still count as shifted,
    /// on top of `--shift-window`.
    #[arg(long, value_name = "PACKET", conflicts_with = "offset")]
    align_on: Vec<String>,
    /// Stop at the last tick both traces finish, so one running longer isn't
    /// reported as missing or extra packets.
    #[arg(long)]
    trim_end: bool,
    /// Skip a packet (`keep_alive`) or a field (`player_input.input`,
    /// `*.salt`). Repeatable.
    #[arg(long, value_name = "RULE")]
    ignore: Vec<String>,
    /// How many ticks apart the same packet still counts as shifted rather
    /// than missing and extra. 0 reports every such pair as both.
    #[arg(long, value_name = "TICKS", default_value_t = DEFAULT_SHIFT_WINDOW)]
    shift_window: i64,
    /// Also compare what differs between any two sessions: fields such as
    /// positions and ids, and how many replies the server asked for.
    #[arg(long)]
    no_default_ignores: bool,
    /// Compare a field that is skipped by default (`move_player_pos.position`,
    /// `*.y_rot`), leaving the other defaults on. Repeatable. For runs
    /// scripted to press the same keys on the same ticks, where positions
    /// repeat exactly. `--ignore` of the same field still wins.
    #[arg(long, value_name = "FIELD")]
    compare: Vec<String>,
    /// Count a field's numbers as the same when they are at most N apart
    /// (`move_player_rot.y_rot=0.001`), each part of a position on its own.
    /// A field skipped by default is compared too. Repeatable.
    #[arg(long, value_name = "FIELD=N", value_parser = tolerance)]
    tolerance: Vec<Tolerance>,
    /// Print one JSON document, with every row, instead of text.
    #[arg(long)]
    json: bool,
    /// Which findings make the exit status 1. `behaviour` passes a diff whose
    /// only findings are shifted packets, which is timing: the right gate
    /// when the two traces were not played tick for tick alike.
    #[arg(long, value_enum, value_name = "FINDINGS", default_value_t = FailOn::Any)]
    fail_on: FailOn,
}

#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum FailOn {
    /// Any finding at all.
    Any,
    /// Missing, extra and moved packets and fields that differ; not shifted
    /// ones.
    #[value(alias = "behavior")]
    Behaviour,
}

fn tolerance(arg: &str) -> Result<Tolerance, String> {
    let (rule, within) = arg
        .rsplit_once('=')
        .ok_or("write it as FIELD=N, such as move_player_rot.y_rot=0.001")?;
    let within = within
        .trim()
        .parse()
        .map_err(|_| format!("{within} is not a number"))?;
    Ok(Tolerance {
        rule: rule.trim().to_string(),
        within,
    })
}

impl FailOn {
    fn fails(self, diff: &Diff) -> bool {
        match self {
            Self::Any => diff.differs(),
            Self::Behaviour => diff.differs_in_behaviour(),
        }
    }
}

/// The exit code: whether the traces differ in what `--fail-on` names.
pub fn run(args: Args) -> Result<i32> {
    let (_, trace) = cli::read_trace(&args.trace)?;
    let (_, reference) = cli::read_trace(&args.against)?;
    let options = Options {
        all_directions: args.all,
        offset: args.offset,
        ignores: args.ignore,
        no_default_ignores: args.no_default_ignores,
        shift_window: Some(args.shift_window),
        align_on: args.align_on,
        trim_end: args.trim_end,
        compare: args.compare,
        tolerances: args.tolerance,
    };
    let diff = peel_decode::diff::diff(&reference, &trace, &options)?;
    cli::print(|out| {
        if args.json {
            cli::write_json(out, &diff)
        } else {
            writeln!(
                out,
                "comparing {} against {}",
                args.trace.display(),
                args.against.display()
            )?;
            write_text(&diff, out)
        }
    })?;
    for rule in &diff.unmatched {
        eprintln!(
            "peel: {rule} was asked to be compared, but no packet in both traces has that field, \
             so it checked nothing"
        );
    }
    for (path, loaded) in [(&args.against, &reference), (&args.trace, &trace)] {
        if let Some(warning) = loaded.warning() {
            eprintln!("peel: {}: {warning}", path.display());
        }
    }
    Ok(if args.fail_on.fails(&diff) {
        cli::exit::DIFFERS
    } else {
        cli::exit::OK
    })
}

fn write_text(diff: &Diff, out: &mut impl Write) -> io::Result<()> {
    cli::write_version(out, diff.protocol, diff.version, diff.grouping)?;
    match diff.aligned_on {
        Some([reference, trace]) if reference == trace => {
            writeln!(out, "aligned on each trace's first {reference}")?
        }
        Some([reference, trace]) => writeln!(
            out,
            "aligned the reference's first {reference} with the trace's first {trace}: they \
             are different packets, so the traces may not be lined up on the same action"
        )?,
        None => {}
    }
    if diff.offset != 0 {
        writeln!(
            out,
            "ticks are the reference's: its tick N is the trace's tick N{:+}",
            diff.offset
        )?;
    }
    if let Some(last) = diff.trimmed_after {
        writeln!(out, "stopped after {last}, the last both traces finish")?;
    }
    writeln!(
        out,
        "{} packets, skipping {} ignore rules",
        if diff.all_directions {
            "all"
        } else {
            "serverbound"
        },
        diff.ignores.len()
    )?;
    if !diff.compared.is_empty() {
        writeln!(
            out,
            "comparing {} although skipped by default",
            diff.compared.join(", ")
        )?;
    }
    for Tolerance { rule, within } in &diff.tolerances {
        writeln!(out, "comparing {rule} to within {within}")?;
    }
    let counts = &diff.group_counts;
    writeln!(
        out,
        "{} compared, {} only in the reference, {} only in the trace",
        count(counts.compared, "group"),
        counts.only_in_reference,
        counts.only_in_trace
    )?;

    for (kind, heading) in [
        (
            FindingKind::Missing,
            "missing: in the reference, not the trace",
        ),
        (FindingKind::Extra, "extra: in the trace, not the reference"),
        (
            FindingKind::Unanswered,
            "unanswered: sent by the server, never answered by the trace",
        ),
        (FindingKind::Moved, "moved: in both, in a different order"),
        (
            FindingKind::Shifted,
            "shifted: in both, a few ticks apart (timing, not behaviour)",
        ),
        (FindingKind::Field, "fields that differ"),
    ] {
        let mut findings = diff.findings.iter().filter(|f| f.kind == kind).peekable();
        if findings.peek().is_none() {
            continue;
        }
        writeln!(out, "\n{heading}")?;
        for finding in findings {
            write_finding(finding, out)?;
        }
    }

    match diff.findings.len() {
        0 => writeln!(out, "\nno differences"),
        n if diff.differs_in_behaviour() => writeln!(out, "\n{}", count(n, "finding")),
        n => writeln!(out, "\n{}, all timing", count(n, "finding")),
    }
}

fn write_finding(finding: &Finding, out: &mut impl Write) -> io::Result<()> {
    let name = finding.name.unwrap_or("?");
    let packet = match &finding.path {
        Some(path) => format!("{name}.{path}"),
        None => name.to_string(),
    };
    cli::write_packet_label(out, finding.direction, finding.phase, &packet)?;
    let value = |value: &Option<String>| value.clone().unwrap_or_else(|| "(none)".into());
    match finding.compared {
        Some(compared) => writeln!(
            out,
            "differs in {} of {compared}, first in {}: {} -> {}",
            finding.count,
            finding.group,
            value(&finding.reference_value),
            value(&finding.trace_value)
        ),
        None => writeln!(
            out,
            "{}, first in {}{}",
            count(finding.count, "packet"),
            finding.group,
            match finding.shift {
                Some(shift) => format!(
                    " ({} {} in the trace)",
                    count(shift.unsigned_abs() as usize, "tick"),
                    if shift < 0 { "earlier" } else { "later" }
                ),
                None => String::new(),
            }
        ),
    }
}

#[cfg(test)]
mod tests {
    use peel_decode::test_util::{loaded, record_with_body};
    use peel_trace::{Direction, Phase};
    use pomme_protocol::version::LATEST;

    use super::*;

    fn text(diff: &Diff) -> String {
        let mut text = Vec::new();
        write_text(diff, &mut text).unwrap();
        String::from_utf8(text).unwrap()
    }

    fn trace(packets: &[(&str, &[u8])]) -> peel_decode::Loaded {
        loaded(
            packets
                .iter()
                .enumerate()
                .map(|(i, (name, body))| {
                    record_with_body(
                        LATEST.protocol,
                        Direction::Serverbound,
                        Phase::Game,
                        name,
                        body,
                        i as u64,
                    )
                })
                .collect(),
        )
    }

    #[test]
    fn lists_findings_by_kind() {
        let reference = trace(&[("player_input", &[1]), ("client_tick_end", &[])]);
        let checked = trace(&[
            ("player_input", &[0x11]),
            ("player_loaded", &[]),
            ("client_tick_end", &[]),
        ]);
        let diff = peel_decode::diff::diff(&reference, &checked, &Options::default()).unwrap();
        let text = text(&diff);
        assert!(
            text.contains("\nextra: in the trace, not the reference\n"),
            "{text}"
        );
        assert!(text.contains(" player_loaded "), "{text}");
        assert!(text.contains("1 packet, first in tick 1\n"), "{text}");
        assert!(
            text.contains(
                "differs in 1 of 1, first in tick 1: 0x01 forward -> 0x11 forward | jump\n"
            ),
            "{text}"
        );
        assert!(text.ends_with("\n2 findings\n"), "{text}");
        assert!(diff.differs());
    }

    #[test]
    fn says_when_there_are_no_differences() {
        let packets = trace(&[("client_tick_end", &[])]);
        let diff = peel_decode::diff::diff(&packets, &packets, &Options::default()).unwrap();
        let text = text(&diff);
        assert!(
            text.contains("1 group compared, 0 only in the reference"),
            "{text}"
        );
        assert!(text.ends_with("\nno differences\n"), "{text}");
        assert!(!diff.differs());
    }

    #[test]
    fn says_which_fields_are_compared_and_how_closely() {
        let moved = |x: f64| {
            let body = [&x.to_be_bytes()[..], &[0; 16], &[1]].concat();
            trace(&[("move_player_pos", &body), ("client_tick_end", &[])])
        };
        let options = Options {
            compare: vec!["move_player_pos.position".into()],
            tolerances: vec![tolerance("*.y_rot = 0.5").unwrap()],
            ..Options::default()
        };
        let diff = peel_decode::diff::diff(&moved(1.0), &moved(1.5), &options).unwrap();
        let text = text(&diff);
        assert!(
            text.contains(
                "
comparing move_player_pos.position although skipped by default
"
            ),
            "{text}"
        );
        assert!(
            text.contains(
                "
comparing *.y_rot to within 0.5
"
            ),
            "{text}"
        );
        assert!(
            text.contains(
                "differs in 1 of 1, first in tick 1: 1.0, 0.0, 0.0 -> 1.5, 0.0, 0.0
"
            ),
            "{text}"
        );
        assert_eq!(diff.unmatched, ["*.y_rot"]);
        assert!(tolerance("move_player_pos.position").is_err());
        assert!(tolerance("*.y_rot=close").is_err());
    }

    /// Three ticks, with `player_loaded` in the tick given, if any.
    fn loaded_in_tick(tick: Option<usize>) -> peel_decode::Loaded {
        let mut packets: Vec<(&str, &[u8])> = vec![("client_tick_end", &[]); 3];
        if let Some(tick) = tick {
            packets.insert(tick, ("player_loaded", &[]));
        }
        trace(&packets)
    }

    #[test]
    fn fail_on_behaviour_passes_a_timing_only_diff() {
        let options = Options::default();
        let reference = loaded_in_tick(Some(2));
        let against =
            |tick| peel_decode::diff::diff(&reference, &loaded_in_tick(tick), &options).unwrap();
        let timing = against(Some(0));
        assert_eq!(timing.findings.len(), 1);
        assert_eq!(timing.findings[0].kind, FindingKind::Shifted);
        assert!(FailOn::Any.fails(&timing));
        assert!(!FailOn::Behaviour.fails(&timing));
        let text = text(&timing);
        assert!(text.ends_with("\n1 finding, all timing\n"), "{text}");
        let json = serde_json::to_value(&timing).unwrap();
        assert_eq!(json["findings"][0]["kind"], "shifted");

        // The same session without the packet at all is behaviour.
        let real = against(None);
        assert!(FailOn::Any.fails(&real));
        assert!(FailOn::Behaviour.fails(&real));
        let json = serde_json::to_value(&real).unwrap();
        assert_eq!(json["findings"][0]["kind"], "missing");

        let same = against(Some(2));
        assert!(!FailOn::Any.fails(&same));
        assert!(!FailOn::Behaviour.fails(&same));
    }
}
