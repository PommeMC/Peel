//! `peel dump`: prints a trace grouped by tick.

use std::fs::File;
use std::io::{self, BufReader, BufWriter, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};
use peel_trace::{Direction, Header, Phase, Record, TraceReader};
use pomme_protocol::{PacketTable, ProtocolVersion};
use serde::Serialize;

use crate::names::{self, TickMarker};

/// Width of the approximate grouping windows used below protocol 768: one
/// nominal game tick.
const WINDOW_US: u64 = 50_000;

#[derive(clap::Args)]
pub struct Args {
    /// The trace file to print.
    trace: PathBuf,
    /// Print one JSON document instead of text.
    #[arg(long)]
    json: bool,
}

#[derive(Serialize)]
struct Dump<'a> {
    format_version: u16,
    started_at_unix_ms: u64,
    recorder: &'a str,
    protocol: Option<i32>,
    version: Option<&'static str>,
    grouping: Grouping,
    groups: Vec<Group>,
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
enum Grouping {
    /// Delimited by serverbound `client_tick_end`.
    Tick,
    /// No tick marker in this protocol: fixed timestamp windows.
    Approximate,
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
enum GroupKind {
    /// Before the first game packet of a ticked trace.
    Setup,
    Tick,
    Window,
}

#[derive(Serialize)]
struct Group {
    kind: GroupKind,
    index: u64,
    start_us: u64,
    packets: Vec<Packet>,
}

#[derive(Serialize)]
struct Packet {
    timestamp_us: u64,
    direction: Direction,
    phase: Phase,
    id: u32,
    name: Option<&'static str>,
    /// The uncompressed frame length.
    length: usize,
}

pub fn run(args: Args) -> Result<()> {
    let file =
        File::open(&args.trace).with_context(|| format!("opening {}", args.trace.display()))?;
    let mut reader = TraceReader::new(BufReader::new(file))
        .with_context(|| format!("reading {}", args.trace.display()))?;
    let header = reader.header().clone();
    let format_version = reader.format_version();
    let mut records = Vec::new();
    let mut error = None;
    for record in &mut reader {
        match record {
            Ok(record) => records.push(record),
            Err(err) => error = Some(err),
        }
    }

    let dump = Dump::build(format_version, &header, &records);
    let mut out = BufWriter::new(io::stdout().lock());
    let written = if args.json {
        serde_json::to_writer_pretty(&mut out, &dump)
            .map_err(io::Error::other)
            .and_then(|()| writeln!(out))
    } else {
        dump.write_text(&mut out)
    };
    match written.and_then(|()| out.flush()) {
        Err(err) if err.kind() != io::ErrorKind::BrokenPipe => return Err(err.into()),
        _ => {}
    }
    if let Some(err) = error {
        eprintln!("peel: stopped after {} records: {err}", records.len());
    }
    Ok(())
}

impl<'a> Dump<'a> {
    fn build(format_version: u16, header: &'a Header, records: &[Record]) -> Self {
        let protocol = records.first().map(|r| r.protocol);
        let (grouping, groups) = group(records);
        Self {
            format_version,
            started_at_unix_ms: header.started_at_unix_ms,
            recorder: &header.recorder,
            protocol,
            version: protocol
                .and_then(ProtocolVersion::from_protocol)
                .map(|v| v.name),
            grouping,
            groups,
        }
    }

    fn write_text(&self, out: &mut impl Write) -> io::Result<()> {
        writeln!(
            out,
            "trace format {}, recorded by {}, started at unix ms {}",
            self.format_version, self.recorder, self.started_at_unix_ms
        )?;
        match (self.protocol, self.version) {
            (None, _) => return writeln!(out, "no packets recorded"),
            (Some(protocol), Some(version)) => writeln!(out, "protocol {protocol} ({version})")?,
            (Some(protocol), None) => writeln!(
                out,
                "protocol {protocol} (not supported by this build of Peel: no packet names)"
            )?,
        }
        match self.grouping {
            Grouping::Tick => writeln!(out, "grouped by tick (serverbound client_tick_end)")?,
            Grouping::Approximate => writeln!(
                out,
                "APPROXIMATE: this protocol has no client_tick_end tick marker (added in 768), \
                 so packets are grouped into {} ms timestamp windows, not ticks",
                WINDOW_US / 1000
            )?,
        }
        for group in &self.groups {
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
}

fn seconds(us: u64) -> f64 {
    us as f64 / 1e6
}

fn group(records: &[Record]) -> (Grouping, Vec<Group>) {
    let marker = records
        .first()
        .and_then(|first| TickMarker::for_protocol(first.protocol));
    let mut groups: Vec<Group> = Vec::new();
    let Some(marker) = marker else {
        for record in records {
            let index = record.timestamp_us / WINDOW_US;
            if groups.last().is_none_or(|g| g.index != index) {
                groups.push(Group::new(GroupKind::Window, index, index * WINDOW_US));
            }
            groups.last_mut().unwrap().packets.push(Packet::new(record));
        }
        return (Grouping::Approximate, groups);
    };

    let mut ticks = 0;
    let mut open = false;
    for record in records {
        let kind = if ticks > 0 || record.phase == Phase::Game {
            GroupKind::Tick
        } else {
            GroupKind::Setup
        };
        if !open || groups.last().is_some_and(|g| g.kind != kind) {
            let index = if kind == GroupKind::Tick {
                ticks + 1
            } else {
                0
            };
            groups.push(Group::new(kind, index, record.timestamp_us));
            open = true;
        }
        groups.last_mut().unwrap().packets.push(Packet::new(record));
        if marker.ends_tick(record) {
            ticks += 1;
            open = false;
        }
    }
    (Grouping::Tick, groups)
}

impl Group {
    fn new(kind: GroupKind, index: u64, start_us: u64) -> Self {
        Self {
            kind,
            index,
            start_us,
            packets: Vec::new(),
        }
    }
}

impl Packet {
    fn new(record: &Record) -> Self {
        Self {
            timestamp_us: record.timestamp_us,
            direction: record.direction,
            phase: record.phase,
            id: record.packet_id,
            name: PacketTable::for_protocol(record.protocol).and_then(|table| {
                names::packet_name(table, record.phase, record.direction, record.packet_id)
            }),
            length: record.frame.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use Direction::{Clientbound as Cb, Serverbound as Sb};
    use peel_trace::FORMAT_VERSION;
    use pomme_protocol::version::LATEST;

    use super::*;
    use crate::wire::test_frames::named;

    fn record(protocol: i32, direction: Direction, phase: Phase, name: &str, us: u64) -> Record {
        let table = PacketTable::for_protocol(protocol).unwrap();
        let (packet_id, frame) = named(table, phase, direction, name, &[0; 4]);
        Record {
            timestamp_us: us,
            protocol,
            direction,
            phase,
            packet_id,
            frame,
        }
    }

    fn dump(records: &[Record]) -> Dump<'static> {
        static HEADER: std::sync::LazyLock<Header> = std::sync::LazyLock::new(|| Header {
            started_at_unix_ms: 7,
            recorder: "test".into(),
        });
        Dump::build(FORMAT_VERSION, &HEADER, records)
    }

    fn shape(groups: &[Group]) -> Vec<(GroupKind, u64, Vec<Option<&str>>)> {
        groups
            .iter()
            .map(|g| (g.kind, g.index, g.packets.iter().map(|p| p.name).collect()))
            .collect()
    }

    fn session(protocol: i32, tick_marker: &str) -> Vec<Record> {
        vec![
            record(protocol, Sb, Phase::Handshake, "intention", 0),
            record(protocol, Sb, Phase::Login, "hello", 10),
            record(protocol, Cb, Phase::Game, "login", 20_000),
            record(protocol, Sb, Phase::Game, tick_marker, 30_000),
            record(protocol, Cb, Phase::Game, "keep_alive", 60_000),
            record(protocol, Sb, Phase::Game, tick_marker, 120_000),
        ]
    }

    #[test]
    fn groups_by_tick_from_768() {
        let (grouping, groups) = group(&session(768, "client_tick_end"));
        assert_eq!(grouping, Grouping::Tick);
        assert_eq!(
            shape(&groups),
            [
                (GroupKind::Setup, 0, vec![Some("intention"), Some("hello")]),
                (
                    GroupKind::Tick,
                    1,
                    vec![Some("login"), Some("client_tick_end")]
                ),
                (
                    GroupKind::Tick,
                    2,
                    vec![Some("keep_alive"), Some("client_tick_end")]
                ),
            ]
        );
    }

    #[test]
    fn approximate_windows_below_768() {
        let (grouping, groups) = group(&session(767, "keep_alive"));
        assert_eq!(grouping, Grouping::Approximate);
        assert_eq!(
            shape(&groups),
            [
                (
                    GroupKind::Window,
                    0,
                    vec![
                        Some("intention"),
                        Some("hello"),
                        Some("login"),
                        Some("keep_alive")
                    ]
                ),
                (GroupKind::Window, 1, vec![Some("keep_alive")]),
                (GroupKind::Window, 2, vec![Some("keep_alive")]),
            ]
        );
        let mut text = Vec::new();
        dump(&session(767, "keep_alive"))
            .write_text(&mut text)
            .unwrap();
        let text = String::from_utf8(text).unwrap();
        assert!(text.contains("APPROXIMATE"), "{text}");
        assert!(!text.contains("\ntick"), "{text}");
    }

    #[test]
    fn json_shape() {
        let records = session(LATEST.protocol, "client_tick_end");
        let json = serde_json::to_value(dump(&records)).unwrap();
        assert_eq!(json["protocol"], LATEST.protocol);
        assert_eq!(json["version"], LATEST.name);
        assert_eq!(json["grouping"], "tick");
        let tick = &json["groups"][1];
        assert_eq!(tick["kind"], "tick");
        assert_eq!(tick["index"], 1);
        assert_eq!(tick["start_us"], 20_000);
        let marker = &tick["packets"][1];
        assert_eq!(marker["name"], "client_tick_end");
        assert_eq!(marker["direction"], "serverbound");
        assert_eq!(marker["phase"], "game");
        assert_eq!(marker["length"], 5);
    }
}
