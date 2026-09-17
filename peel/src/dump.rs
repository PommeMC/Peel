//! `peel dump`: prints a trace grouped by tick.

use std::fs::File;
use std::io::{self, BufReader, BufWriter, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};
use peel_decode::fields::{Decoded, Field};
use peel_decode::{GroupKind, Grouping, Timeline, WINDOW_US, names};
use peel_trace::Direction;

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
    let file =
        File::open(&args.trace).with_context(|| format!("opening {}", args.trace.display()))?;
    let loaded = peel_decode::load(BufReader::new(file))
        .with_context(|| format!("reading {}", args.trace.display()))?;
    let mut timeline = loaded.timeline();
    if args.fields {
        timeline.decode_fields(&loaded.records);
    }
    let mut out = BufWriter::new(io::stdout().lock());
    let written = if args.json {
        serde_json::to_writer_pretty(&mut out, &timeline)
            .map_err(io::Error::other)
            .and_then(|()| writeln!(out))
    } else {
        write_text(&timeline, &mut out)
    };
    match written.and_then(|()| out.flush()) {
        Err(err) if err.kind() != io::ErrorKind::BrokenPipe => return Err(err.into()),
        _ => {}
    }
    if args.fields {
        eprintln!("peel: {}", field_summary(&timeline));
    }
    if let Some(warning) = loaded.warning() {
        eprintln!("peel: {warning}");
    }
    Ok(())
}

fn field_summary(timeline: &Timeline) -> String {
    let (mut total, mut decoded, mut failed) = (0, 0, 0);
    for packet in timeline.packets() {
        total += 1;
        match packet.fields {
            Some(Decoded::Fields { .. }) => decoded += 1,
            Some(Decoded::Failed { .. }) => failed += 1,
            _ => {}
        }
    }
    format!("decoded the fields of {decoded} of {total} packets, {failed} failed")
}

fn write_text(timeline: &Timeline, out: &mut impl Write) -> io::Result<()> {
    writeln!(
        out,
        "trace format {}, recorded by {}, started at unix ms {}",
        timeline.format_version, timeline.recorder, timeline.started_at_unix_ms
    )?;
    match (timeline.protocol, timeline.version) {
        (None, _) => return writeln!(out, "no packets recorded"),
        (Some(protocol), Some(version)) => writeln!(out, "protocol {protocol} ({version})")?,
        (Some(protocol), None) => writeln!(
            out,
            "protocol {protocol} (not supported by this build of Peel: no packet names)"
        )?,
    }
    match timeline.grouping {
        Grouping::Tick => writeln!(out, "grouped by tick (serverbound client_tick_end)")?,
        Grouping::Approximate => writeln!(
            out,
            "APPROXIMATE: this protocol has no client_tick_end tick marker (added in 768), \
             so packets are grouped into {} ms timestamp windows, not ticks",
            WINDOW_US / 1000
        )?,
    }
    for group in &timeline.groups {
        match group.kind {
            GroupKind::Setup => writeln!(out, "\nbefore first tick")?,
            GroupKind::Tick => writeln!(out, "\ntick {}", group.index)?,
            GroupKind::Window => writeln!(
                out,
                "\nwindow +{:.3}s (approximate)",
                seconds(group.start_us)
            )?,
        }
        for p in &group.packets {
            writeln!(
                out,
                "  {:>12}  {}  {:<13}  {:<32}  0x{:02x}  {} B",
                format!("+{:.6}s", seconds(p.timestamp_us)),
                match p.direction {
                    Direction::Serverbound => "to server",
                    Direction::Clientbound => "to client",
                },
                names::phase_label(p.phase),
                p.name.unwrap_or("?"),
                p.id,
                p.length
            )?;
            match &p.fields {
                Some(Decoded::Fields { fields }) => write_fields(fields, out)?,
                Some(Decoded::Failed { fields, error, at }) => {
                    write_fields(fields, out)?;
                    writeln!(out, "      ! stopped at byte {at}: {error}")?;
                }
                _ => {}
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
    use peel_decode::test_util::{named, session};
    use peel_trace::{FORMAT_VERSION, Header, Phase, Record};
    use pomme_protocol::PacketTable;
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
        let table = PacketTable::for_protocol(protocol).unwrap();
        let (packet_id, frame) = named(
            table,
            Phase::Game,
            Direction::Clientbound,
            "keep_alive",
            &5i64.to_be_bytes(),
        );
        // The session's packets carry 4 zero bytes, too few or too many for
        // most decoders.
        let mut records = session(protocol, "client_tick_end");
        records.push(Record {
            timestamp_us: 130_000,
            protocol,
            direction: Direction::Clientbound,
            phase: Phase::Game,
            packet_id,
            frame,
        });
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
            "decoded the fields of 1 of 7 packets, 5 failed"
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
