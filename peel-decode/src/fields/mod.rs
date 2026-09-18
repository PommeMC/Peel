//! Per-packet field decoders, each a port of a vanilla codec. Decoders are
//! keyed by phase, direction and packet name, and gated to the protocols
//! whose codec they were checked against.

mod decoder;
mod packets;
pub mod reader;

use std::ops::RangeInclusive;

pub use decoder::{Decoder, Field, Name, count};
use peel_trace::{Direction, Phase, Record};
use pomme_protocol::PacketTable;
use reader::{Reader, Result};
use serde::Serialize;

use crate::names;

/// The oldest and newest protocols decoders have been checked against. A
/// version added to `pomme-protocol` stays undecoded until its codecs are
/// diffed and these move.
pub const OLDEST: i32 = 763;
pub const NEWEST: i32 = 777;

pub struct PacketDecoder {
    pub phases: &'static [Phase],
    pub direction: Direction,
    pub name: &'static str,
    pub protocols: RangeInclusive<i32>,
    pub decode: fn(&mut Decoder) -> Result<()>,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Decoded {
    Fields {
        fields: Vec<Field>,
    },
    NoDecoder,
    /// A read failed or bytes were left over. `fields` holds what was read
    /// before.
    Failed {
        fields: Vec<Field>,
        reason: String,
        at: usize,
    },
    /// Decoding reached a codec Peel doesn't port yet at `at`; the rest of the
    /// frame is undecoded rather than misread.
    Partial {
        fields: Vec<Field>,
        reason: String,
        at: usize,
    },
}

impl Decoded {
    pub fn fields(&self) -> &[Field] {
        match self {
            Self::Fields { fields }
            | Self::Failed { fields, .. }
            | Self::Partial { fields, .. } => fields,
            Self::NoDecoder => &[],
        }
    }

    /// Why and where decoding stopped short, if it did.
    pub fn stop_note(&self) -> Option<String> {
        match self {
            Self::Failed { reason, at, .. } => Some(format!("stopped at byte {at}: {reason}")),
            Self::Partial { reason, at, .. } => Some(format!("decoded up to byte {at}: {reason}")),
            Self::Fields { .. } | Self::NoDecoder => None,
        }
    }
}

pub fn decode(record: &Record) -> Decoded {
    decode_with(packets::DECODERS, record)
}

fn decode_with(decoders: &[&[PacketDecoder]], record: &Record) -> Decoded {
    let Some(decoder) = find(decoders, record) else {
        return Decoded::NoDecoder;
    };
    let mut reader = Reader::new(&record.frame);
    if let Err(err) = reader.var_int() {
        return stopped(Vec::new(), err);
    }
    let mut d = Decoder::new(reader, record.protocol);
    let result = (decoder.decode)(&mut d);
    let (reader, fields) = d.into_parts();
    match result {
        Err(err) => stopped(fields, err),
        Ok(()) if reader.remaining() > 0 => stopped(
            fields,
            reader.error(format!(
                "{} left over after the last field",
                count(reader.remaining(), "byte")
            )),
        ),
        Ok(()) => Decoded::Fields { fields },
    }
}

fn stopped(fields: Vec<Field>, err: reader::Error) -> Decoded {
    let (reason, at) = (err.message, err.at);
    if err.unsupported {
        Decoded::Partial { fields, reason, at }
    } else {
        Decoded::Failed { fields, reason, at }
    }
}

/// Whether a packet has a decoder in `protocol`.
pub fn has_decoder(protocol: i32, phase: Phase, direction: Direction, name: &str) -> bool {
    decoder(packets::DECODERS, protocol, phase, direction, name).is_some()
}

fn decoder<'a>(
    decoders: &'a [&[PacketDecoder]],
    protocol: i32,
    phase: Phase,
    direction: Direction,
    name: &str,
) -> Option<&'a PacketDecoder> {
    decoders.iter().flat_map(|group| group.iter()).find(|d| {
        d.name == name
            && d.direction == direction
            && d.phases.contains(&phase)
            && d.protocols.contains(&protocol)
    })
}

fn find<'a>(decoders: &'a [&[PacketDecoder]], record: &Record) -> Option<&'a PacketDecoder> {
    let table = PacketTable::for_protocol(record.protocol)?;
    let name = names::packet_name(table, record.phase, record.direction, record.packet_id)?;
    decoder(
        decoders,
        record.protocol,
        record.phase,
        record.direction,
        name,
    )
}

#[cfg(test)]
pub(crate) mod testing {
    use peel_trace::{Direction, Phase, Record};
    use pomme_protocol::version::VERSIONS;
    use pomme_protocol::{ClientRegistry, PacketTable, RegistryTable};

    use super::{Decoded, Field};
    use crate::test_util::named;

    /// What a packet with no fields decodes to.
    pub const NO_FIELDS: &[&str] = &[];

    pub fn f64s(values: &[f64]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_be_bytes()).collect()
    }

    pub fn f32s(values: &[f32]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_be_bytes()).collect()
    }

    /// `BlockPos.asLong`: x and z masked to 26 bits, y to 12.
    pub fn block_pos(x: i64, y: i64, z: i64) -> [u8; 8] {
        (((x & 0x3FF_FFFF) << 38) | ((z & 0x3FF_FFFF) << 12) | (y & 0xFFF)).to_be_bytes()
    }

    /// How entry `id` of `registry` shows in `protocol`.
    pub fn entry(protocol: i32, registry: ClientRegistry, id: u32) -> String {
        let table = RegistryTable::for_protocol(protocol).unwrap();
        format!("{} (#{id})", table.name_of(registry, id).unwrap())
    }

    /// The id of `name` in `registry`, for ids that fit one `VarInt` byte.
    pub fn registry_id(protocol: i32, registry: ClientRegistry, name: &str) -> u8 {
        let table = RegistryTable::for_protocol(protocol).unwrap();
        let id = table
            .names(registry)
            .iter()
            .position(|n| n == name)
            .unwrap();
        u8::try_from(id).ok().filter(|&id| id < 0x80).unwrap()
    }

    /// Every supported protocol in `range`, once each.
    pub fn protocols(range: std::ops::RangeInclusive<i32>) -> Vec<i32> {
        let mut protocols: Vec<i32> = VERSIONS
            .iter()
            .map(|v| v.protocol)
            .filter(|p| range.contains(p))
            .collect();
        protocols.dedup();
        protocols
    }

    /// `name: value [start..end]` lines, children indented.
    pub fn lines(fields: &[Field]) -> Vec<String> {
        shifted_lines(fields, 0)
    }

    /// [`lines`] with byte ranges moved back by `shift`.
    fn shifted_lines(fields: &[Field], shift: usize) -> Vec<String> {
        let mut lines = Vec::new();
        for field in fields {
            field.walk(0, &mut |depth, f| {
                lines.push(format!(
                    "{}{}: {} [{}..{}]",
                    "  ".repeat(depth),
                    f.name,
                    f.value.as_deref().unwrap_or(""),
                    f.start - shift,
                    f.end - shift
                ));
            });
        }
        lines
    }

    pub fn decode(
        protocol: i32,
        phase: Phase,
        direction: Direction,
        name: &str,
        body: &[u8],
    ) -> Decoded {
        let table = PacketTable::for_protocol(protocol).unwrap();
        let (packet_id, frame) = named(table, phase, direction, name, body);
        super::decode(&Record {
            timestamp_us: 0,
            protocol,
            direction,
            phase,
            packet_id,
            frame,
        })
    }

    /// Decodes `body` as `name` in every protocol of `range` and checks the
    /// fields against `expected`, whose byte ranges count a 1-byte id
    /// whatever the id's real length.
    pub fn assert_decodes(
        range: std::ops::RangeInclusive<i32>,
        phase: Phase,
        direction: Direction,
        name: &str,
        body: &[u8],
        expected: &[impl AsRef<str>],
    ) {
        let expected: Vec<&str> = expected.iter().map(AsRef::as_ref).collect();
        each_protocol(
            range,
            phase,
            direction,
            name,
            body,
            |protocol, shift, decoded| match decoded {
                Decoded::Fields { fields } => assert_eq!(
                    shifted_lines(&fields, shift),
                    expected,
                    "{name} in protocol {protocol}"
                ),
                other => panic!("{name} in protocol {protocol}: {other:?}"),
            },
        );
    }

    /// [`assert_decodes`] for a packet that stops as `Partial` at byte
    /// `stop.0` with reason `stop.1`.
    pub fn assert_partial(
        range: std::ops::RangeInclusive<i32>,
        phase: Phase,
        direction: Direction,
        name: &str,
        body: &[u8],
        expected: &[impl AsRef<str>],
        stop: (usize, &str),
    ) {
        let expected: Vec<&str> = expected.iter().map(AsRef::as_ref).collect();
        each_protocol(
            range,
            phase,
            direction,
            name,
            body,
            |protocol, shift, decoded| match decoded {
                Decoded::Partial { fields, reason, at } => assert_eq!(
                    (shifted_lines(&fields, shift), (at - shift, reason.as_str())),
                    (expected.iter().map(|s| s.to_string()).collect(), stop),
                    "{name} in protocol {protocol}"
                ),
                other => panic!("{name} in protocol {protocol}: {other:?}"),
            },
        );
    }

    /// Decodes `body` as `name` in every protocol of `range`, passing how far
    /// a longer packet id moves byte ranges.
    fn each_protocol(
        range: std::ops::RangeInclusive<i32>,
        phase: Phase,
        direction: Direction,
        name: &str,
        body: &[u8],
        check: impl Fn(i32, usize, Decoded),
    ) {
        for protocol in protocols(range) {
            let table = PacketTable::for_protocol(protocol).unwrap();
            let (_, frame) = named(table, phase, direction, name, body);
            let shift = frame.len() - body.len() - 1;
            check(
                protocol,
                shift,
                decode(protocol, phase, direction, name, body),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use peel_trace::{Direction, Phase};

    use super::testing::lines;
    use super::*;
    use crate::test_util::{latest, named};

    const TEST_DECODERS: &[PacketDecoder] = &[PacketDecoder {
        phases: &[Phase::Configuration, Phase::Game],
        direction: Direction::Clientbound,
        name: "keep_alive",
        protocols: OLDEST..=NEWEST,
        decode: |d| {
            d.group("pair", |d| {
                d.var_int("first")?;
                d.var_int("second")
            })?;
            Ok(())
        },
    }];

    fn record(phase: Phase, direction: Direction, name: &str, body: &[u8]) -> Record {
        let (packet_id, frame) = named(latest(), phase, direction, name, body);
        Record {
            timestamp_us: 0,
            protocol: NEWEST,
            direction,
            phase,
            packet_id,
            frame,
        }
    }

    fn decode_test(record: &Record) -> Decoded {
        decode_with(&[TEST_DECODERS], record)
    }

    #[test]
    fn decodes_with_byte_ranges() {
        let record = record(
            Phase::Game,
            Direction::Clientbound,
            "keep_alive",
            &[1, 0xac, 0x02],
        );
        let Decoded::Fields { fields } = decode_test(&record) else {
            panic!()
        };
        assert_eq!(
            lines(&fields),
            ["pair:  [1..4]", "  first: 1 [1..2]", "  second: 300 [2..4]"]
        );
    }

    #[test]
    fn matches_phase_direction_and_name() {
        for record in [
            record(Phase::Game, Direction::Serverbound, "keep_alive", &[1, 2]),
            record(Phase::Game, Direction::Clientbound, "ping", &[1, 2]),
        ] {
            assert_eq!(decode_test(&record), Decoded::NoDecoder);
        }
    }

    #[test]
    fn keeps_fields_read_before_a_failure() {
        let record = record(
            Phase::Game,
            Direction::Clientbound,
            "keep_alive",
            &[5, 0x80],
        );
        let Decoded::Failed { fields, reason, at } = decode_test(&record) else {
            panic!()
        };
        assert_eq!(lines(&fields), ["pair:  [1..2]", "  first: 5 [1..2]"]);
        assert_eq!(
            (reason.as_str(), at),
            ("VarInt is cut off or longer than 5 bytes", 2)
        );
    }

    #[test]
    fn leftover_bytes_fail() {
        let record = record(
            Phase::Game,
            Direction::Clientbound,
            "keep_alive",
            &[1, 2, 3, 4],
        );
        let Decoded::Failed { reason, at, .. } = decode_test(&record) else {
            panic!()
        };
        assert_eq!(
            (reason.as_str(), at),
            ("2 bytes left over after the last field", 3)
        );
    }

    #[test]
    fn serializes_for_the_viewer() {
        let record = record(Phase::Game, Direction::Clientbound, "keep_alive", &[1, 2]);
        let json = serde_json::to_value(decode_test(&record)).unwrap();
        assert_eq!(json["status"], "fields");
        assert_eq!(json["fields"][0]["children"][1]["value"], "2");
        assert!(json["fields"][0].get("value").is_none());
        let none = serde_json::to_value(Decoded::NoDecoder).unwrap();
        assert_eq!(none, serde_json::json!({ "status": "no_decoder" }));
    }
}
