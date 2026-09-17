use peel_trace::{Direction, Header, Phase, Record};
use pomme_protocol::{PacketTable, ProtocolVersion};
use serde::Serialize;

use crate::fields::{self, Decoded};
use crate::names::{self, TickMarker};

/// Width of the approximate grouping windows used below protocol 768: one
/// nominal game tick.
pub const WINDOW_US: u64 = 50_000;

/// A trace grouped into ticks, or timestamp windows where the protocol has no
/// tick marker. Serialized as `peel dump --json`.
#[derive(Serialize)]
pub struct Timeline<'a> {
    pub format_version: u16,
    pub started_at_unix_ms: u64,
    pub recorder: &'a str,
    pub protocol: Option<i32>,
    pub version: Option<&'static str>,
    pub grouping: Grouping,
    pub groups: Vec<Group>,
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Grouping {
    /// Delimited by serverbound `client_tick_end`.
    Tick,
    /// No tick marker in this protocol: fixed timestamp windows.
    Approximate,
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum GroupKind {
    /// Before the first game packet of a ticked trace.
    Setup,
    Tick,
    Window,
}

#[derive(Serialize)]
pub struct Group {
    pub kind: GroupKind,
    pub index: u64,
    pub start_us: u64,
    pub packets: Vec<Packet>,
}

#[derive(Serialize)]
pub struct Packet {
    /// The record's position in the trace.
    pub index: usize,
    pub timestamp_us: u64,
    pub direction: Direction,
    pub phase: Phase,
    pub id: u32,
    pub name: Option<&'static str>,
    /// The uncompressed frame length.
    pub length: usize,
    /// Filled by [`Timeline::decode_fields`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<Decoded>,
}

impl<'a> Timeline<'a> {
    pub fn build(format_version: u16, header: &'a Header, records: &[Record]) -> Self {
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

    /// Decodes every packet's fields from the `records` the timeline was
    /// built from.
    pub fn decode_fields(&mut self, records: &[Record]) {
        for packet in self.groups.iter_mut().flat_map(|g| &mut g.packets) {
            packet.fields = Some(fields::decode(&records[packet.index]));
        }
    }

    pub fn packets(&self) -> impl Iterator<Item = &Packet> {
        self.groups.iter().flat_map(|g| &g.packets)
    }
}

fn group(records: &[Record]) -> (Grouping, Vec<Group>) {
    let marker = records
        .first()
        .and_then(|first| TickMarker::for_protocol(first.protocol));
    let mut groups: Vec<Group> = Vec::new();
    let Some(marker) = marker else {
        for (i, record) in records.iter().enumerate() {
            let index = record.timestamp_us / WINDOW_US;
            if groups.last().is_none_or(|g| g.index != index) {
                groups.push(Group::new(GroupKind::Window, index, index * WINDOW_US));
            }
            groups
                .last_mut()
                .unwrap()
                .packets
                .push(Packet::new(i, record));
        }
        return (Grouping::Approximate, groups);
    };

    let mut ticks = 0;
    let mut open = false;
    for (i, record) in records.iter().enumerate() {
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
        groups
            .last_mut()
            .unwrap()
            .packets
            .push(Packet::new(i, record));
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
    fn new(index: usize, record: &Record) -> Self {
        Self {
            index,
            timestamp_us: record.timestamp_us,
            direction: record.direction,
            phase: record.phase,
            id: record.packet_id,
            name: PacketTable::for_protocol(record.protocol).and_then(|table| {
                names::packet_name(table, record.phase, record.direction, record.packet_id)
            }),
            length: record.frame.len(),
            fields: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use peel_trace::FORMAT_VERSION;
    use pomme_protocol::version::LATEST;

    use super::*;
    use crate::test_util::session;

    fn shape(groups: &[Group]) -> Vec<(GroupKind, u64, Vec<Option<&str>>)> {
        groups
            .iter()
            .map(|g| (g.kind, g.index, g.packets.iter().map(|p| p.name).collect()))
            .collect()
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
    }

    #[test]
    fn json_shape() {
        let header = Header {
            started_at_unix_ms: 7,
            recorder: "test".into(),
        };
        let records = session(LATEST.protocol, "client_tick_end");
        let json =
            serde_json::to_value(Timeline::build(FORMAT_VERSION, &header, &records)).unwrap();
        assert_eq!(json["protocol"], LATEST.protocol);
        assert_eq!(json["version"], LATEST.name);
        assert_eq!(json["grouping"], "tick");
        let tick = &json["groups"][1];
        assert_eq!(tick["kind"], "tick");
        assert_eq!(tick["index"], 1);
        assert_eq!(tick["start_us"], 20_000);
        let marker = &tick["packets"][1];
        assert_eq!(marker["index"], 3);
        assert_eq!(marker["name"], "client_tick_end");
        assert_eq!(marker["direction"], "serverbound");
        assert_eq!(marker["phase"], "game");
        assert_eq!(marker["length"], 5);
    }
}
