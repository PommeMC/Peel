//! `peel dump`: prints a trace grouped by tick.

use std::io::{self, Write};
use std::path::PathBuf;

use anyhow::Result;
use peel_decode::fields::{Decoded, Field};
use peel_decode::{Timeline, names};

use crate::cli::{self, direction_label};

#[derive(clap::Args)]
pub struct Args {
    /// The trace file to print.
    trace: PathBuf,
    /// Print one JSON document instead of text.
    #[arg(long)]
    json: bool,
    /// Decode each packet's fields, where Peel has a decoder for the packet.
    #[arg(long)]
    fields: bool,
}

pub fn run(args: Args) -> Result<()> {
    let (_, loaded) = cli::read_trace(&args.trace)?;
    let mut timeline = loaded.timeline();
    if args.fields {
        timeline.decode_fields(&loaded.records);
    }
    cli::print(|out| {
        if args.json {
            cli::write_json(out, &timeline)
        } else {
            write_text(&timeline, out)
        }
    })?;
    if args.fields {
        eprintln!("peel: {}", field_summary(&timeline));
    }
    if let Some(warning) = loaded.warning() {
        eprintln!("peel: {warning}");
    }
    Ok(())
}

fn field_summary(timeline: &Timeline) -> String {
    let (mut total, mut decoded, mut partly, mut failed) = (0, 0, 0, 0);
    for packet in timeline.packets() {
        total += 1;
        match packet.fields {
            Some(Decoded::Fields { .. }) => decoded += 1,
            Some(Decoded::Partial { .. }) => partly += 1,
            Some(Decoded::Failed { .. }) => failed += 1,
            _ => {}
        }
    }
    format!("decoded the fields of {decoded} of {total} packets, {partly} partly, {failed} failed")
}

fn write_text(timeline: &Timeline, out: &mut impl Write) -> io::Result<()> {
    writeln!(
        out,
        "trace format {}, recorded by {}, started at unix ms {}",
        timeline.format_version, timeline.recorder, timeline.started_at_unix_ms
    )?;
    let Some(protocol) = timeline.protocol else {
        return writeln!(out, "no packets recorded");
    };
    cli::write_version(out, protocol, timeline.version, timeline.grouping)?;
    for group in &timeline.groups {
        writeln!(out, "\n{}", group.key())?;
        for p in &group.packets {
            writeln!(
                out,
                "  {:>12}  {}  {:<13}  {:<32}  0x{:02x}  {} B",
                format!("+{:.6}s", seconds(p.timestamp_us)),
                direction_label(p.direction),
                names::phase_label(p.phase),
                p.name.unwrap_or("?"),
                p.id,
                p.length
            )?;
            if let Some(decoded) = &p.fields {
                write_fields(decoded.fields(), out)?;
                if let Some(note) = decoded.stop_note() {
                    writeln!(out, "      ! {note}")?;
                }
            }
        }
    }
    Ok(())
}

fn write_fields(fields: &[Field], out: &mut impl Write) -> io::Result<()> {
    let mut written = Ok(());
    for field in fields {
        field.walk(3, &mut |depth, f| {
            if written.is_ok() {
                let value = f
                    .value
                    .as_deref()
                    .map_or(String::new(), |v| format!(" {v}"));
                written = writeln!(out, "{}{}:{value}", "  ".repeat(depth), f.name);
            }
        });
    }
    written
}

fn seconds(us: u64) -> f64 {
    us as f64 / 1e6
}

#[cfg(test)]
mod tests {
    use peel_decode::test_util::{record_with_body, session};
    use peel_trace::{Direction, FORMAT_VERSION, Header, Phase};
    use pomme_protocol::version::LATEST;

    use super::*;

    fn header() -> Header {
        Header {
            started_at_unix_ms: 0,
            recorder: "test".into(),
        }
    }

    fn text(timeline: &Timeline) -> String {
        let mut text = Vec::new();
        write_text(timeline, &mut text).unwrap();
        String::from_utf8(text).unwrap()
    }

    #[test]
    fn fields_print_under_their_packet() {
        let protocol = LATEST.protocol;
        // The session's packets carry 4 zero bytes, too few or too many for
        // most decoders.
        let mut records = session(protocol, "client_tick_end");
        records.push(record_with_body(
            protocol,
            Direction::Clientbound,
            Phase::Game,
            "keep_alive",
            &5i64.to_be_bytes(),
            130_000,
        ));
        let header = header();
        let mut timeline = Timeline::build(FORMAT_VERSION, &header, &records);
        timeline.decode_fields(&records);
        let text = text(&timeline);
        assert!(text.contains(" 9 B\n      id: 5\n"), "{text}");
        assert!(
            text.contains("\n      ! stopped at byte 1: a long needs 8 bytes, 4 left\n"),
            "{text}"
        );
        assert_eq!(
            field_summary(&timeline),
            "decoded the fields of 1 of 7 packets, 0 partly, 6 failed"
        );
    }

    #[test]
    fn approximate_text_has_no_ticks() {
        let header = header();
        let records = session(767, "keep_alive");
        let text = text(&Timeline::build(FORMAT_VERSION, &header, &records));
        assert!(text.contains("APPROXIMATE"), "{text}");
        assert!(!text.contains("\ntick"), "{text}");
    }
}
