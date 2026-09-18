//! What a trace decoded, and where decoding stopped: the report behind
//! `peel coverage` and the viewer's coverage panel.

use std::collections::HashMap;

use peel_trace::{Direction, Phase};
use pomme_protocol::PacketTable;
use pomme_protocol::version::VERSIONS;
use serde::Serialize;

use crate::fields::{self, Decoded};
use crate::{FieldCounts, Loaded, names};

/// How much of the protocol Peel decodes at all, across every protocol
/// `pomme-protocol` has tables for.
#[derive(Debug, PartialEq, Serialize)]
pub struct Support {
    pub versions: usize,
    pub packets: usize,
    /// Packets with a decoder. A test keeps this equal to `packets`.
    pub decoded: usize,
}

#[derive(Debug, Serialize)]
pub struct Coverage {
    pub protocol: Option<i32>,
    pub version: Option<&'static str>,
    pub support: Support,
    pub counts: FieldCounts,
    /// One row per packet the trace holds, the ones that stopped first.
    pub packets: Vec<PacketCoverage>,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct PacketCoverage {
    pub direction: Direction,
    pub phase: Phase,
    pub id: u32,
    pub name: Option<&'static str>,
    pub counts: FieldCounts,
    /// Why the first one stopped, and the record it was.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record: Option<usize>,
}

pub fn support() -> Support {
    let mut support = Support {
        versions: 0,
        packets: 0,
        decoded: 0,
    };
    // Versions that share a protocol, such as 26.1.1 and 26.1, share a table.
    let mut protocols: Vec<i32> = VERSIONS.iter().map(|v| v.protocol).collect();
    protocols.dedup();
    for protocol in protocols {
        let Some(table) = PacketTable::for_protocol(protocol) else {
            continue;
        };
        support.versions += 1;
        for phase in names::PHASES {
            for direction in [Direction::Serverbound, Direction::Clientbound] {
                for name in (0..).map_while(|id| names::packet_name(table, phase, direction, id)) {
                    support.packets += 1;
                    if fields::has_decoder(protocol, phase, direction, name) {
                        support.decoded += 1;
                    }
                }
            }
        }
    }
    support
}

/// Decodes every packet and counts how each one went, worst packets first.
pub fn coverage(loaded: &Loaded) -> Coverage {
    let mut timeline = loaded.timeline();
    timeline.decode_fields(&loaded.records);

    let mut rows: Vec<PacketCoverage> = Vec::new();
    let mut positions: HashMap<(Direction, Phase, u32), usize> = HashMap::new();
    for packet in timeline.packets() {
        let key = (packet.direction, packet.phase, packet.id);
        let position = *positions.entry(key).or_insert_with(|| {
            rows.push(PacketCoverage {
                direction: packet.direction,
                phase: packet.phase,
                id: packet.id,
                name: packet.name,
                counts: FieldCounts::default(),
                reason: None,
                record: None,
            });
            rows.len() - 1
        });
        let row = &mut rows[position];
        row.counts.add(packet.fields.as_ref());
        if row.reason.is_none()
            && let Some(note) = packet.fields.as_ref().and_then(Decoded::stop_note)
        {
            row.reason = Some(note);
            row.record = Some(packet.index);
        }
    }
    // Failures first, then packets that stopped part way, then the rest.
    rows.sort_by_key(|row| {
        (
            row.counts.failed == 0,
            !row.counts.stops(),
            row.name.unwrap_or_default(),
        )
    });

    Coverage {
        protocol: timeline.protocol,
        version: timeline.version,
        support: support(),
        counts: timeline.field_counts(),
        packets: rows,
    }
}

#[cfg(test)]
mod tests {
    use peel_trace::Direction::Clientbound;
    use pomme_protocol::version::LATEST;

    use super::*;
    use crate::test_util::{loaded, record_with_body, session};

    #[test]
    fn every_supported_packet_has_a_decoder() {
        let support = support();
        assert!(support.versions < VERSIONS.len(), "{support:?}");
        assert!(support.packets > 1_000, "{support:?}");
        assert_eq!(support.decoded, support.packets);
    }

    #[test]
    fn packets_that_stop_come_first() {
        let protocol = LATEST.protocol;
        let mut records = session(protocol, "client_tick_end");
        // Too few bytes for the keep_alive id it names, so decoding fails.
        records.push(record_with_body(
            protocol,
            Clientbound,
            Phase::Game,
            "keep_alive",
            &[0; 4],
            130_000,
        ));
        let coverage = coverage(&loaded(records));

        assert_eq!(coverage.version, Some(LATEST.name));
        let failed: usize = coverage.packets.iter().map(|row| row.counts.failed).sum();
        let total: usize = coverage.packets.iter().map(|row| row.counts.total).sum();
        assert_eq!(
            (failed, total),
            (coverage.counts.failed, coverage.counts.total)
        );

        let first = &coverage.packets[0];
        assert!(first.counts.stops(), "{first:?}");
        assert!(
            first.reason.as_ref().is_some_and(|r| r.contains("stopped")),
            "{first:?}"
        );
        assert!(first.record.is_some(), "{first:?}");
        // Anything that decoded in full sorts after everything that stopped.
        let stopping = coverage
            .packets
            .iter()
            .filter(|row| row.counts.stops())
            .count();
        assert!(
            coverage.packets[..stopping]
                .iter()
                .all(|row| row.counts.stops()),
            "{:?}",
            coverage.packets
        );
    }
}
