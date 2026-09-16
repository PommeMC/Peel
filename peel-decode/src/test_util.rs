//! Packet builders for tests, looked up by name so no test hardcodes an id.

use peel_trace::{Direction, Phase, Record};
use pomme_protocol::PacketTable;
use pomme_protocol::version::LATEST;
use pomme_protocol::wire::write_varint;

use crate::names;

pub fn packet(id: u32, body: &[u8]) -> Vec<u8> {
    let mut packet = Vec::new();
    write_varint(&mut packet, id);
    packet.extend_from_slice(body);
    packet
}

/// A packet looked up by name in `table`, with its id.
pub fn named(
    table: &PacketTable,
    phase: Phase,
    direction: Direction,
    name: &str,
    body: &[u8],
) -> (u32, Vec<u8>) {
    let id = names::packet_id(table, phase, direction, name).unwrap_or_else(|| {
        let version = table.version().name;
        panic!("{version} has no {phase:?} {direction:?} {name}")
    });
    (id, packet(id, body))
}

/// The table for the newest supported version.
pub fn latest() -> &'static PacketTable {
    PacketTable::for_protocol(LATEST.protocol).expect("the newest version has tables")
}

pub fn record(protocol: i32, direction: Direction, phase: Phase, name: &str, us: u64) -> Record {
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

/// A handshake and login, then two game "ticks" closed by `tick_marker`.
pub fn session(protocol: i32, tick_marker: &str) -> Vec<Record> {
    vec![
        record(
            protocol,
            Direction::Serverbound,
            Phase::Handshake,
            "intention",
            0,
        ),
        record(protocol, Direction::Serverbound, Phase::Login, "hello", 10),
        record(
            protocol,
            Direction::Clientbound,
            Phase::Game,
            "login",
            20_000,
        ),
        record(
            protocol,
            Direction::Serverbound,
            Phase::Game,
            tick_marker,
            30_000,
        ),
        record(
            protocol,
            Direction::Clientbound,
            Phase::Game,
            "keep_alive",
            60_000,
        ),
        record(
            protocol,
            Direction::Serverbound,
            Phase::Game,
            tick_marker,
            120_000,
        ),
    ]
}
