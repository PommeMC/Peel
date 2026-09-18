//! What the trace-reading commands share: loading a trace file and printing.

use std::fs;
use std::io::{self, BufWriter, StdoutLock, Write};
use std::path::Path;

use anyhow::{Context, Result};
use peel_decode::{Grouping, Loaded, WINDOW_US, names};
use peel_trace::{Direction, Phase};
use serde::Serialize;

/// A trace file's bytes, and the trace read from them.
pub fn read_trace(path: &Path) -> Result<(Vec<u8>, Loaded)> {
    let bytes = fs::read(path).with_context(|| format!("opening {}", path.display()))?;
    let loaded = peel_decode::load(bytes.as_slice())
        .with_context(|| format!("reading {}", path.display()))?;
    Ok((bytes, loaded))
}

/// Writes to stdout with `write`, ending quietly when the reader goes away,
/// as when piping into `head`.
pub fn print(write: impl FnOnce(&mut BufWriter<StdoutLock>) -> io::Result<()>) -> Result<()> {
    let mut out = BufWriter::new(io::stdout().lock());
    match write(&mut out).and_then(|()| out.flush()) {
        Err(err) if err.kind() != io::ErrorKind::BrokenPipe => Err(err.into()),
        _ => Ok(()),
    }
}

pub fn write_json(out: &mut impl Write, value: &impl Serialize) -> io::Result<()> {
    serde_json::to_writer_pretty(&mut *out, value).map_err(io::Error::other)?;
    writeln!(out)
}

/// The start of a row naming one packet, as `peel diff` and `peel coverage`
/// both list them: direction, phase and what the row is about.
pub fn write_packet_label(
    out: &mut impl Write,
    direction: Direction,
    phase: Phase,
    label: &str,
) -> io::Result<()> {
    write!(
        out,
        "  {}  {:<13}  {label:<40}  ",
        direction_label(direction),
        names::phase_label(phase)
    )
}

pub fn direction_label(direction: Direction) -> &'static str {
    match direction {
        Direction::Serverbound => "to server",
        Direction::Clientbound => "to client",
    }
}

/// The protocol line and how packets are grouped, flagging approximate
/// grouping.
pub fn write_version(
    out: &mut impl Write,
    protocol: i32,
    version: Option<&str>,
    grouping: Grouping,
) -> io::Result<()> {
    match version {
        Some(version) => writeln!(out, "protocol {protocol} ({version})")?,
        None => writeln!(
            out,
            "protocol {protocol} (not supported by this build of Peel: no packet names)"
        )?,
    }
    match grouping {
        Grouping::Tick => writeln!(out, "grouped by tick (serverbound client_tick_end)"),
        Grouping::Approximate => writeln!(
            out,
            "APPROXIMATE: this protocol has no client_tick_end tick marker (added in 768), \
             so packets are grouped into {} ms timestamp windows, not ticks",
            WINDOW_US / 1000
        ),
    }
}
