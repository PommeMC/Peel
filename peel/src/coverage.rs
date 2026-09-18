//! `peel coverage`: what a trace decodes, and where decoding stops.

use std::io::{self, Write};
use std::path::PathBuf;

use anyhow::Result;
use peel_decode::coverage::{Coverage, PacketCoverage};
use peel_decode::fields::count;

use crate::cli;

#[derive(clap::Args)]
pub struct Args {
    /// The trace to report on.
    trace: PathBuf,
    /// List every packet, not only the ones that stop early.
    #[arg(long)]
    all: bool,
    /// Print one JSON document instead of text.
    #[arg(long)]
    json: bool,
}

pub fn run(args: Args) -> Result<()> {
    let (_, loaded) = cli::read_trace(&args.trace)?;
    let coverage = peel_decode::coverage::coverage(&loaded);
    cli::print(|out| {
        if args.json {
            cli::write_json(out, &coverage)
        } else {
            write_text(&coverage, args.all, out)
        }
    })?;
    if let Some(warning) = loaded.warning() {
        eprintln!("peel: {warning}");
    }
    Ok(())
}

fn write_text(coverage: &Coverage, all: bool, out: &mut impl Write) -> io::Result<()> {
    match (coverage.protocol, coverage.version) {
        (Some(protocol), Some(version)) => writeln!(out, "protocol {protocol} ({version})")?,
        (Some(protocol), None) => writeln!(
            out,
            "protocol {protocol} (not supported by this build of Peel: no packet names)"
        )?,
        (None, _) => writeln!(out, "empty trace")?,
    }

    let support = &coverage.support;
    let decoders = if support.decoded == support.packets {
        "every one has a decoder".to_string()
    } else {
        format!("{} with a decoder", support.decoded)
    };
    writeln!(
        out,
        "{}, {}, {decoders}",
        count(support.versions, "version"),
        count(support.packets, "packet")
    )?;

    let counts = &coverage.counts;
    writeln!(
        out,
        "{} of {} decoded, {} partly, {} failed",
        counts.decoded, counts.total, counts.partly, counts.failed
    )?;

    let stopping = coverage
        .packets
        .iter()
        .filter(|row| row.counts.stops())
        .count();
    let (heading, rows) = if all {
        ("every packet", &coverage.packets[..])
    } else {
        ("stops early", &coverage.packets[..stopping])
    };
    if rows.is_empty() {
        return writeln!(out, "\nnothing stops early");
    }
    writeln!(out, "\n{heading}")?;
    for row in rows {
        write_packet(row, out)?;
    }
    Ok(())
}

fn write_packet(row: &PacketCoverage, out: &mut impl Write) -> io::Result<()> {
    cli::write_packet_label(out, row.direction, row.phase, row.name.unwrap_or("?"))?;
    let counts = &row.counts;
    match (&row.reason, row.record) {
        (Some(reason), Some(record)) => writeln!(
            out,
            "{} of {}, record #{record}: {reason}",
            counts.partly + counts.failed,
            counts.total
        ),
        _ => writeln!(out, "{} decoded", count(counts.total, "packet")),
    }
}

#[cfg(test)]
mod tests {
    use peel_decode::test_util::{loaded, record_with_body, session};
    use peel_trace::Direction::Serverbound;
    use peel_trace::Phase;
    use pomme_protocol::version::LATEST;

    use super::*;

    fn report(all: bool) -> String {
        let protocol = LATEST.protocol;
        let mut records = session(protocol, "client_tick_end");
        // One packet that decodes in full: a float of chunks per tick.
        records.push(record_with_body(
            protocol,
            Serverbound,
            Phase::Game,
            "chunk_batch_received",
            &8.1f32.to_be_bytes(),
            130_000,
        ));
        let coverage = peel_decode::coverage::coverage(&loaded(records));
        let mut text = Vec::new();
        write_text(&coverage, all, &mut text).unwrap();
        String::from_utf8(text).unwrap()
    }

    #[test]
    fn reports_support_counts_and_what_stopped() {
        let text = report(false);
        assert!(
            text.contains(&format!("protocol {} (", LATEST.protocol)),
            "{text}"
        );
        assert!(text.contains("every one has a decoder"), "{text}");
        assert!(
            text.contains("1 of 7 decoded, 0 partly, 6 failed"),
            "{text}"
        );
        assert!(text.contains("stops early"), "{text}");
        assert!(text.contains("intention"), "{text}");

        // Packets that decoded are left out until --all.
        assert!(!text.contains("chunk_batch_received"), "{text}");
        let all = report(true);
        assert!(all.contains("every packet"), "{all}");
        assert!(all.contains("chunk_batch_received"), "{all}");
    }
}
