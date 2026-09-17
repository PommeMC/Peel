//! Serverbound game packets about the player: movement, input and actions.

use peel_trace::Direction::Serverbound;

use super::super::reader::{Reader, Result};
use super::super::{Decoder, NEWEST, OLDEST, PacketDecoder};
use super::{
    DIRECTION, HAND, V1_20_2, V1_21, V1_21_2, V1_21_4, V1_21_6, V1_21_11, V26_1, V26_2, V26_3,
    game, rotation,
};

pub static DECODERS: &[PacketDecoder] = &[
    game(Serverbound, "client_tick_end", V1_21_2..=NEWEST, |_| Ok(())),
    game(Serverbound, "move_player_pos", OLDEST..=NEWEST, |d| {
        d.vec3("position")?;
        movement_flags(d)
    }),
    game(Serverbound, "move_player_pos_rot", OLDEST..=NEWEST, |d| {
        d.vec3("position")?;
        rotation(d)?;
        movement_flags(d)
    }),
    game(Serverbound, "move_player_rot", OLDEST..=NEWEST, |d| {
        rotation(d)?;
        movement_flags(d)
    }),
    game(
        Serverbound,
        "move_player_status_only",
        OLDEST..=NEWEST,
        movement_flags,
    ),
    game(Serverbound, "player_input", OLDEST..=NEWEST, |d| {
        if d.protocol() >= V1_21_2 {
            d.flags(
                "input",
                Reader::u8,
                &[
                    "forward", "backward", "left", "right", "jump", "shift", "sprint",
                ],
            )?;
        } else {
            d.f32("sideways")?;
            d.f32("forward")?;
            d.flags("flags", Reader::u8, &["jumping", "shift_key_down"])?;
        }
        Ok(())
    }),
    game(Serverbound, "set_carried_item", OLDEST..=NEWEST, |d| {
        d.i16("slot")?;
        Ok(())
    }),
    game(Serverbound, "accept_teleportation", OLDEST..=NEWEST, |d| {
        d.var_int("id")?;
        if d.protocol() >= V26_3 {
            d.vec3("position")?;
            rotation(d)?;
        }
        Ok(())
    }),
    game(Serverbound, "chunk_batch_received", V1_20_2..=NEWEST, |d| {
        d.f32("desired_chunks_per_tick")?;
        Ok(())
    }),
    game(Serverbound, "player_loaded", V1_21_4..=NEWEST, |_| Ok(())),
    game(Serverbound, "player_command", OLDEST..=NEWEST, |d| {
        d.var_int("entity_id")?;
        let actions: &[&str] = if d.protocol() >= V1_21_6 {
            &PLAYER_COMMANDS[2..]
        } else {
            PLAYER_COMMANDS
        };
        d.variant("action", Reader::var_int, actions)?;
        d.var_int("data")?;
        Ok(())
    }),
    game(Serverbound, "interact", OLDEST..=V1_21_11, |d| {
        d.var_int("entity_id")?;
        match d.variant(
            "action",
            Reader::var_int,
            &["interact", "attack", "interact_at"],
        )? {
            0 => {
                d.variant("hand", Reader::var_int, HAND)?;
            }
            2 => {
                d.vector3f("location")?;
                d.variant("hand", Reader::var_int, HAND)?;
            }
            _ => {}
        }
        d.bool("using_secondary_action")?;
        Ok(())
    }),
    game(Serverbound, "interact", V26_1..=NEWEST, |d| {
        d.var_int("entity_id")?;
        d.variant("hand", Reader::var_int, HAND)?;
        d.lp_vec3("location")?;
        d.bool("using_secondary_action")?;
        Ok(())
    }),
    game(Serverbound, "attack", V26_1..=NEWEST, |d| {
        d.var_int("entity_id")?;
        Ok(())
    }),
    game(Serverbound, "swing", OLDEST..=V26_2, |d| {
        d.variant("hand", Reader::var_int, HAND)?;
        Ok(())
    }),
    game(Serverbound, "punch", V26_3..=NEWEST, |_| Ok(())),
    game(Serverbound, "use_item_on", OLDEST..=NEWEST, |d| {
        d.variant("hand", Reader::var_int, HAND)?;
        d.group("hit_result", |d| {
            d.block_pos("block_pos")?;
            d.variant("direction", Reader::var_int, DIRECTION)?;
            d.vector3f("cursor")?;
            d.bool("inside")?;
            if d.protocol() >= V1_21_2 {
                d.bool("world_border_hit")?;
            }
            Ok(())
        })?;
        d.var_int("sequence")?;
        Ok(())
    }),
    game(Serverbound, "use_item", OLDEST..=NEWEST, |d| {
        d.variant("hand", Reader::var_int, HAND)?;
        d.var_int("sequence")?;
        if d.protocol() >= V1_21 {
            rotation(d)?;
        }
        Ok(())
    }),
    game(Serverbound, "player_action", OLDEST..=NEWEST, |d| {
        let actions: &[&str] = match d.protocol() {
            V26_3.. => PLAYER_ACTIONS_26_3,
            V1_21_11.. => &PLAYER_ACTIONS[..8],
            _ => &PLAYER_ACTIONS[..7],
        };
        d.variant("action", Reader::var_int, actions)?;
        d.block_pos("block_pos")?;
        d.variant("direction", Reader::u8, DIRECTION)?;
        d.var_int("sequence")?;
        Ok(())
    }),
    game(Serverbound, "player_abilities", OLDEST..=NEWEST, |d| {
        d.flags("flags", Reader::u8, &["", "flying"])?;
        Ok(())
    }),
    game(Serverbound, "move_vehicle", OLDEST..=NEWEST, |d| {
        d.vec3("position")?;
        rotation(d)?;
        if d.protocol() >= V1_21_4 {
            d.bool("on_ground")?;
        }
        Ok(())
    }),
    game(Serverbound, "paddle_boat", OLDEST..=NEWEST, |d| {
        d.bool("left")?;
        d.bool("right")?;
        Ok(())
    }),
    game(Serverbound, "pick_item", OLDEST..=V1_21_2, |d| {
        d.var_int("slot")?;
        Ok(())
    }),
    game(Serverbound, "pick_item_from_block", V1_21_4..=NEWEST, |d| {
        d.block_pos("block_pos")?;
        d.bool("include_data")?;
        Ok(())
    }),
    game(
        Serverbound,
        "pick_item_from_entity",
        V1_21_4..=NEWEST,
        |d| {
            d.var_int("entity_id")?;
            d.bool("include_data")?;
            Ok(())
        },
    ),
];

/// `ServerboundPlayerCommandPacket.Action` before 1.21.6, which dropped the
/// first two.
const PLAYER_COMMANDS: &[&str] = &[
    "press_shift_key",
    "release_shift_key",
    "stop_sleeping",
    "start_sprinting",
    "stop_sprinting",
    "start_riding_jump",
    "stop_riding_jump",
    "open_inventory",
    "start_fall_flying",
];

/// `ServerboundPlayerActionPacket.Action` up to 26.2; 1.21.11 added `stab`.
const PLAYER_ACTIONS: &[&str] = &[
    "start_destroy_block",
    "abort_destroy_block",
    "stop_destroy_block",
    "drop_all_items",
    "drop_item",
    "release_use_item",
    "swap_item_with_offhand",
    "stab",
];

/// 26.3 inserted `change_destroy_direction` second.
const PLAYER_ACTIONS_26_3: &[&str] = &[
    "start_destroy_block",
    "change_destroy_direction",
    "abort_destroy_block",
    "stop_destroy_block",
    "drop_all_items",
    "drop_item",
    "release_use_item",
    "swap_item_with_offhand",
    "stab",
];

/// `ServerboundMovePlayerPacket`'s last byte: an on-ground bool until 1.21.2
/// made it flags.
fn movement_flags(d: &mut Decoder) -> Result<()> {
    if d.protocol() >= V1_21_2 {
        d.flags("flags", Reader::u8, &["on_ground", "horizontal_collision"])?;
    } else {
        d.bool("on_ground")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::ops::RangeInclusive;

    use peel_trace::Phase::Game;

    use super::super::super::testing::{NO_FIELDS, assert_decodes, block_pos, f32s, f64s};
    use super::*;

    fn check(range: RangeInclusive<i32>, name: &str, body: &[u8], expected: &[impl AsRef<str>]) {
        assert_decodes(range, Game, Serverbound, name, body, expected);
    }

    #[test]
    fn empty_packets() {
        check(V1_21_2..=NEWEST, "client_tick_end", &[], NO_FIELDS);
        check(V1_21_4..=NEWEST, "player_loaded", &[], NO_FIELDS);
        check(V26_3..=NEWEST, "punch", &[], NO_FIELDS);
    }

    #[test]
    fn move_player() {
        let position = f64s(&[1.5, 64.0, -2.25]);
        let rotation = f32s(&[90.0, -12.5]);
        check(
            V1_21_2..=NEWEST,
            "move_player_pos_rot",
            &[&position[..], &rotation, &[3]].concat(),
            &[
                "position: 1.5, 64.0, -2.25 [1..25]",
                "y_rot: 90.0 [25..29]",
                "x_rot: -12.5 [29..33]",
                "flags: 0x03 on_ground | horizontal_collision [33..34]",
            ],
        );
        check(
            OLDEST..=V1_21,
            "move_player_pos",
            &[&position[..], &[1]].concat(),
            &[
                "position: 1.5, 64.0, -2.25 [1..25]",
                "on_ground: true [25..26]",
            ],
        );
        check(
            V1_21_2..=NEWEST,
            "move_player_rot",
            &[&rotation[..], &[0]].concat(),
            &[
                "y_rot: 90.0 [1..5]",
                "x_rot: -12.5 [5..9]",
                "flags: none [9..10]",
            ],
        );
        check(
            OLDEST..=V1_21,
            "move_player_status_only",
            &[0],
            &["on_ground: false [1..2]"],
        );
    }

    #[test]
    fn player_input() {
        check(
            V1_21_2..=NEWEST,
            "player_input",
            &[0x51],
            &["input: 0x51 forward | jump | sprint [1..2]"],
        );
        check(
            OLDEST..=V1_21,
            "player_input",
            &[&f32s(&[0.0, 0.98])[..], &[2]].concat(),
            &[
                "sideways: 0.0 [1..5]",
                "forward: 0.98 [5..9]",
                "flags: 0x02 shift_key_down [9..10]",
            ],
        );
    }

    #[test]
    fn simple_fields() {
        check(
            OLDEST..=NEWEST,
            "set_carried_item",
            &[0, 4],
            &["slot: 4 [1..3]"],
        );
        check(
            V1_20_2..=NEWEST,
            "chunk_batch_received",
            &f32s(&[25.0]),
            &["desired_chunks_per_tick: 25.0 [1..5]"],
        );
        check(V26_1..=NEWEST, "attack", &[42], &["entity_id: 42 [1..2]"]);
        check(
            OLDEST..=V26_2,
            "swing",
            &[1],
            &["hand: off_hand (1) [1..2]"],
        );
        check(
            OLDEST..=NEWEST,
            "player_abilities",
            &[2],
            &["flags: 0x02 flying [1..2]"],
        );
        check(
            OLDEST..=NEWEST,
            "paddle_boat",
            &[1, 0],
            &["left: true [1..2]", "right: false [2..3]"],
        );
        check(OLDEST..=V1_21_2, "pick_item", &[9], &["slot: 9 [1..2]"]);
        check(
            V1_21_4..=NEWEST,
            "pick_item_from_block",
            &[&block_pos(1, 2, 3)[..], &[1]].concat(),
            &["block_pos: 1, 2, 3 [1..9]", "include_data: true [9..10]"],
        );
        check(
            V1_21_4..=NEWEST,
            "pick_item_from_entity",
            &[7, 0],
            &["entity_id: 7 [1..2]", "include_data: false [2..3]"],
        );
    }

    #[test]
    fn accept_teleportation() {
        check(
            OLDEST..=V26_2,
            "accept_teleportation",
            &[3],
            &["id: 3 [1..2]"],
        );
        let body = [&[3][..], &f64s(&[0.5, 70.0, 0.5]), &f32s(&[180.0, 0.0])].concat();
        check(
            V26_3..=NEWEST,
            "accept_teleportation",
            &body,
            &[
                "id: 3 [1..2]",
                "position: 0.5, 70.0, 0.5 [2..26]",
                "y_rot: 180.0 [26..30]",
                "x_rot: 0.0 [30..34]",
            ],
        );
    }

    #[test]
    fn player_command_actions_shift_at_1_21_6() {
        let expected = |action: &str| {
            [
                "entity_id: 7 [1..2]".to_string(),
                format!("action: {action} (1) [2..3]"),
                "data: 0 [3..4]".to_string(),
            ]
        };
        for (range, action) in [
            (OLDEST..=V1_21_6 - 1, "release_shift_key"),
            (V1_21_6..=NEWEST, "start_sprinting"),
        ] {
            let expected = expected(action);
            check(range, "player_command", &[7, 1, 0], &expected);
        }
    }

    #[test]
    fn interact() {
        check(
            OLDEST..=V1_21_11,
            "interact",
            &[5, 1, 1],
            &[
                "entity_id: 5 [1..2]",
                "action: attack (1) [2..3]",
                "using_secondary_action: true [3..4]",
            ],
        );
        check(
            OLDEST..=V1_21_11,
            "interact",
            &[&[5, 2][..], &f32s(&[0.25, 1.0, -0.5]), &[1, 0]].concat(),
            &[
                "entity_id: 5 [1..2]",
                "action: interact_at (2) [2..3]",
                "location: 0.25, 1.0, -0.5 [3..15]",
                "hand: off_hand (1) [15..16]",
                "using_secondary_action: false [16..17]",
            ],
        );
        check(
            V26_1..=NEWEST,
            "interact",
            &[5, 0, 0, 1],
            &[
                "entity_id: 5 [1..2]",
                "hand: main_hand (0) [2..3]",
                "location: 0.0, 0.0, 0.0 [3..4]",
                "using_secondary_action: true [4..5]",
            ],
        );
    }

    #[test]
    fn use_item_on() {
        let hit = [
            &block_pos(-4, 70, 12)[..],
            &[1],
            &f32s(&[0.5, 1.0, 0.25]),
            &[0],
        ]
        .concat();
        let expected = [
            "hand: main_hand (0) [1..2]",
            "hit_result:  [2..24]",
            "  block_pos: -4, 70, 12 [2..10]",
            "  direction: up (1) [10..11]",
            "  cursor: 0.5, 1.0, 0.25 [11..23]",
            "  inside: false [23..24]",
            "sequence: 9 [24..25]",
        ];
        check(
            OLDEST..=V1_21,
            "use_item_on",
            &[&[0][..], &hit, &[9]].concat(),
            &expected,
        );
        let mut newer = expected.to_vec();
        newer[1] = "hit_result:  [2..25]";
        newer.insert(6, "  world_border_hit: true [24..25]");
        newer[7] = "sequence: 9 [25..26]";
        check(
            V1_21_2..=NEWEST,
            "use_item_on",
            &[&[0][..], &hit, &[1, 9]].concat(),
            &newer,
        );
    }

    #[test]
    fn use_item() {
        check(
            OLDEST..=V1_21 - 1,
            "use_item",
            &[1, 4],
            &["hand: off_hand (1) [1..2]", "sequence: 4 [2..3]"],
        );
        check(
            V1_21..=NEWEST,
            "use_item",
            &[&[1, 4][..], &f32s(&[45.0, 10.0])].concat(),
            &[
                "hand: off_hand (1) [1..2]",
                "sequence: 4 [2..3]",
                "y_rot: 45.0 [3..7]",
                "x_rot: 10.0 [7..11]",
            ],
        );
    }

    #[test]
    fn player_action_names_by_version() {
        for (range, action, stab) in [
            (OLDEST..=V1_21_11 - 1, "abort_destroy_block", "unknown"),
            (V1_21_11..=V26_2, "abort_destroy_block", "stab"),
            (
                V26_3..=NEWEST,
                "change_destroy_direction",
                "swap_item_with_offhand",
            ),
        ] {
            let body = |ordinal: u8| [&[ordinal][..], &block_pos(0, -60, 0), &[5, 2]].concat();
            let lines = |label: &str, ordinal: u8| {
                [
                    format!("action: {label} ({ordinal}) [1..2]"),
                    "block_pos: 0, -60, 0 [2..10]".to_string(),
                    "direction: east (5) [10..11]".to_string(),
                    "sequence: 2 [11..12]".to_string(),
                ]
            };
            for (ordinal, label) in [(1, action), (7, stab)] {
                let expected = lines(label, ordinal);
                check(range.clone(), "player_action", &body(ordinal), &expected);
            }
        }
    }

    #[test]
    fn move_vehicle() {
        let body = [&f64s(&[1.0, 2.0, 3.0])[..], &f32s(&[4.0, 5.0])].concat();
        let expected = [
            "position: 1.0, 2.0, 3.0 [1..25]",
            "y_rot: 4.0 [25..29]",
            "x_rot: 5.0 [29..33]",
        ];
        check(OLDEST..=V1_21_2, "move_vehicle", &body, &expected);
        check(
            V1_21_4..=NEWEST,
            "move_vehicle",
            &[&body[..], &[1]].concat(),
            &[&expected[..], &["on_ground: true [33..34]"]].concat(),
        );
    }
}
