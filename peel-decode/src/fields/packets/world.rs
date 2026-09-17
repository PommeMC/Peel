//! Clientbound game packets about the world: time, chunks, blocks, sounds
//! and the player's own position.

use peel_trace::Direction::Clientbound;
use pomme_protocol::ClientRegistry;

use super::super::reader::{Reader, Result};
use super::super::{Decoder, NEWEST, OLDEST, PacketDecoder};
use super::{
    RELATIVES, V1_20_2, V1_20_3, V1_21_2, V1_21_6, V1_21_9, V26_1, game, position_move_rotation,
    relatives, rotation,
};

pub static DECODERS: &[PacketDecoder] = &[
    game(Clientbound, "player_position", OLDEST..=NEWEST, |d| {
        if d.protocol() >= V1_21_2 {
            d.var_int("teleport_id")?;
            position_move_rotation(d)?;
            relatives(d)?;
        } else {
            d.vec3("position")?;
            rotation(d)?;
            d.flags("relatives", Reader::u8, &RELATIVES[..5])?;
            d.var_int("teleport_id")?;
        }
        Ok(())
    }),
    game(Clientbound, "set_time", OLDEST..=NEWEST, |d| {
        d.i64("game_time")?;
        if d.protocol() >= V26_1 {
            d.list("clocks", |d, name| {
                d.group(name, |d| {
                    d.raw_id("clock")?;
                    d.var_long("total_ticks")?;
                    d.f32("partial_tick")?;
                    d.f32("rate")?;
                    Ok(())
                })
            })?;
        } else {
            d.i64("day_time")?;
            if d.protocol() >= V1_21_2 {
                d.bool("tick_day_time")?;
            }
        }
        Ok(())
    }),
    game(Clientbound, "game_event", OLDEST..=NEWEST, |d| {
        let known = match d.protocol() {
            V1_20_3.. => 14,
            V1_20_2.. => 13,
            _ => 12,
        };
        d.variant("event", Reader::u8, &GAME_EVENTS[..known])?;
        d.f32("param")?;
        Ok(())
    }),
    game(Clientbound, "block_update", OLDEST..=NEWEST, |d| {
        d.block_pos("block_pos")?;
        d.raw_id("block_state")?;
        Ok(())
    }),
    game(Clientbound, "forget_level_chunk", OLDEST..=NEWEST, |d| {
        if d.protocol() >= V1_20_2 {
            d.chunk_pos("chunk_pos")?;
        } else {
            d.i32("x")?;
            d.i32("z")?;
        }
        Ok(())
    }),
    game(
        Clientbound,
        "set_chunk_cache_center",
        OLDEST..=NEWEST,
        |d| {
            d.var_int("x")?;
            d.var_int("z")?;
            Ok(())
        },
    ),
    game(Clientbound, "chunk_batch_start", V1_20_2..=NEWEST, |_| {
        Ok(())
    }),
    game(Clientbound, "chunk_batch_finished", V1_20_2..=NEWEST, |d| {
        d.var_int("batch_size")?;
        Ok(())
    }),
    game(Clientbound, "bundle_delimiter", OLDEST..=NEWEST, |_| Ok(())),
    game(
        Clientbound,
        "set_default_spawn_position",
        OLDEST..=NEWEST,
        |d| {
            if d.protocol() >= V1_21_9 {
                d.string("dimension")?;
                d.block_pos("block_pos")?;
                d.f32("yaw")?;
                d.f32("pitch")?;
            } else {
                d.block_pos("block_pos")?;
                d.f32("angle")?;
            }
            Ok(())
        },
    ),
    game(Clientbound, "block_changed_ack", OLDEST..=NEWEST, |d| {
        d.var_int("sequence")?;
        Ok(())
    }),
    game(Clientbound, "block_destruction", OLDEST..=NEWEST, |d| {
        d.var_int("breaker_id")?;
        d.block_pos("block_pos")?;
        d.u8("progress")?;
        Ok(())
    }),
    game(Clientbound, "level_event", OLDEST..=NEWEST, |d| {
        d.i32("type")?;
        d.block_pos("block_pos")?;
        d.i32("data")?;
        d.bool("global")?;
        Ok(())
    }),
    game(Clientbound, "sound", OLDEST..=NEWEST, |d| {
        sound_event(d)?;
        sound_source(d)?;
        d.fixed_vec3("position", Reader::i32, 8.0)?;
        sound_tail(d)
    }),
    game(Clientbound, "sound_entity", OLDEST..=NEWEST, |d| {
        sound_event(d)?;
        sound_source(d)?;
        d.var_int("entity_id")?;
        sound_tail(d)
    }),
];

/// `ClientboundGameEventPacket` types by id; 1.20.2 and 1.20.3 each added one.
const GAME_EVENTS: &[&str] = &[
    "no_respawn_block_available",
    "start_raining",
    "stop_raining",
    "change_game_mode",
    "win_game",
    "demo_event",
    "play_arrow_hit_sound",
    "rain_level_change",
    "thunder_level_change",
    "puffer_fish_sting",
    "guardian_elder_effect",
    "immediate_respawn",
    "limited_crafting",
    "level_chunks_load_start",
];

/// A `sound_event` holder: 0 and an inline event, or a registry id plus one.
fn sound_event(d: &mut Decoder) -> Result<()> {
    d.prefixed(Reader::var_int, |d, holder| match holder {
        0 => d.group("sound", |d| {
            d.string("location")?;
            d.optional("fixed_range", |d, name| d.f32(name))?;
            Ok(())
        }),
        id => {
            let entry = d.registry_entry(ClientRegistry::SoundEvent, id - 1);
            d.note("sound", &entry)
        }
    })
}

/// `SoundSource`; 1.21.6 added `ui`.
fn sound_source(d: &mut Decoder) -> Result<()> {
    let sources: &[&str] = &[
        "master", "music", "record", "weather", "block", "hostile", "neutral", "player", "ambient",
        "voice", "ui",
    ];
    let known = if d.protocol() >= V1_21_6 { 11 } else { 10 };
    d.variant("source", Reader::var_int, &sources[..known])?;
    Ok(())
}

fn sound_tail(d: &mut Decoder) -> Result<()> {
    d.f32("volume")?;
    d.f32("pitch")?;
    d.i64("seed")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::ops::RangeInclusive;

    use peel_trace::Phase::Game;
    use pomme_protocol::RegistryTable;

    use super::super::super::testing::{
        NO_FIELDS, assert_decodes, block_pos, f32s, f64s, protocols,
    };
    use super::*;

    fn check(range: RangeInclusive<i32>, name: &str, body: &[u8], expected: &[impl AsRef<str>]) {
        assert_decodes(range, Game, Clientbound, name, body, expected);
    }

    #[test]
    fn player_position() {
        check(
            OLDEST..=V1_21_2 - 1,
            "player_position",
            &[&f64s(&[1.0, 2.0, 3.0])[..], &f32s(&[90.0, 0.0]), &[0x18, 5]].concat(),
            &[
                "position: 1.0, 2.0, 3.0 [1..25]",
                "y_rot: 90.0 [25..29]",
                "x_rot: 0.0 [29..33]",
                "relatives: 0x18 y_rot | x_rot [33..34]",
                "teleport_id: 5 [34..35]",
            ],
        );
        check(
            V1_21_2..=NEWEST,
            "player_position",
            &[
                &[5][..],
                &f64s(&[1.0, 2.0, 3.0, 0.0, 0.0, 0.0]),
                &f32s(&[90.0, 0.0]),
                &0x100i32.to_be_bytes(),
            ]
            .concat(),
            &[
                "teleport_id: 5 [1..2]",
                "position: 1.0, 2.0, 3.0 [2..26]",
                "delta_movement: 0.0, 0.0, 0.0 [26..50]",
                "y_rot: 90.0 [50..54]",
                "x_rot: 0.0 [54..58]",
                "relatives: 0x100 rotate_delta [58..62]",
            ],
        );
    }

    #[test]
    fn set_time_by_version() {
        let times = [&100i64.to_be_bytes()[..], &6000i64.to_be_bytes()].concat();
        let old = ["game_time: 100 [1..9]", "day_time: 6000 [9..17]"];
        check(OLDEST..=V1_21_2 - 1, "set_time", &times, &old);
        check(
            V1_21_2..=V26_1 - 1,
            "set_time",
            &[&times[..], &[1]].concat(),
            &[&old[..], &["tick_day_time: true [17..18]"]].concat(),
        );
        check(
            V26_1..=NEWEST,
            "set_time",
            &[
                &100i64.to_be_bytes()[..],
                &[1, 0, 0xe8, 0x07],
                &f32s(&[0.5, 1.0]),
            ]
            .concat(),
            &[
                "game_time: 100 [1..9]",
                "clocks: 1 item [9..21]",
                "  0:  [10..21]",
                "    clock: #0 [10..11]",
                "    total_ticks: 1000 [11..13]",
                "    partial_tick: 0.5 [13..17]",
                "    rate: 1.0 [17..21]",
            ],
        );
    }

    #[test]
    fn game_events_by_version() {
        let body = [&[13][..], &f32s(&[0.0])].concat();
        for (range, event) in [
            (OLDEST..=V1_20_2, "unknown"),
            (V1_20_3..=NEWEST, "level_chunks_load_start"),
        ] {
            let first = format!("event: {event} (13) [1..2]");
            check(range, "game_event", &body, &[&first, "param: 0.0 [2..6]"]);
        }
    }

    #[test]
    fn chunks_and_blocks() {
        let packed = ((-5i64) << 32 | 7).to_be_bytes();
        check(
            V1_20_2..=NEWEST,
            "forget_level_chunk",
            &packed,
            &["chunk_pos: 7, -5 [1..9]"],
        );
        check(
            OLDEST..=OLDEST,
            "forget_level_chunk",
            &[&7i32.to_be_bytes()[..], &(-5i32).to_be_bytes()].concat(),
            &["x: 7 [1..5]", "z: -5 [5..9]"],
        );
        check(
            OLDEST..=NEWEST,
            "set_chunk_cache_center",
            &[3, 0x7f],
            &["x: 3 [1..2]", "z: 127 [2..3]"],
        );
        check(V1_20_2..=NEWEST, "chunk_batch_start", &[], NO_FIELDS);
        check(
            V1_20_2..=NEWEST,
            "chunk_batch_finished",
            &[9],
            &["batch_size: 9 [1..2]"],
        );
        check(OLDEST..=NEWEST, "bundle_delimiter", &[], NO_FIELDS);
        check(
            OLDEST..=NEWEST,
            "block_update",
            &[&block_pos(1, -2, 3)[..], &[0x80, 0x01]].concat(),
            &["block_pos: 1, -2, 3 [1..9]", "block_state: #128 [9..11]"],
        );
        check(
            OLDEST..=NEWEST,
            "block_changed_ack",
            &[4],
            &["sequence: 4 [1..2]"],
        );
        check(
            OLDEST..=NEWEST,
            "block_destruction",
            &[&[2][..], &block_pos(0, 0, 0), &[9]].concat(),
            &[
                "breaker_id: 2 [1..2]",
                "block_pos: 0, 0, 0 [2..10]",
                "progress: 9 [10..11]",
            ],
        );
        check(
            OLDEST..=NEWEST,
            "level_event",
            &[
                &2001i32.to_be_bytes()[..],
                &block_pos(0, 64, 0),
                &1i32.to_be_bytes(),
                &[0],
            ]
            .concat(),
            &[
                "type: 2001 [1..5]",
                "block_pos: 0, 64, 0 [5..13]",
                "data: 1 [13..17]",
                "global: false [17..18]",
            ],
        );
    }

    #[test]
    fn default_spawn_gains_a_dimension_in_1_21_9() {
        let pos = block_pos(0, 70, 0);
        check(
            OLDEST..=V1_21_9 - 1,
            "set_default_spawn_position",
            &[&pos[..], &f32s(&[0.0])].concat(),
            &["block_pos: 0, 70, 0 [1..9]", "angle: 0.0 [9..13]"],
        );
        check(
            V1_21_9..=NEWEST,
            "set_default_spawn_position",
            &[&[9][..], b"overworld", &pos, &f32s(&[0.0, 0.0])].concat(),
            &[
                "dimension: \"overworld\" [1..11]",
                "block_pos: 0, 70, 0 [11..19]",
                "yaw: 0.0 [19..23]",
                "pitch: 0.0 [23..27]",
            ],
        );
    }

    #[test]
    fn sounds() {
        let tail = [&f32s(&[1.0, 0.5])[..], &3i64.to_be_bytes()].concat();
        let tail_lines = |at: usize| {
            [
                format!("volume: 1.0 [{at}..{}]", at + 4),
                format!("pitch: 0.5 [{}..{}]", at + 4, at + 8),
                format!("seed: 3 [{}..{}]", at + 8, at + 16),
            ]
        };
        for protocol in protocols(OLDEST..=NEWEST) {
            let name = RegistryTable::for_protocol(protocol)
                .unwrap()
                .name_of(ClientRegistry::SoundEvent, 0)
                .unwrap();
            let body = [
                &[1, 7][..],
                &8i32.to_be_bytes(),
                &(-4i32).to_be_bytes(),
                &16i32.to_be_bytes(),
                &tail,
            ]
            .concat();
            let mut expected = vec![
                format!("sound: {name} (#0) [1..2]"),
                "source: player (7) [2..3]".into(),
                "position: 1.0, -0.5, 2.0 [3..15]".into(),
            ];
            expected.extend(tail_lines(15));
            check(protocol..=protocol, "sound", &body, &expected);
        }
        let inline = [&[0, 3][..], b"a:b", &[1], &f32s(&[16.0]), &[10, 4], &tail].concat();
        let mut expected = vec![
            "sound:  [1..11]".to_string(),
            "  location: \"a:b\" [2..6]".into(),
            "  fixed_range: 16.0 [6..11]".into(),
            "source: ui (10) [11..12]".into(),
            "entity_id: 4 [12..13]".into(),
        ];
        expected.extend(tail_lines(13));
        check(V1_21_6..=NEWEST, "sound_entity", &inline, &expected);
    }
}
