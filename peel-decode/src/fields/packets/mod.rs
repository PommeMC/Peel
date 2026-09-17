//! Decoders grouped by the part of the protocol they cover. Protocol
//! boundaries come from diffing each codec across the reference versions.

mod chat;
mod common;
mod connection;
mod entities;
mod nbt;
mod player;
mod text;
mod world;

use std::ops::RangeInclusive;

use peel_trace::{Direction, Phase};

use super::reader::{Reader, Result};
use super::{Decoder, PacketDecoder};

pub static DECODERS: &[&[PacketDecoder]] = &[
    connection::DECODERS,
    common::DECODERS,
    player::DECODERS,
    entities::DECODERS,
    world::DECODERS,
    chat::DECODERS,
];

/// The protocol of each version where a codec changed.
pub const V1_20_2: i32 = 764;
pub const V1_20_3: i32 = 765;
pub const V1_20_5: i32 = 766;
pub const V1_21: i32 = 767;
pub const V1_21_2: i32 = 768;
pub const V1_21_4: i32 = 769;
pub const V1_21_5: i32 = 770;
pub const V1_21_6: i32 = 771;
pub const V1_21_9: i32 = 773;
pub const V1_21_11: i32 = 774;
pub const V26_1: i32 = 775;
pub const V26_2: i32 = 776;
pub const V26_3: i32 = 777;

pub const GAME: &[Phase] = &[Phase::Game];

pub const fn decoder(
    phases: &'static [Phase],
    direction: Direction,
    name: &'static str,
    protocols: RangeInclusive<i32>,
    decode: fn(&mut Decoder) -> Result<()>,
) -> PacketDecoder {
    PacketDecoder {
        phases,
        direction,
        name,
        protocols,
        decode,
    }
}

/// A game-phase packet.
pub const fn game(
    direction: Direction,
    name: &'static str,
    protocols: RangeInclusive<i32>,
    decode: fn(&mut Decoder) -> Result<()>,
) -> PacketDecoder {
    decoder(GAME, direction, name, protocols, decode)
}

/// Yaw then pitch, as floats.
pub fn rotation(d: &mut Decoder) -> Result<()> {
    d.f32("y_rot")?;
    d.f32("x_rot")?;
    Ok(())
}

/// `PositionMoveRotation`, from 1.21.2.
pub fn position_move_rotation(d: &mut Decoder) -> Result<()> {
    d.vec3("position")?;
    d.vec3("delta_movement")?;
    rotation(d)
}

/// `Relative` flags by bit; before 1.21.2 a byte held the first five.
pub const RELATIVES: &[&str] = &[
    "x",
    "y",
    "z",
    "y_rot",
    "x_rot",
    "delta_x",
    "delta_y",
    "delta_z",
    "rotate_delta",
];

/// `Relative.SET_STREAM_CODEC`, from 1.21.2: an int of flags.
pub fn relatives(d: &mut Decoder) -> Result<()> {
    d.flags("relatives", Reader::i32, RELATIVES)?;
    Ok(())
}

/// `InteractionHand`.
pub const HAND: &[&str] = &["main_hand", "off_hand"];
/// `Direction`, by 3D data value.
pub const DIRECTION: &[&str] = &["down", "up", "north", "south", "west", "east"];

#[cfg(test)]
mod tests {
    use pomme_protocol::version::VERSIONS;
    use pomme_protocol::{PacketTable, ProtocolVersion};

    use super::super::testing::protocols;
    use super::super::{NEWEST, OLDEST};
    use super::*;
    use crate::names;

    fn all() -> impl Iterator<Item = &'static PacketDecoder> {
        DECODERS.iter().flat_map(|group| group.iter())
    }

    #[test]
    fn version_constants_match_the_tables() {
        for (constant, name) in [
            (V1_20_2, "1.20.2"),
            (V1_20_3, "1.20.3"),
            (V1_20_5, "1.20.5"),
            (V1_21, "1.21"),
            (V1_21_2, "1.21.2"),
            (V1_21_4, "1.21.4"),
            (V1_21_5, "1.21.5"),
            (V1_21_6, "1.21.6"),
            (V1_21_9, "1.21.9"),
            (V1_21_11, "1.21.11"),
            (V26_1, "26.1"),
            (V26_2, "26.2"),
            (V26_3, "26.3"),
        ] {
            assert_eq!(
                Some(constant),
                ProtocolVersion::from_name(name).map(|v| v.protocol),
                "{name}"
            );
        }
    }

    #[test]
    fn checked_range_spans_the_supported_versions() {
        assert_eq!(NEWEST, VERSIONS.first().unwrap().protocol);
        assert_eq!(OLDEST, VERSIONS.last().unwrap().protocol);
    }

    #[test]
    fn every_decoder_names_a_packet_in_its_whole_range() {
        let supported = protocols(OLDEST..=NEWEST);
        for d in all() {
            let (start, end) = (*d.protocols.start(), *d.protocols.end());
            assert!(
                supported.contains(&start) && supported.contains(&end) && start <= end,
                "{} range {start}..={end} must start and end at supported protocols",
                d.name
            );
            for protocol in protocols(d.protocols.clone()) {
                let table = PacketTable::for_protocol(protocol).unwrap();
                for &phase in d.phases {
                    assert!(
                        names::packet_id(table, phase, d.direction, d.name).is_some(),
                        "{protocol} has no {phase:?} {:?} {}",
                        d.direction,
                        d.name
                    );
                }
            }
        }
    }

    #[test]
    fn ranges_for_one_packet_never_overlap() {
        let decoders: Vec<_> = all().collect();
        for (i, a) in decoders.iter().enumerate() {
            for b in &decoders[i + 1..] {
                let same_packet = a.name == b.name
                    && a.direction == b.direction
                    && a.phases.iter().any(|p| b.phases.contains(p));
                let overlap = a.protocols.start() <= b.protocols.end()
                    && b.protocols.start() <= a.protocols.end();
                assert!(!(same_packet && overlap), "{} decoders overlap", a.name);
            }
        }
    }
}
