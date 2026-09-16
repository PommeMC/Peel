//! `peel dump`: prints a trace grouped by tick.

use std::fs::File;
use std::io::{self, BufReader, BufWriter, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};
use peel_decode::{GroupKind, Grouping, Timeline, WINDOW_US, names};
use peel_trace::Direction;

#[derive(clap::Args)]
pub struct Args {
    /// The trace file to print.
    trace: PathBuf,
    /// Print one JSON document instead of text.
    #[arg(long)]
    json: bool,
}

pub fn run(args: Args) -> Result<()> {
    let file =
        File::open(&args.trace).with_context(|| format!("opening {}", args.trace.display()))?;
    let loaded = peel_decode::load(BufReader::new(file))
        .with_context(|| format!("reading {}", args.trace.display()))?;
    let timeline = loaded.timeline();
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
    if let Some(warning) = loaded.warning() {
        eprintln!("peel: {warning}");
    }
    Ok(())
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
                    Direction::Serverbound => "C->S",
                    Direction::Clientbound => "S->C",
                },
                names::phase_label(p.phase),
                p.name.unwrap_or("?"),
                p.id,
                p.length
            )?;
        }
    }
    Ok(())
}

fn seconds(us: u64) -> f64 {
    us as f64 / 1e6
}

#[cfg(test)]
mod tests {
    use peel_decode::test_util::session;
    use peel_trace::{FORMAT_VERSION, Header};

    use super::*;

    #[test]
    fn approximate_text_has_no_ticks() {
        let header = Header {
            started_at_unix_ms: 0,
            recorder: "test".into(),
        };
        let records = session(767, "keep_alive");
        let mut text = Vec::new();
        write_text(
            &Timeline::build(FORMAT_VERSION, &header, &records),
            &mut text,
        )
        .unwrap();
        let text = String::from_utf8(text).unwrap();
        assert!(text.contains("APPROXIMATE"), "{text}");
        assert!(!text.contains("\ntick"), "{text}");
    }
}
