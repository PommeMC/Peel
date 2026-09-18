//! Decoders grouped by the part of the protocol they cover. Protocol
//! boundaries come from diffing each codec across the reference versions.

mod advancements;
mod blocks;
mod chat;
mod chunk;
mod common;
mod components;
mod connection;
mod entities;
mod entity_data;
mod items;
mod nbt;
mod particles;
mod player;
mod recipes;
mod scoreboard;
mod session;
mod text;
mod world;

use std::ops::RangeInclusive;

use peel_trace::{Direction, Phase};
use pomme_protocol::{ClientRegistry, RegistryTable};

use super::reader::{Reader, Result};
use super::{Decoder, NEWEST, Name, OLDEST, PacketDecoder, count};

pub static DECODERS: &[&[PacketDecoder]] = &[
    connection::DECODERS,
    common::DECODERS,
    player::DECODERS,
    session::DECODERS,
    recipes::DECODERS,
    scoreboard::DECODERS,
    advancements::DECODERS,
    blocks::DECODERS,
    entities::DECODERS,
    entity_data::DECODERS,
    world::DECODERS,
    chunk::DECODERS,
    chat::DECODERS,
    items::DECODERS,
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
pub const CONFIGURATION: &[Phase] = &[Phase::Configuration];

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

/// A packet of the game phase in every version.
pub const fn in_game(
    direction: Direction,
    name: &'static str,
    decode: fn(&mut Decoder) -> Result<()>,
) -> PacketDecoder {
    decoder(GAME, direction, name, OLDEST..=NEWEST, decode)
}

/// A packet of the configuration phase, which 1.20.2 added.
pub const fn in_configuration(
    direction: Direction,
    name: &'static str,
    decode: fn(&mut Decoder) -> Result<()>,
) -> PacketDecoder {
    decoder(CONFIGURATION, direction, name, V1_20_2..=NEWEST, decode)
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

/// An id in `registry`, or a raw id for registries `pomme-protocol` has no
/// table for.
pub fn id(d: &mut Decoder, name: impl Into<Name>, registry: Option<ClientRegistry>) -> Result<i32> {
    match registry {
        Some(registry) => d.registry(name, registry),
        None => d.raw_id(name),
    }
}

/// `ByteBufCodecs.holder`: 0 and the value inline, or a registry id plus one.
pub fn holder(
    d: &mut Decoder,
    name: impl Into<Name>,
    registry: Option<ClientRegistry>,
    direct: impl FnOnce(&mut Decoder) -> Result<()>,
) -> Result<()> {
    let name = name.into();
    d.prefixed(Reader::var_int, |d, holder| match holder {
        0 => d.group(name, direct),
        id => {
            let entry = match registry {
                Some(registry) => d.registry_entry(registry, id - 1),
                None => format!("#{}", id - 1),
            };
            d.note(name, &entry)
        }
    })
}

/// `ByteBufCodecs.holderSet`: 0 and a tag, or the entry count plus one and
/// the entries.
pub fn holder_set(
    d: &mut Decoder,
    name: impl Into<Name>,
    registry: Option<ClientRegistry>,
) -> Result<()> {
    let name = name.into();
    d.prefixed(Reader::var_int, |d, prefix| {
        if prefix == 0 {
            return tag_key(d, name);
        }
        let len = usize::try_from(prefix - 1).map_err(|_| {
            d.reader()
                .error(format!("{prefix} isn't a holder set size"))
        })?;
        d.group(name, |d| {
            for index in 0..len {
                id(d, index.to_string(), registry)?;
            }
            Ok(())
        })?;
        d.show_last(count(len, "item"));
        Ok(())
    })
}

/// `TagKey.streamCodec`: a tag's location, shown with `#`.
pub fn tag_key(d: &mut Decoder, name: impl Into<Name>) -> Result<()> {
    d.read(name, Reader::string, |tag| format!("#{tag}"))?;
    Ok(())
}

/// `SoundEvent.STREAM_CODEC`: a sound event holder.
pub fn sound_event(d: &mut Decoder, name: impl Into<Name>) -> Result<()> {
    holder(d, name, Some(ClientRegistry::SoundEvent), inline_sound)
}

/// A `SoundEvent` itself: a location and an optional fixed range.
pub fn inline_sound(d: &mut Decoder) -> Result<()> {
    d.string("location")?;
    d.optional("fixed_range", |d, name| d.f32(name))?;
    Ok(())
}

/// Three signed bytes, such as a block offset.
pub fn byte_vec3(d: &mut Decoder, name: Name) -> Result<()> {
    d.read(
        name,
        |r| Ok([r.i8()?, r.i8()?, r.i8()?]),
        |[x, y, z]| format!("{x}, {y}, {z}"),
    )
    .map(drop)
}

/// `Vec3i`: three `VarInt`s.
pub fn vec3i(d: &mut Decoder, name: Name) -> Result<()> {
    d.read(
        name,
        |r| Ok([r.var_int()?, r.var_int()?, r.var_int()?]),
        |[x, y, z]| format!("{x}, {y}, {z}"),
    )
    .map(drop)
}

// Readers for `list` and `optional`, which pass each item's name.

pub fn string(d: &mut Decoder, name: Name) -> Result<()> {
    d.string(name).map(drop)
}

pub fn var_int(d: &mut Decoder, name: Name) -> Result<()> {
    d.var_int(name).map(drop)
}

pub fn uuid(d: &mut Decoder, name: Name) -> Result<()> {
    d.uuid(name).map(drop)
}

pub fn raw_id(d: &mut Decoder, name: Name) -> Result<()> {
    d.raw_id(name).map(drop)
}

pub fn nbt_tag(d: &mut Decoder, name: Name) -> Result<()> {
    nbt::root(d, name).map(drop)
}

/// `ByteBufCodecs.OPTIONAL_VAR_INT`, shown as the value or `none`.
pub fn optional_var_int(d: &mut Decoder, name: impl Into<Name>) -> Result<()> {
    d.read(name, Reader::optional_var_int, |value| {
        value.map_or_else(|| "none".into(), |value| value.to_string())
    })?;
    Ok(())
}

/// An enum sent as a byte until protocol `since` sent a `VarInt`.
pub fn byte_enum(d: &mut Decoder, name: &'static str, names: &[&str], since: i32) -> Result<i32> {
    if d.protocol() >= since {
        d.variant(name, Reader::var_int, names)
    } else {
        d.variant(name, Reader::u8, names).map(i32::from)
    }
}

/// An enum sent by `VarInt` id.
pub fn variant(d: &mut Decoder, name: Name, names: &[&str]) -> Result<()> {
    d.variant(name, Reader::var_int, names).map(drop)
}

/// The name of entry `id` in `registry`, for decoding that depends on it.
pub fn registry_name(
    d: &mut Decoder,
    registry: ClientRegistry,
    id: i32,
    what: &str,
) -> Result<&'static str> {
    let protocol = d.protocol();
    u32::try_from(id)
        .ok()
        .and_then(|id| RegistryTable::for_protocol(protocol)?.name_of(registry, id))
        .ok_or_else(|| d.reader().error(format!("{what} #{id} doesn't exist")))
}

/// `GlobalPos`: a dimension and a block position.
pub fn global_pos(d: &mut Decoder, name: Name) -> Result<()> {
    d.group(name, |d| {
        d.string("dimension")?;
        d.block_pos("block_pos")?;
        Ok(())
    })
}

/// `InteractionHand`.
pub const HAND: &[&str] = &["main_hand", "off_hand"];
/// `Difficulty`.
pub const DIFFICULTIES: &[&str] = &["peaceful", "easy", "normal", "hard"];
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
    fn every_packet_has_a_decoder() {
        let mut missing = Vec::new();
        for protocol in protocols(OLDEST..=NEWEST) {
            let table = PacketTable::for_protocol(protocol).unwrap();
            for phase in [
                Phase::Handshake,
                Phase::Status,
                Phase::Login,
                Phase::Configuration,
                Phase::Game,
            ] {
                for direction in [Direction::Serverbound, Direction::Clientbound] {
                    for name in
                        (0..).map_while(|id| names::packet_name(table, phase, direction, id))
                    {
                        if !super::super::has_decoder(protocol, phase, direction, name) {
                            missing.push(format!("{protocol} {phase:?} {direction:?} {name}"));
                        }
                    }
                }
            }
        }
        assert!(missing.is_empty(), "no decoder for {missing:#?}");
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
