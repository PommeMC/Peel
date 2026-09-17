//! Clientbound game packets about the world: time, chunks, blocks, sounds
//! and the player's own position.

use peel_trace::Direction::Clientbound;
use pomme_protocol::ClientRegistry;

use super::super::reader::{Error, Reader, Result};
use super::super::{Decoder, NEWEST, OLDEST, PacketDecoder};
use super::components::string_entry;
use super::particles::{particle, particle_options, particle_type};
use super::text::{component, optional_component};
use super::{
    DIFFICULTIES, RELATIVES, V1_20_2, V1_20_3, V1_20_5, V1_21_2, V1_21_4, V1_21_5, V1_21_6,
    V1_21_9, V26_1, V26_3, byte_enum, byte_vec3, game, inline_sound, nbt, position_move_rotation,
    relatives, rotation, sound_event, vec3i,
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
        sound_event(d, "sound")?;
        sound_source(d)?;
        d.fixed_vec3("position", Reader::i32, 8.0)?;
        sound_tail(d)
    }),
    game(Clientbound, "sound_entity", OLDEST..=NEWEST, |d| {
        sound_event(d, "sound")?;
        sound_source(d)?;
        d.var_int("entity_id")?;
        sound_tail(d)
    }),
    game(Clientbound, "level_particles", OLDEST..=NEWEST, |d| {
        let protocol = d.protocol();
        if protocol >= V26_3 {
            particle(d, "particle".into())?;
            d.bool("override_limiter")?;
            d.bool("always_show")?;
            d.vec3("position")?;
            d.vector3f("distance")?;
            d.vector3f("max_speed")?;
            d.var_int("count")?;
            d.variant(
                "randomization",
                Reader::var_int,
                &["default", "alternative", "alternative_with_speed"],
            )?;
            return Ok(());
        }
        let kind = if protocol < V1_20_5 {
            Some(particle_type(d, "particle")?)
        } else {
            None
        };
        d.bool("override_limiter")?;
        if protocol >= V1_21_4 {
            d.bool("always_show")?;
        }
        d.vec3("position")?;
        d.vector3f("distance")?;
        d.f32("max_speed")?;
        d.i32("count")?;
        match kind {
            Some(kind) => d.group("options", |d| particle_options(d, kind)),
            None => particle(d, "particle".into()),
        }
    }),
    game(Clientbound, "explode", OLDEST..=NEWEST, |d| {
        let protocol = d.protocol();
        d.vec3("center")?;
        if protocol >= V1_21_2 {
            if protocol >= V1_21_9 {
                d.f32("radius")?;
                d.i32("block_count")?;
            }
            d.optional("player_knockback", |d, name| d.vec3(name))?;
            particle(d, "explosion_particle".into())?;
            sound_event(d, "explosion_sound")?;
            if protocol >= V1_21_9 {
                d.list("block_particles", |d, name| {
                    d.group(name, |d| {
                        particle(d, "particle".into())?;
                        d.f32("scaling")?;
                        d.f32("speed")?;
                        d.var_int("weight")?;
                        Ok(())
                    })
                })?;
            }
            if protocol >= V26_3 {
                d.bool("play_sound")?;
            }
            return Ok(());
        }
        d.f32("power")?;
        d.list("blocks", byte_vec3)?;
        d.vector3f("player_knockback")?;
        if protocol >= V1_20_3 {
            d.variant(
                "block_interaction",
                Reader::var_int,
                &["keep", "destroy", "destroy_with_decay", "trigger_block"],
            )?;
            particle(d, "small_explosion_particles".into())?;
            particle(d, "large_explosion_particles".into())?;
            if protocol >= V1_20_5 {
                sound_event(d, "explosion_sound")?;
            } else {
                d.group("explosion_sound", inline_sound)?;
            }
        }
        Ok(())
    }),
    game(Clientbound, "block_event", OLDEST..=NEWEST, |d| {
        d.block_pos("block_pos")?;
        d.u8("action")?;
        d.u8("param")?;
        d.raw_id("block")?;
        Ok(())
    }),
    game(Clientbound, "block_entity_data", OLDEST..=NEWEST, |d| {
        d.block_pos("block_pos")?;
        d.registry("type", ClientRegistry::BlockEntityType)?;
        nbt::root(d, "data")?;
        Ok(())
    }),
    game(Clientbound, "map_item_data", OLDEST..=NEWEST, |d| {
        d.var_int("map_id")?;
        d.i8("scale")?;
        d.bool("locked")?;
        d.optional("decorations", |d, name| {
            d.list(name, |d, name| {
                d.group(name, |d| {
                    d.raw_id("type")?;
                    d.i8("x")?;
                    d.i8("y")?;
                    d.i8("rotation")?;
                    optional_component(d, "name")
                })
            })
        })?;
        if d.u8("width")? > 0 {
            d.u8("height")?;
            d.u8("start_x")?;
            d.u8("start_y")?;
            d.byte_array("colors")?;
        }
        Ok(())
    }),
    game(Clientbound, "stop_sound", OLDEST..=NEWEST, |d| {
        let flags = d.flags("flags", Reader::u8, &["source", "sound"])?;
        if flags & 1 != 0 {
            sound_source(d)?;
        }
        if flags & 2 != 0 {
            d.string("sound")?;
        }
        Ok(())
    }),
    game(Clientbound, "set_border_center", OLDEST..=NEWEST, |d| {
        d.f64("new_center_x")?;
        d.f64("new_center_z")?;
        Ok(())
    }),
    // The lerp time is in milliseconds until 1.21.11 and in ticks from it.
    game(Clientbound, "set_border_lerp_size", OLDEST..=NEWEST, |d| {
        d.f64("old_size")?;
        d.f64("new_size")?;
        d.var_long("lerp_time")?;
        Ok(())
    }),
    game(Clientbound, "set_border_size", OLDEST..=NEWEST, |d| {
        d.f64("size")?;
        Ok(())
    }),
    game(
        Clientbound,
        "set_border_warning_delay",
        OLDEST..=NEWEST,
        |d| {
            d.var_int("warning_time")?;
            Ok(())
        },
    ),
    game(
        Clientbound,
        "set_border_warning_distance",
        OLDEST..=NEWEST,
        |d| {
            d.var_int("warning_blocks")?;
            Ok(())
        },
    ),
    game(
        Clientbound,
        "set_chunk_cache_radius",
        OLDEST..=NEWEST,
        |d| {
            d.var_int("radius")?;
            Ok(())
        },
    ),
    game(
        Clientbound,
        "set_simulation_distance",
        OLDEST..=NEWEST,
        |d| {
            d.var_int("simulation_distance")?;
            Ok(())
        },
    ),
    game(Clientbound, "waypoint", V1_21_6..=NEWEST, |d| {
        d.variant(
            "operation",
            Reader::var_int,
            &["track", "untrack", "update"],
        )?;
        d.prefixed(Reader::bool, |d, is_uuid| {
            if is_uuid {
                d.uuid("id").map(drop)
            } else {
                d.string("id").map(drop)
            }
        })?;
        d.group("icon", |d| {
            d.string("style")?;
            d.optional("color", |d, name| {
                d.read(
                    name,
                    |r| Ok([r.u8()?, r.u8()?, r.u8()?]),
                    |[r, g, b]| format!("{r:02x}{g:02x}{b:02x}"),
                )
            })?;
            Ok(())
        })?;
        let kind = d.variant(
            "type",
            Reader::var_int,
            &["empty", "vec3i", "chunk", "azimuth"],
        )?;
        match kind {
            1 => vec3i(d, "position".into())?,
            2 => {
                d.read(
                    "chunk_pos",
                    |r| Ok((r.var_int()?, r.var_int()?)),
                    |(x, z)| format!("{x}, {z}"),
                )?;
            }
            3 => {
                d.f32("angle")?;
            }
            _ => {}
        }
        Ok(())
    }),
    game(
        Clientbound,
        "game_test_highlight_pos",
        V1_21_9..=NEWEST,
        |d| {
            d.block_pos("absolute_pos")?;
            d.block_pos("relative_pos")?;
            Ok(())
        },
    ),
    game(
        Clientbound,
        "test_instance_block_status",
        V1_21_5..=NEWEST,
        |d| {
            component(d, "status")?;
            d.optional("size", vec3i)?;
            Ok(())
        },
    ),
    game(Clientbound, "debug_sample", V1_20_5..=NEWEST, |d| {
        d.long_array("sample")?;
        d.variant("type", Reader::var_int, &["tick_time"])?;
        Ok(())
    }),
    game(Clientbound, "debug/block_value", V1_21_9..=NEWEST, |d| {
        d.block_pos("block_pos")?;
        debug_update(d)
    }),
    game(Clientbound, "debug/chunk_value", V1_21_9..=NEWEST, |d| {
        d.chunk_pos("chunk_pos")?;
        debug_update(d)
    }),
    game(Clientbound, "debug/entity_value", V1_21_9..=NEWEST, |d| {
        d.var_int("entity_id")?;
        debug_update(d)
    }),
    game(Clientbound, "debug/event", V1_21_9..=NEWEST, |d| {
        let subscription = d.raw_id("subscription")?;
        Err(debug_value(d, subscription))
    }),
    game(Clientbound, "game_rule_values", V26_1..=NEWEST, game_rules),
    game(
        Clientbound,
        "low_disk_space_warning",
        V26_1..=NEWEST,
        |_| Ok(()),
    ),
    game(Clientbound, "add_transient_block", V26_3..=NEWEST, |d| {
        d.block_pos("block_pos")?;
        d.raw_id("block_state")?;
        Ok(())
    }),
    game(Clientbound, "initialize_border", OLDEST..=NEWEST, |d| {
        for name in ["new_center_x", "new_center_z", "old_size", "new_size"] {
            d.f64(name)?;
        }
        d.var_long("lerp_time")?;
        for name in ["new_absolute_max_size", "warning_blocks", "warning_time"] {
            d.var_int(name)?;
        }
        Ok(())
    }),
    game(Clientbound, "ticking_state", V1_20_3..=NEWEST, |d| {
        d.f32("tick_rate")?;
        d.bool("frozen")?;
        Ok(())
    }),
    game(Clientbound, "ticking_step", V1_20_3..=NEWEST, |d| {
        d.var_int("tick_steps")?;
        Ok(())
    }),
    game(Clientbound, "change_difficulty", OLDEST..=NEWEST, |d| {
        difficulty(d)?;
        d.bool("locked")?;
        Ok(())
    }),
    game(
        Clientbound,
        "set_carried_item",
        OLDEST..=V1_21_2 - 1,
        held_slot,
    ),
    game(Clientbound, "set_held_slot", V1_21_2..=NEWEST, held_slot),
    game(Clientbound, "player_abilities", OLDEST..=NEWEST, |d| {
        d.flags(
            "flags",
            Reader::u8,
            &["invulnerable", "flying", "may_fly", "instabuild"],
        )?;
        d.f32("flying_speed")?;
        d.f32("walking_speed")?;
        Ok(())
    }),
];

pub fn game_rules(d: &mut Decoder) -> Result<()> {
    d.list("game_rules", |d, name| string_entry(d, name))?;
    Ok(())
}

/// `DebugSubscription.Update`: the subscription, then its value if there is
/// one.
fn debug_update(d: &mut Decoder) -> Result<()> {
    let subscription = d.raw_id("subscription")?;
    d.prefixed(Reader::bool, |d, present| {
        if present {
            Err(debug_value(d, subscription))
        } else {
            d.note("value", "absent")
        }
    })
}

/// A debug value's codec depends on its subscription, a registry
/// `pomme-protocol` has no table for.
fn debug_value(d: &mut Decoder, subscription: i32) -> Error {
    d.reader()
        .unsupported(format!("debug subscription #{subscription}"))
}

pub fn difficulty(d: &mut Decoder) -> Result<()> {
    byte_enum(d, "difficulty", DIFFICULTIES, V1_21_6).map(drop)
}

/// The selected hotbar slot: a byte until 1.21.4 sent a `VarInt`.
fn held_slot(d: &mut Decoder) -> Result<()> {
    if d.protocol() >= V1_21_4 {
        d.var_int("slot")?;
    } else {
        d.i8("slot")?;
    }
    Ok(())
}

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
    use pomme_protocol::ClientRegistry;

    use super::super::super::testing::{
        NO_FIELDS, assert_decodes, assert_partial, block_pos, entry, f32s, f64s, protocols,
        registry_id,
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
            let sound = entry(protocol, ClientRegistry::SoundEvent, 0);
            let body = [
                &[1, 7][..],
                &8i32.to_be_bytes(),
                &(-4i32).to_be_bytes(),
                &16i32.to_be_bytes(),
                &tail,
            ]
            .concat();
            let mut expected = vec![
                format!("sound: {sound} [1..2]"),
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

    #[test]
    fn add_transient_block() {
        check(
            V26_3..=NEWEST,
            "add_transient_block",
            &[&block_pos(1, 2, 3)[..], &[9]].concat(),
            &["block_pos: 1, 2, 3 [1..9]", "block_state: #9 [9..10]"],
        );
    }

    #[test]
    fn initialize_border() {
        check(
            OLDEST..=NEWEST,
            "initialize_border",
            &[&f64s(&[1.0, 2.0, 3.0, 4.0])[..], &[5, 7, 8, 9]].concat(),
            &[
                "new_center_x: 1.0 [1..9]",
                "new_center_z: 2.0 [9..17]",
                "old_size: 3.0 [17..25]",
                "new_size: 4.0 [25..33]",
                "lerp_time: 5 [33..34]",
                "new_absolute_max_size: 7 [34..35]",
                "warning_blocks: 8 [35..36]",
                "warning_time: 9 [36..37]",
            ],
        );
    }

    #[test]
    fn ticking() {
        check(
            V1_20_3..=NEWEST,
            "ticking_state",
            &[&f32s(&[20.0])[..], &[1]].concat(),
            &["tick_rate: 20.0 [1..5]", "frozen: true [5..6]"],
        );
        check(
            V1_20_3..=NEWEST,
            "ticking_step",
            &[3],
            &["tick_steps: 3 [1..2]"],
        );
    }

    #[test]
    fn change_difficulty() {
        let expected = ["difficulty: hard (3) [1..2]", "locked: true [2..3]"];
        check(
            OLDEST..=V1_21_6 - 1,
            "change_difficulty",
            &[3, 1],
            &expected,
        );
        check(V1_21_6..=NEWEST, "change_difficulty", &[3, 1], &expected);
        check(
            OLDEST..=V1_21_6 - 1,
            "change_difficulty",
            &[0x80, 1],
            &["difficulty: unknown (128) [1..2]", "locked: true [2..3]"],
        );
    }

    #[test]
    fn held_slot() {
        check(
            OLDEST..=V1_21_2 - 1,
            "set_carried_item",
            &[8],
            &["slot: 8 [1..2]"],
        );
        check(
            V1_21_2..=V1_21_2,
            "set_held_slot",
            &[8],
            &["slot: 8 [1..2]"],
        );
        check(V1_21_4..=NEWEST, "set_held_slot", &[8], &["slot: 8 [1..2]"]);
    }

    #[test]
    fn player_abilities() {
        check(
            OLDEST..=NEWEST,
            "player_abilities",
            &[&[6][..], &f32s(&[0.05, 0.1])].concat(),
            &[
                "flags: 0x06 flying | may_fly [1..2]",
                "flying_speed: 0.05 [2..6]",
                "walking_speed: 0.1 [6..10]",
            ],
        );
    }

    fn flame(protocol: i32) -> (u8, String) {
        let id = registry_id(protocol, ClientRegistry::ParticleType, "flame");
        (
            id,
            entry(protocol, ClientRegistry::ParticleType, u32::from(id)),
        )
    }

    #[test]
    fn level_particles_by_version() {
        let position = f64s(&[1.0, 2.0, 3.0]);
        let distance = f32s(&[0.5, 0.5, 0.5]);
        for protocol in protocols(OLDEST..=NEWEST) {
            let (id, flame) = flame(protocol);
            let (body, expected): (Vec<u8>, Vec<String>) = if protocol >= V26_3 {
                (
                    [
                        &[id, 1, 0][..],
                        &position,
                        &distance,
                        &f32s(&[0.1, 0.1, 0.1]),
                        &[5, 1],
                    ]
                    .concat(),
                    vec![
                        format!("particle: {flame} [1..2]"),
                        "override_limiter: true [2..3]".into(),
                        "always_show: false [3..4]".into(),
                        "position: 1.0, 2.0, 3.0 [4..28]".into(),
                        "distance: 0.5, 0.5, 0.5 [28..40]".into(),
                        "max_speed: 0.1, 0.1, 0.1 [40..52]".into(),
                        "count: 5 [52..53]".into(),
                        "randomization: alternative (1) [53..54]".into(),
                    ],
                )
            } else {
                let always_show = protocol >= V1_21_4;
                let head: &[u8] = if protocol < V1_20_5 { &[id, 1] } else { &[1] };
                let tail: &[u8] = if protocol < V1_20_5 { &[] } else { &[id] };
                let body = [
                    head,
                    if always_show { &[0] } else { &[] },
                    &position,
                    &distance,
                    &f32s(&[0.1]),
                    &5i32.to_be_bytes(),
                    tail,
                ]
                .concat();
                let mut expected = vec![];
                let mut at = 1;
                if protocol < V1_20_5 {
                    expected.push(format!("particle: {flame} [1..2]"));
                    at = 2;
                }
                expected.push(format!("override_limiter: true [{at}..{}]", at + 1));
                at += 1;
                if always_show {
                    expected.push(format!("always_show: false [{at}..{}]", at + 1));
                    at += 1;
                }
                for (line, len) in [
                    ("position: 1.0, 2.0, 3.0", 24),
                    ("distance: 0.5, 0.5, 0.5", 12),
                    ("max_speed: 0.1", 4),
                    ("count: 5", 4),
                ] {
                    expected.push(format!("{line} [{at}..{}]", at + len));
                    at += len;
                }
                expected.push(if protocol < V1_20_5 {
                    format!("options:  [{at}..{at}]")
                } else {
                    format!("particle: {flame} [{at}..{}]", at + 1)
                });
                (body, expected)
            };
            check(protocol..=protocol, "level_particles", &body, &expected);
        }
    }

    #[test]
    fn explode_by_version() {
        let center = f64s(&[1.0, 2.0, 3.0]);
        let old = [
            &center[..],
            &f32s(&[4.0]),
            &[1, 1, 0xff, 0],
            &f32s(&[0.5, 0.0, 0.0]),
        ]
        .concat();
        let old_lines = [
            "center: 1.0, 2.0, 3.0 [1..25]",
            "power: 4.0 [25..29]",
            "blocks: 1 item [29..33]",
            "  0: 1, -1, 0 [30..33]",
            "player_knockback: 0.5, 0.0, 0.0 [33..45]",
        ];
        check(OLDEST..=V1_20_3 - 1, "explode", &old, &old_lines);
        for protocol in protocols(V1_20_3..=NEWEST) {
            let (id, flame) = flame(protocol);
            let sound = entry(protocol, ClientRegistry::SoundEvent, 0);
            let (body, mut expected): (Vec<u8>, Vec<String>) = if protocol < V1_21_2 {
                let mut lines: Vec<String> =
                    old_lines.iter().map(|line| line.to_string()).collect();
                lines.extend([
                    "block_interaction: keep (0) [45..46]".into(),
                    format!("small_explosion_particles: {flame} [46..47]"),
                    format!("large_explosion_particles: {flame} [47..48]"),
                ]);
                let sound_bytes: &[u8] = if protocol < V1_20_5 {
                    lines.extend([
                        "explosion_sound:  [48..53]".into(),
                        "  location: \"a:b\" [48..52]".into(),
                        "  fixed_range: absent [52..53]".into(),
                    ]);
                    &[3, b'a', b':', b'b', 0]
                } else {
                    lines.push(format!("explosion_sound: {sound} [48..49]"));
                    &[1]
                };
                ([&old[..], &[0, id, id], sound_bytes].concat(), lines)
            } else {
                let mut body = center.clone();
                let mut lines = vec!["center: 1.0, 2.0, 3.0 [1..25]".to_string()];
                let mut at = 25;
                if protocol >= V1_21_9 {
                    body.extend(f32s(&[4.0]));
                    body.extend(9i32.to_be_bytes());
                    lines.extend([
                        "radius: 4.0 [25..29]".into(),
                        "block_count: 9 [29..33]".into(),
                    ]);
                    at = 33;
                }
                body.extend([0, id, 1]);
                lines.extend([
                    format!("player_knockback: absent [{at}..{}]", at + 1),
                    format!("explosion_particle: {flame} [{}..{}]", at + 1, at + 2),
                    format!("explosion_sound: {sound} [{}..{}]", at + 2, at + 3),
                ]);
                (body, lines)
            };
            let mut body = body;
            if protocol >= V1_21_9 {
                body.extend([1, id]);
                body.extend(f32s(&[1.0, 0.5]));
                body.push(2);
                expected.extend([
                    "block_particles: 1 item [36..47]".into(),
                    "  0:  [37..47]".into(),
                    format!("    particle: {flame} [37..38]"),
                    "    scaling: 1.0 [38..42]".into(),
                    "    speed: 0.5 [42..46]".into(),
                    "    weight: 2 [46..47]".into(),
                ]);
            }
            if protocol >= V26_3 {
                body.push(1);
                expected.push("play_sound: true [47..48]".into());
            }
            check(protocol..=protocol, "explode", &body, &expected);
        }
    }

    #[test]
    fn blocks_and_maps() {
        check(
            OLDEST..=NEWEST,
            "block_event",
            &[&block_pos(1, 2, 3)[..], &[1, 2, 40]].concat(),
            &[
                "block_pos: 1, 2, 3 [1..9]",
                "action: 1 [9..10]",
                "param: 2 [10..11]",
                "block: #40 [11..12]",
            ],
        );
        for protocol in protocols(OLDEST..=NEWEST) {
            let tag: &[u8] = if protocol < V1_20_2 {
                &[10, 0, 0, 0]
            } else {
                &[10, 0]
            };
            check(
                protocol..=protocol,
                "block_entity_data",
                &[&block_pos(1, 2, 3)[..], &[7], tag].concat(),
                &[
                    "block_pos: 1, 2, 3 [1..9]".to_string(),
                    format!(
                        "type: {} [9..10]",
                        entry(protocol, ClientRegistry::BlockEntityType, 7)
                    ),
                    format!("data: 0 keys [10..{}]", 10 + tag.len()),
                ],
            );
        }
        check(
            OLDEST..=NEWEST,
            "map_item_data",
            &[3, 2, 1, 1, 1, 4, 1, 0xfe, 3, 0, 2, 1, 0, 5, 2, 9, 9],
            &[
                "map_id: 3 [1..2]",
                "scale: 2 [2..3]",
                "locked: true [3..4]",
                "decorations: 1 item [4..11]",
                "  0:  [6..11]",
                "    type: #4 [6..7]",
                "    x: 1 [7..8]",
                "    y: -2 [8..9]",
                "    rotation: 3 [9..10]",
                "    name: absent [10..11]",
                "width: 2 [11..12]",
                "height: 1 [12..13]",
                "start_x: 0 [13..14]",
                "start_y: 5 [14..15]",
                "colors: 2 bytes [15..18]",
            ],
        );
        check(
            OLDEST..=NEWEST,
            "stop_sound",
            &[3, 7, 3, b'a', b':', b'b'],
            &[
                "flags: 0x03 source | sound [1..2]",
                "source: player (7) [2..3]",
                "sound: \"a:b\" [3..7]",
            ],
        );
    }

    #[test]
    fn border_and_distances() {
        check(
            OLDEST..=NEWEST,
            "set_border_center",
            &f64s(&[1.0, 2.0]),
            &["new_center_x: 1.0 [1..9]", "new_center_z: 2.0 [9..17]"],
        );
        check(
            OLDEST..=NEWEST,
            "set_border_lerp_size",
            &[&f64s(&[1.0, 2.0])[..], &[5]].concat(),
            &[
                "old_size: 1.0 [1..9]",
                "new_size: 2.0 [9..17]",
                "lerp_time: 5 [17..18]",
            ],
        );
        check(
            OLDEST..=NEWEST,
            "set_border_size",
            &f64s(&[3.0]),
            &["size: 3.0 [1..9]"],
        );
        check(
            OLDEST..=NEWEST,
            "set_border_warning_delay",
            &[15],
            &["warning_time: 15 [1..2]"],
        );
        check(
            OLDEST..=NEWEST,
            "set_border_warning_distance",
            &[5],
            &["warning_blocks: 5 [1..2]"],
        );
        check(
            OLDEST..=NEWEST,
            "set_chunk_cache_radius",
            &[10],
            &["radius: 10 [1..2]"],
        );
        check(
            OLDEST..=NEWEST,
            "set_simulation_distance",
            &[8],
            &["simulation_distance: 8 [1..2]"],
        );
    }

    #[test]
    fn waypoints() {
        check(
            V1_21_6..=NEWEST,
            "waypoint",
            &[
                0, 0, 1, b'w', 3, b'a', b':', b'b', 1, 0xff, 0x80, 0, 1, 1, 2, 3,
            ],
            &[
                "operation: track (0) [1..2]",
                "id: \"w\" [2..5]",
                "icon:  [5..13]",
                "  style: \"a:b\" [5..9]",
                "  color: ff8000 [9..13]",
                "type: vec3i (1) [13..14]",
                "position: 1, 2, 3 [14..17]",
            ],
        );
        check(
            V1_21_6..=NEWEST,
            "waypoint",
            &[&[2, 1][..], &[0x12; 16], &[3, b'a', b':', b'b', 0, 2, 5, 6]].concat(),
            &[
                "operation: update (2) [1..2]",
                "id: 12121212-1212-1212-1212-121212121212 [2..19]",
                "icon:  [19..24]",
                "  style: \"a:b\" [19..23]",
                "  color: absent [23..24]",
                "type: chunk (2) [24..25]",
                "chunk_pos: 5, 6 [25..27]",
            ],
        );
    }

    #[test]
    fn tests_and_debug() {
        check(
            V1_21_9..=NEWEST,
            "game_test_highlight_pos",
            &[&block_pos(1, 2, 3)[..], &block_pos(0, 1, 0)].concat(),
            &[
                "absolute_pos: 1, 2, 3 [1..9]",
                "relative_pos: 0, 1, 0 [9..17]",
            ],
        );
        check(
            V1_21_5..=NEWEST,
            "test_instance_block_status",
            &[8, 0, 2, b'o', b'k', 1, 1, 2, 3],
            &["status: \"ok\" [1..6]", "size: 1, 2, 3 [6..10]"],
        );
        check(
            V1_20_5..=NEWEST,
            "debug_sample",
            &[&[2][..], &1i64.to_be_bytes(), &2i64.to_be_bytes(), &[0]].concat(),
            &["sample: 2 longs [1..18]", "type: tick_time (0) [18..19]"],
        );
        let pos = block_pos(1, 2, 3);
        check(
            V1_21_9..=NEWEST,
            "debug/block_value",
            &[&pos[..], &[4, 0]].concat(),
            &[
                "block_pos: 1, 2, 3 [1..9]",
                "subscription: #4 [9..10]",
                "value: absent [10..11]",
            ],
        );
        assert_partial(
            V1_21_9..=NEWEST,
            Game,
            Clientbound,
            "debug/block_value",
            &[&pos[..], &[4, 1]].concat(),
            &["block_pos: 1, 2, 3 [1..9]", "subscription: #4 [9..10]"],
            (11, "debug subscription #4 isn't decoded yet"),
        );
        check(
            V1_21_9..=NEWEST,
            "debug/chunk_value",
            &[&((2i64 << 32) | 1).to_be_bytes()[..], &[4, 0]].concat(),
            &[
                "chunk_pos: 1, 2 [1..9]",
                "subscription: #4 [9..10]",
                "value: absent [10..11]",
            ],
        );
        check(
            V1_21_9..=NEWEST,
            "debug/entity_value",
            &[7, 4, 0],
            &[
                "entity_id: 7 [1..2]",
                "subscription: #4 [2..3]",
                "value: absent [3..4]",
            ],
        );
        assert_partial(
            V1_21_9..=NEWEST,
            Game,
            Clientbound,
            "debug/event",
            &[4],
            &["subscription: #4 [1..2]"],
            (2, "debug subscription #4 isn't decoded yet"),
        );
    }

    #[test]
    fn low_disk_space_warning() {
        check(V26_1..=NEWEST, "low_disk_space_warning", &[], NO_FIELDS);
    }

    #[test]
    fn game_rule_values() {
        check(
            V26_1..=NEWEST,
            "game_rule_values",
            &[1, 3, b'a', b':', b'b', 1, b'5'],
            &["game_rules: 1 item [1..8]", "  0: a:b: 5 [2..8]"],
        );
    }
}
