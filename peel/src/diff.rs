//! `peel diff`: compares a trace against a vanilla reference.

use std::io::{self, Write};
use std::path::PathBuf;

use anyhow::Result;
use peel_decode::diff::{Diff, Finding, FindingKind, Options};
use peel_decode::fields::count;
use peel_decode::names;

use crate::cli::{self, direction_label};

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
    /// Skip a packet (`keep_alive`) or a field (`player_input.input`,
    /// `*.salt`). Repeatable.
    #[arg(long, value_name = "RULE")]
    ignore: Vec<String>,
    /// Also compare fields that differ between any two sessions, such as
    /// positions and ids.
    #[arg(long)]
    no_default_ignores: bool,
    /// Print one JSON document, with every row, instead of text.
    #[arg(long)]
    json: bool,
}

/// Whether the traces differ.
pub fn run(args: Args) -> Result<bool> {
    let (_, trace) = cli::read_trace(&args.trace)?;
    let (_, reference) = cli::read_trace(&args.against)?;
    let options = Options {
        all_directions: args.all,
        offset: args.offset,
        ignores: args.ignore,
        no_default_ignores: args.no_default_ignores,
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
    for (path, loaded) in [(&args.against, &reference), (&args.trace, &trace)] {
        if let Some(warning) = loaded.warning() {
            eprintln!("peel: {}: {warning}", path.display());
        }
    }
    Ok(diff.differs())
}

fn write_text(diff: &Diff, out: &mut impl Write) -> io::Result<()> {
    cli::write_version(out, diff.protocol, diff.version, diff.grouping)?;
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
        (FindingKind::Moved, "moved: in both, in a different order"),
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
        n => writeln!(out, "\n{}", count(n, "finding")),
    }
}

fn write_finding(finding: &Finding, out: &mut impl Write) -> io::Result<()> {
    let name = finding.name.unwrap_or("?");
    let packet = match &finding.path {
        Some(path) => format!("{name}.{path}"),
        None => name.to_string(),
    };
    write!(
        out,
        "  {}  {:<13}  {:<40}  ",
        direction_label(finding.direction),
        names::phase_label(finding.phase),
        packet
    )?;
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
            "{}, first in {}",
            count(finding.count, "packet"),
            finding.group
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
}
