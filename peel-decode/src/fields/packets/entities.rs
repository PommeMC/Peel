//! Clientbound game packets about entities: spawning, movement and state.

use peel_trace::Direction::Clientbound;
use pomme_protocol::ClientRegistry;

use super::super::reader::{Reader, Result};
use super::super::{Decoder, NEWEST, OLDEST, PacketDecoder, count};
use super::{
    HAND, V1_20_5, V1_21_2, V1_21_9, V26_3, game, nbt, position_move_rotation, relatives, rotation,
};

pub static DECODERS: &[PacketDecoder] = &[
    game(Clientbound, "add_entity", OLDEST..=NEWEST, |d| {
        d.var_int("entity_id")?;
        d.uuid("uuid")?;
        d.registry("entity_type", ClientRegistry::EntityType)?;
        d.vec3("position")?;
        if d.protocol() >= V1_21_9 {
            d.lp_vec3("movement")?;
        }
        d.angle("x_rot")?;
        d.angle("y_rot")?;
        d.angle("y_head_rot")?;
        d.var_int("data")?;
        if d.protocol() < V1_21_9 {
            velocity(d)?;
        }
        Ok(())
    }),
    game(Clientbound, "add_player", OLDEST..=OLDEST, |d| {
        d.var_int("entity_id")?;
        d.uuid("uuid")?;
        d.vec3("position")?;
        angles(d)
    }),
    game(Clientbound, "remove_entities", OLDEST..=NEWEST, |d| {
        d.list("entity_ids", |d, name| d.var_int(name).map(drop))?;
        Ok(())
    }),
    game(Clientbound, "set_entity_motion", OLDEST..=NEWEST, |d| {
        d.var_int("entity_id")?;
        if d.protocol() >= V1_21_9 {
            d.lp_vec3("movement")?;
        } else {
            velocity(d)?;
        }
        Ok(())
    }),
    game(Clientbound, "move_entity_pos", OLDEST..=NEWEST, |d| {
        d.var_int("entity_id")?;
        if d.protocol() >= V26_3 {
            stepped_delta(d)?;
        } else {
            delta(d)?;
            d.bool("on_ground")?;
        }
        Ok(())
    }),
    game(Clientbound, "move_entity_pos_rot", OLDEST..=NEWEST, |d| {
        d.var_int("entity_id")?;
        if d.protocol() >= V26_3 {
            stepped_delta(d)?;
            angles(d)?;
        } else {
            delta(d)?;
            angles(d)?;
            d.bool("on_ground")?;
        }
        Ok(())
    }),
    game(Clientbound, "move_entity_rot", OLDEST..=NEWEST, |d| {
        d.var_int("entity_id")?;
        if d.protocol() >= V26_3 {
            d.bool("on_ground")?;
            angles(d)?;
        } else {
            angles(d)?;
            d.bool("on_ground")?;
        }
        Ok(())
    }),
    game(Clientbound, "rotate_head", OLDEST..=NEWEST, |d| {
        d.var_int("entity_id")?;
        d.angle("y_head_rot")?;
        Ok(())
    }),
    game(Clientbound, "entity_position_sync", V1_21_2..=NEWEST, |d| {
        d.var_int("entity_id")?;
        if d.protocol() >= V26_3 {
            position_path(d)?;
            rotation(d)?;
        } else {
            position_move_rotation(d)?;
        }
        d.bool("on_ground")?;
        Ok(())
    }),
    game(Clientbound, "teleport_entity", OLDEST..=NEWEST, |d| {
        d.var_int("entity_id")?;
        if d.protocol() >= V1_21_2 {
            position_move_rotation(d)?;
            relatives(d)?;
        } else {
            d.vec3("position")?;
            angles(d)?;
        }
        d.bool("on_ground")?;
        Ok(())
    }),
    game(Clientbound, "set_entity_link", OLDEST..=NEWEST, |d| {
        d.i32("source_id")?;
        d.i32("dest_id")?;
        Ok(())
    }),
    game(Clientbound, "set_passengers", OLDEST..=NEWEST, |d| {
        d.var_int("vehicle_id")?;
        d.list("passenger_ids", |d, name| d.var_int(name).map(drop))?;
        Ok(())
    }),
    game(Clientbound, "take_item_entity", OLDEST..=NEWEST, |d| {
        d.var_int("item_id")?;
        d.var_int("player_id")?;
        d.var_int("amount")?;
        Ok(())
    }),
    game(Clientbound, "entity_event", OLDEST..=NEWEST, |d| {
        d.i32("entity_id")?;
        d.i8("event")?;
        Ok(())
    }),
    game(Clientbound, "hurt_animation", OLDEST..=NEWEST, |d| {
        d.var_int("entity_id")?;
        d.f32("yaw")?;
        Ok(())
    }),
    game(Clientbound, "animate", OLDEST..=NEWEST, |d| {
        d.var_int("entity_id")?;
        let actions: &[&str] = if d.protocol() >= V26_3 {
            &["wake_up", "critical_hit", "magic_critical_hit"]
        } else {
            &[
                "swing_main_hand",
                "",
                "wake_up",
                "swing_off_hand",
                "critical_hit",
                "magic_critical_hit",
            ]
        };
        d.variant("action", Reader::u8, actions)?;
        Ok(())
    }),
    game(Clientbound, "swing_animation", V26_3..=NEWEST, |d| {
        d.var_int("entity_id")?;
        d.variant("hand", Reader::var_int, HAND)?;
        d.variant("type", Reader::var_int, &["none", "whack", "stab"])?;
        d.var_int("duration")?;
        Ok(())
    }),
    game(Clientbound, "damage_event", OLDEST..=NEWEST, |d| {
        d.var_int("entity_id")?;
        d.raw_id("damage_type")?;
        for name in ["cause_id", "direct_id"] {
            d.read(name, Reader::var_int, |&id| match id {
                0 => "none".into(),
                id => (id - 1).to_string(),
            })?;
        }
        d.optional("source_position", |d, name| d.vec3(name))?;
        Ok(())
    }),
    game(Clientbound, "update_mob_effect", OLDEST..=NEWEST, |d| {
        d.var_int("entity_id")?;
        d.raw_id("effect")?;
        if d.protocol() >= V1_20_5 {
            d.var_int("amplifier")?;
        } else {
            d.i8("amplifier")?;
        }
        d.var_int("duration")?;
        d.flags(
            "flags",
            Reader::u8,
            &["ambient", "visible", "show_icon", "blend"],
        )?;
        if d.protocol() < V1_20_5 {
            d.optional("factor_data", |d, name| nbt::root(d, name).map(drop))?;
        }
        Ok(())
    }),
    game(Clientbound, "remove_mob_effect", OLDEST..=NEWEST, |d| {
        d.var_int("entity_id")?;
        d.raw_id("effect")?;
        Ok(())
    }),
    game(Clientbound, "set_health", OLDEST..=NEWEST, |d| {
        d.f32("health")?;
        d.var_int("food")?;
        d.f32("saturation")?;
        Ok(())
    }),
    game(Clientbound, "set_experience", OLDEST..=NEWEST, |d| {
        d.f32("progress")?;
        d.var_int("level")?;
        d.var_int("total")?;
        Ok(())
    }),
];

/// Velocity in 1/8000 blocks per tick, as shorts until 1.21.9 sent `LpVec3`.
fn velocity(d: &mut Decoder) -> Result<()> {
    d.fixed_vec3("velocity", Reader::i16, 8000.0)?;
    Ok(())
}

/// A position change in 1/4096 blocks.
fn delta(d: &mut Decoder) -> Result<()> {
    d.fixed_vec3("delta", Reader::i16, 4096.0)?;
    Ok(())
}

/// 26.3's movement: packed on-ground and step count, then one delta or that
/// many timed steps (`VecDelta`).
fn stepped_delta(d: &mut Decoder) -> Result<()> {
    let properties = d.read("properties", Reader::var_int, |&p| {
        format!("on_ground: {}, steps: {}", p & 1 == 1, (p as u32) >> 1)
    })?;
    let steps = (properties as u32) >> 1;
    if steps == 0 {
        return delta(d);
    }
    d.group("steps", |d| {
        for step in 0..steps {
            d.group(step.to_string(), |d| {
                d.var_int("ticks")?;
                delta(d)
            })?;
        }
        Ok(())
    })?;
    d.show_last(count(steps as usize, "step"));
    Ok(())
}

fn angles(d: &mut Decoder) -> Result<()> {
    d.angle("y_rot")?;
    d.angle("x_rot")?;
    Ok(())
}

/// `PositionPath`: a straight move, or timed steps.
fn position_path(d: &mut Decoder) -> Result<()> {
    if d.variant("path", Reader::var_int, &["linear", "stepped"])? == 0 {
        d.vec3("position")?;
    } else {
        d.list("steps", |d, name| {
            d.group(name, |d| {
                d.vec3("position")?;
                d.var_int("tick_offset")?;
                Ok(())
            })
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::ops::RangeInclusive;

    use peel_trace::Phase::Game;
    use pomme_protocol::RegistryTable;

    use super::super::super::testing::{assert_decodes, f32s, f64s, protocols};
    use super::super::V1_20_2;
    use super::*;

    fn check(range: RangeInclusive<i32>, name: &str, body: &[u8], expected: &[impl AsRef<str>]) {
        assert_decodes(range, Game, Clientbound, name, body, expected);
    }

    const UUID: [u8; 16] = [0xab; 16];

    #[test]
    fn add_entity_moves_velocity_in_1_21_9() {
        let head = [&[7][..], &UUID, &[1], &f64s(&[0.5, 64.0, -3.5])].concat();
        let rotations = [64, 0xc0, 0, 0];
        let velocity = [
            &4000i16.to_be_bytes()[..],
            &(-2000i16).to_be_bytes(),
            &[0, 0],
        ]
        .concat();
        let older = [&head[..], &rotations, &velocity].concat();
        let newer = [&head[..], &[0], &rotations].concat();
        for (range, body, lp) in [
            (OLDEST..=V1_21_9 - 1, older, false),
            (V1_21_9..=NEWEST, newer, true),
        ] {
            for protocol in protocols(range) {
                let table = RegistryTable::for_protocol(protocol).unwrap();
                let entity_type = table.name_of(ClientRegistry::EntityType, 1).unwrap();
                let at = if lp { 44 } else { 43 };
                let mut expected = vec![
                    "entity_id: 7 [1..2]".to_string(),
                    "uuid: abababab-abab-abab-abab-abababababab [2..18]".into(),
                    format!("entity_type: {entity_type} (#1) [18..19]"),
                    "position: 0.5, 64.0, -3.5 [19..43]".into(),
                ];
                if lp {
                    expected.push("movement: 0.0, 0.0, 0.0 [43..44]".into());
                }
                expected.extend([
                    format!("x_rot: 90.0° [{at}..{}]", at + 1),
                    format!("y_rot: -90.0° [{}..{}]", at + 1, at + 2),
                    format!("y_head_rot: 0.0° [{}..{}]", at + 2, at + 3),
                    format!("data: 0 [{}..{}]", at + 3, at + 4),
                ]);
                if !lp {
                    expected.push("velocity: 0.5, -0.25, 0.0 [47..53]".into());
                }
                check(protocol..=protocol, "add_entity", &body, &expected);
            }
        }
    }

    #[test]
    fn motion_and_head() {
        check(
            OLDEST..=V1_21_9 - 1,
            "set_entity_motion",
            &[&[3][..], &8000i16.to_be_bytes(), &[0, 0, 0, 0]].concat(),
            &["entity_id: 3 [1..2]", "velocity: 1.0, 0.0, 0.0 [2..8]"],
        );
        check(
            V1_21_9..=NEWEST,
            "set_entity_motion",
            &[3, 0],
            &["entity_id: 3 [1..2]", "movement: 0.0, 0.0, 0.0 [2..3]"],
        );
        check(
            OLDEST..=NEWEST,
            "rotate_head",
            &[3, 32],
            &["entity_id: 3 [1..2]", "y_head_rot: 45.0° [2..3]"],
        );
        check(
            OLDEST..=NEWEST,
            "remove_entities",
            &[2, 1, 2],
            &[
                "entity_ids: 2 items [1..4]",
                "  0: 1 [2..3]",
                "  1: 2 [3..4]",
            ],
        );
    }

    #[test]
    fn move_entity_before_26_3() {
        let delta = [
            &4096i16.to_be_bytes()[..],
            &(-2048i16).to_be_bytes(),
            &[0, 0],
        ]
        .concat();
        check(
            V1_20_2..=V26_3 - 1,
            "move_entity_pos",
            &[&[9][..], &delta, &[1]].concat(),
            &[
                "entity_id: 9 [1..2]",
                "delta: 1.0, -0.5, 0.0 [2..8]",
                "on_ground: true [8..9]",
            ],
        );
        check(
            OLDEST..=V26_3 - 1,
            "move_entity_pos_rot",
            &[&[9][..], &delta, &[64, 0, 0]].concat(),
            &[
                "entity_id: 9 [1..2]",
                "delta: 1.0, -0.5, 0.0 [2..8]",
                "y_rot: 90.0° [8..9]",
                "x_rot: 0.0° [9..10]",
                "on_ground: false [10..11]",
            ],
        );
        check(
            OLDEST..=V26_3 - 1,
            "move_entity_rot",
            &[9, 64, 0, 1],
            &[
                "entity_id: 9 [1..2]",
                "y_rot: 90.0° [2..3]",
                "x_rot: 0.0° [3..4]",
                "on_ground: true [4..5]",
            ],
        );
    }

    #[test]
    fn move_entity_in_26_3() {
        let delta = [&4096i16.to_be_bytes()[..], &[0, 0, 0, 0]].concat();
        check(
            V26_3..=NEWEST,
            "move_entity_pos",
            &[&[9, 1][..], &delta].concat(),
            &[
                "entity_id: 9 [1..2]",
                "properties: on_ground: true, steps: 0 [2..3]",
                "delta: 1.0, 0.0, 0.0 [3..9]",
            ],
        );
        check(
            V26_3..=NEWEST,
            "move_entity_pos_rot",
            &[&[9, 4][..], &[1], &delta, &[3], &delta, &[64, 0]].concat(),
            &[
                "entity_id: 9 [1..2]",
                "properties: on_ground: false, steps: 2 [2..3]",
                "steps: 2 steps [3..17]",
                "  0:  [3..10]",
                "    ticks: 1 [3..4]",
                "    delta: 1.0, 0.0, 0.0 [4..10]",
                "  1:  [10..17]",
                "    ticks: 3 [10..11]",
                "    delta: 1.0, 0.0, 0.0 [11..17]",
                "y_rot: 90.0° [17..18]",
                "x_rot: 0.0° [18..19]",
            ],
        );
        check(
            V26_3..=NEWEST,
            "move_entity_rot",
            &[9, 1, 64, 0],
            &[
                "entity_id: 9 [1..2]",
                "on_ground: true [2..3]",
                "y_rot: 90.0° [3..4]",
                "x_rot: 0.0° [4..5]",
            ],
        );
    }

    fn position_move_rotation_bytes() -> Vec<u8> {
        [
            &f64s(&[1.0, 2.0, 3.0])[..],
            &f64s(&[0.0, -0.5, 0.0]),
            &f32s(&[90.0, 10.0]),
        ]
        .concat()
    }

    const POSITION_MOVE_ROTATION: [&str; 4] = [
        "position: 1.0, 2.0, 3.0 [2..26]",
        "delta_movement: 0.0, -0.5, 0.0 [26..50]",
        "y_rot: 90.0 [50..54]",
        "x_rot: 10.0 [54..58]",
    ];

    #[test]
    fn position_sync_and_teleports() {
        let pmr = position_move_rotation_bytes();
        check(
            V1_21_2..=V26_3 - 1,
            "entity_position_sync",
            &[&[4][..], &pmr, &[1]].concat(),
            &[
                &["entity_id: 4 [1..2]"][..],
                &POSITION_MOVE_ROTATION,
                &["on_ground: true [58..59]"],
            ]
            .concat(),
        );
        check(
            V26_3..=NEWEST,
            "entity_position_sync",
            &[
                &[4, 1, 1][..],
                &f64s(&[1.0, 2.0, 3.0]),
                &[5],
                &f32s(&[90.0, 10.0]),
                &[0],
            ]
            .concat(),
            &[
                "entity_id: 4 [1..2]",
                "path: stepped (1) [2..3]",
                "steps: 1 item [3..29]",
                "  0:  [4..29]",
                "    position: 1.0, 2.0, 3.0 [4..28]",
                "    tick_offset: 5 [28..29]",
                "y_rot: 90.0 [29..33]",
                "x_rot: 10.0 [33..37]",
                "on_ground: false [37..38]",
            ],
        );
        check(
            V1_21_2..=NEWEST,
            "teleport_entity",
            &[&[4][..], &pmr, &0x21i32.to_be_bytes(), &[1]].concat(),
            &[
                &["entity_id: 4 [1..2]"][..],
                &POSITION_MOVE_ROTATION,
                &[
                    "relatives: 0x21 x | delta_x [58..62]",
                    "on_ground: true [62..63]",
                ],
            ]
            .concat(),
        );
        check(
            OLDEST..=V1_21_2 - 1,
            "teleport_entity",
            &[&[4][..], &f64s(&[1.0, 2.0, 3.0]), &[64, 0, 1]].concat(),
            &[
                "entity_id: 4 [1..2]",
                "position: 1.0, 2.0, 3.0 [2..26]",
                "y_rot: 90.0° [26..27]",
                "x_rot: 0.0° [27..28]",
                "on_ground: true [28..29]",
            ],
        );
    }

    #[test]
    fn links_events_and_animations() {
        check(
            OLDEST..=NEWEST,
            "set_entity_link",
            &[&1i32.to_be_bytes()[..], &0i32.to_be_bytes()].concat(),
            &["source_id: 1 [1..5]", "dest_id: 0 [5..9]"],
        );
        check(
            OLDEST..=NEWEST,
            "set_passengers",
            &[5, 1, 6],
            &[
                "vehicle_id: 5 [1..2]",
                "passenger_ids: 1 item [2..4]",
                "  0: 6 [3..4]",
            ],
        );
        check(
            OLDEST..=NEWEST,
            "take_item_entity",
            &[1, 2, 3],
            &[
                "item_id: 1 [1..2]",
                "player_id: 2 [2..3]",
                "amount: 3 [3..4]",
            ],
        );
        check(
            OLDEST..=NEWEST,
            "entity_event",
            &[0, 0, 0, 9, 3],
            &["entity_id: 9 [1..5]", "event: 3 [5..6]"],
        );
        check(
            OLDEST..=NEWEST,
            "hurt_animation",
            &[&[9][..], &f32s(&[180.0])].concat(),
            &["entity_id: 9 [1..2]", "yaw: 180.0 [2..6]"],
        );
        check(
            OLDEST..=V26_3 - 1,
            "animate",
            &[9, 3],
            &["entity_id: 9 [1..2]", "action: swing_off_hand (3) [2..3]"],
        );
        check(
            V26_3..=NEWEST,
            "animate",
            &[9, 1],
            &["entity_id: 9 [1..2]", "action: critical_hit (1) [2..3]"],
        );
        check(
            V26_3..=NEWEST,
            "swing_animation",
            &[9, 0, 1, 6],
            &[
                "entity_id: 9 [1..2]",
                "hand: main_hand (0) [2..3]",
                "type: whack (1) [3..4]",
                "duration: 6 [4..5]",
            ],
        );
    }

    #[test]
    fn damage_and_effects() {
        check(
            OLDEST..=NEWEST,
            "damage_event",
            &[&[9, 4, 0, 10, 1][..], &f64s(&[1.0, 2.0, 3.0])].concat(),
            &[
                "entity_id: 9 [1..2]",
                "damage_type: #4 [2..3]",
                "cause_id: none [3..4]",
                "direct_id: 9 [4..5]",
                "source_position: 1.0, 2.0, 3.0 [5..30]",
            ],
        );
        check(
            OLDEST..=V1_20_5 - 1,
            "update_mob_effect",
            &[9, 1, 2, 0x80, 0x01, 0x06, 0],
            &[
                "entity_id: 9 [1..2]",
                "effect: #1 [2..3]",
                "amplifier: 2 [3..4]",
                "duration: 128 [4..6]",
                "flags: 0x06 visible | show_icon [6..7]",
                "factor_data: absent [7..8]",
            ],
        );
        check(
            V1_20_5..=NEWEST,
            "update_mob_effect",
            &[9, 1, 2, 0x80, 0x01, 0x0e],
            &[
                "entity_id: 9 [1..2]",
                "effect: #1 [2..3]",
                "amplifier: 2 [3..4]",
                "duration: 128 [4..6]",
                "flags: 0x0e visible | show_icon | blend [6..7]",
            ],
        );
        check(
            OLDEST..=NEWEST,
            "remove_mob_effect",
            &[9, 1],
            &["entity_id: 9 [1..2]", "effect: #1 [2..3]"],
        );
        check(
            OLDEST..=NEWEST,
            "set_health",
            &[&f32s(&[20.0])[..], &[18], &f32s(&[5.0])].concat(),
            &[
                "health: 20.0 [1..5]",
                "food: 18 [5..6]",
                "saturation: 5.0 [6..10]",
            ],
        );
        check(
            OLDEST..=NEWEST,
            "set_experience",
            &[&f32s(&[0.5])[..], &[3, 40]].concat(),
            &[
                "progress: 0.5 [1..5]",
                "level: 3 [5..6]",
                "total: 40 [6..7]",
            ],
        );
    }

    #[test]
    fn add_player_in_1_20_1() {
        check(
            OLDEST..=OLDEST,
            "add_player",
            &[&[7][..], &UUID, &f64s(&[0.0, 1.0, 2.0]), &[0, 0]].concat(),
            &[
                "entity_id: 7 [1..2]",
                "uuid: abababab-abab-abab-abab-abababababab [2..18]",
                "position: 0.0, 1.0, 2.0 [18..42]",
                "y_rot: 0.0° [42..43]",
                "x_rot: 0.0° [43..44]",
            ],
        );
    }
}
