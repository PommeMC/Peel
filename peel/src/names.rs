//! Bridges `peel-trace`'s enums to `pomme-protocol`'s packet tables.

use peel_trace::{Direction, Phase};
use pomme_protocol::PacketTable;

pub fn table_phase(phase: Phase) -> pomme_protocol::Phase {
    match phase {
        Phase::Handshake => pomme_protocol::Phase::Handshake,
        Phase::Status => pomme_protocol::Phase::Status,
        Phase::Login => pomme_protocol::Phase::Login,
        Phase::Configuration => pomme_protocol::Phase::Configuration,
        Phase::Game => pomme_protocol::Phase::Game,
    }
}

pub fn table_direction(direction: Direction) -> pomme_protocol::Direction {
    match direction {
        Direction::Serverbound => pomme_protocol::Direction::Serverbound,
        Direction::Clientbound => pomme_protocol::Direction::Clientbound,
    }
}

pub fn packet_id(
    table: &PacketTable,
    phase: Phase,
    direction: Direction,
    name: &str,
) -> Option<u32> {
    table.id(table_phase(phase), table_direction(direction), name)
}

pub fn packet_name(
    table: &'static PacketTable,
    phase: Phase,
    direction: Direction,
    id: u32,
) -> Option<&'static str> {
    table.name_of(table_phase(phase), table_direction(direction), id)
}

pub fn phase_label(phase: Phase) -> &'static str {
    match phase {
        Phase::Handshake => "handshake",
        Phase::Status => "status",
        Phase::Login => "login",
        Phase::Configuration => "configuration",
        Phase::Game => "game",
    }
}
