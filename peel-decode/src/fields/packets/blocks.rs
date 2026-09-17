//! Packets for editing blocks and their menus: signs, command blocks,
//! structure and test blocks, beacons, books and trading.

use peel_trace::Direction::{Clientbound, Serverbound};

use super::super::reader::{Reader, Result};
use super::super::{Decoder, NEWEST, OLDEST, PacketDecoder};
use super::text::optional_component;
use super::{V1_20_3, V1_21_5, V26_3, byte_enum, byte_vec3, game, raw_id, string, vec3i};

pub static DECODERS: &[PacketDecoder] = &[
    game(Clientbound, "open_sign_editor", OLDEST..=NEWEST, |d| {
        d.block_pos("block_pos")?;
        sign_side(d)
    }),
    game(Serverbound, "sign_update", OLDEST..=NEWEST, |d| {
        d.block_pos("block_pos")?;
        if d.protocol() < V26_3 {
            sign_side(d)?;
        }
        d.group("lines", |d| {
            for index in 0..4 {
                d.string(index.to_string())?;
            }
            Ok(())
        })?;
        if d.protocol() >= V26_3 {
            sign_side(d)?;
        }
        Ok(())
    }),
    game(Serverbound, "set_command_block", OLDEST..=NEWEST, |d| {
        d.block_pos("block_pos")?;
        d.string("command")?;
        d.variant("mode", Reader::var_int, &["sequence", "auto", "redstone"])?;
        d.flags(
            "flags",
            Reader::u8,
            &["track_output", "conditional", "automatic"],
        )?;
        Ok(())
    }),
    game(Serverbound, "set_command_minecart", OLDEST..=NEWEST, |d| {
        d.var_int("entity_id")?;
        d.string("command")?;
        d.bool("track_output")?;
        Ok(())
    }),
    game(Serverbound, "set_structure_block", OLDEST..=NEWEST, |d| {
        d.block_pos("block_pos")?;
        d.variant(
            "update_type",
            Reader::var_int,
            &["update_data", "save_area", "load_area", "scan_area"],
        )?;
        d.variant("mode", Reader::var_int, &["save", "load", "corner", "data"])?;
        d.string("name")?;
        for name in ["offset", "size"] {
            byte_vec3(d, name.into())?;
        }
        d.variant(
            "mirror",
            Reader::var_int,
            &["none", "left_right", "front_back"],
        )?;
        structure_rotation(d)?;
        d.string("data")?;
        d.f32("integrity")?;
        d.var_long("seed")?;
        let flags = ["ignore_entities", "show_air", "show_bounding_box", "strict"];
        let known = if d.protocol() >= V1_21_5 { 4 } else { 3 };
        d.flags("flags", Reader::u8, &flags[..known])?;
        Ok(())
    }),
    game(Serverbound, "set_jigsaw_block", OLDEST..=NEWEST, |d| {
        d.block_pos("block_pos")?;
        for name in ["name", "target", "pool", "final_state", "joint"] {
            d.string(name)?;
        }
        if d.protocol() >= V1_20_3 {
            d.var_int("selection_priority")?;
            d.var_int("placement_priority")?;
        }
        Ok(())
    }),
    game(Serverbound, "jigsaw_generate", OLDEST..=NEWEST, |d| {
        d.block_pos("block_pos")?;
        d.var_int("levels")?;
        d.bool("keep_jigsaws")?;
        Ok(())
    }),
    game(Serverbound, "set_test_block", V1_21_5..=NEWEST, |d| {
        d.block_pos("block_pos")?;
        d.variant("mode", Reader::var_int, &["start", "log", "fail", "accept"])?;
        d.string("message")?;
        Ok(())
    }),
    game(
        Serverbound,
        "test_instance_block_action",
        V1_21_5..=NEWEST,
        |d| {
            d.block_pos("block_pos")?;
            d.variant(
                "action",
                Reader::var_int,
                &["init", "query", "set", "reset", "save", "export", "run"],
            )?;
            d.group("data", |d| {
                d.optional("test", string)?;
                vec3i(d, "size".into())?;
                structure_rotation(d)?;
                d.bool("ignore_entities")?;
                d.variant(
                    "status",
                    Reader::var_int,
                    &["cleared", "running", "finished"],
                )?;
                optional_component(d, "error_message")
            })
        },
    ),
    game(
        Serverbound,
        "block_entity_tag_query",
        OLDEST..=NEWEST,
        |d| {
            d.var_int("transaction_id")?;
            d.block_pos("block_pos")?;
            Ok(())
        },
    ),
    game(Serverbound, "entity_tag_query", OLDEST..=NEWEST, |d| {
        d.var_int("transaction_id")?;
        d.var_int("entity_id")?;
        Ok(())
    }),
    game(Serverbound, "set_beacon", OLDEST..=NEWEST, |d| {
        for name in ["primary_effect", "secondary_effect"] {
            d.optional(name, raw_id)?;
        }
        Ok(())
    }),
    game(Serverbound, "select_trade", OLDEST..=NEWEST, |d| {
        d.var_int("item")?;
        Ok(())
    }),
    game(Serverbound, "rename_item", OLDEST..=NEWEST, |d| {
        d.string("name")?;
        Ok(())
    }),
    game(Serverbound, "edit_book", OLDEST..=NEWEST, |d| {
        d.var_int("slot")?;
        d.list("pages", string)?;
        d.optional("title", string)?;
        Ok(())
    }),
];

/// `SignTextSlot`: whether the front text, as a bool until 26.3 sent an enum
/// with the same values.
fn sign_side(d: &mut Decoder) -> Result<()> {
    byte_enum(d, "side", &["back", "front"], V26_3).map(drop)
}

/// `Rotation`.
fn structure_rotation(d: &mut Decoder) -> Result<()> {
    d.variant(
        "rotation",
        Reader::var_int,
        &[
            "none",
            "clockwise_90",
            "clockwise_180",
            "counterclockwise_90",
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::ops::RangeInclusive;

    use peel_trace::Direction;
    use peel_trace::Phase::Game;

    use super::super::super::testing::{assert_decodes, block_pos, f32s};
    use super::*;

    fn check(
        direction: Direction,
        range: RangeInclusive<i32>,
        name: &str,
        body: &[u8],
        expected: &[impl AsRef<str>],
    ) {
        assert_decodes(range, Game, direction, name, body, expected);
    }

    fn pos() -> [u8; 8] {
        block_pos(1, 2, 3)
    }

    #[test]
    fn signs() {
        check(
            Clientbound,
            OLDEST..=NEWEST,
            "open_sign_editor",
            &[&pos()[..], &[1]].concat(),
            &["block_pos: 1, 2, 3 [1..9]", "side: front (1) [9..10]"],
        );
        let lines = [1, b'a', 1, b'b', 1, b'c', 1, b'd'];
        let line_fields = |at: usize| {
            [
                format!("lines:  [{at}..{}]", at + 8),
                format!("  0: \"a\" [{at}..{}]", at + 2),
                format!("  1: \"b\" [{}..{}]", at + 2, at + 4),
                format!("  2: \"c\" [{}..{}]", at + 4, at + 6),
                format!("  3: \"d\" [{}..{}]", at + 6, at + 8),
            ]
        };
        let mut expected = vec![
            "block_pos: 1, 2, 3 [1..9]".to_string(),
            "side: front (1) [9..10]".into(),
        ];
        expected.extend(line_fields(10));
        check(
            Serverbound,
            OLDEST..=V26_3 - 1,
            "sign_update",
            &[&pos()[..], &[1], &lines].concat(),
            &expected,
        );
        let mut expected = vec!["block_pos: 1, 2, 3 [1..9]".to_string()];
        expected.extend(line_fields(9));
        expected.push("side: back (0) [17..18]".into());
        check(
            Serverbound,
            V26_3..=NEWEST,
            "sign_update",
            &[&pos()[..], &lines, &[0]].concat(),
            &expected,
        );
    }

    #[test]
    fn command_blocks() {
        check(
            Serverbound,
            OLDEST..=NEWEST,
            "set_command_block",
            &[&pos()[..], &[2, b's', b'a', 1, 5]].concat(),
            &[
                "block_pos: 1, 2, 3 [1..9]",
                "command: \"sa\" [9..12]",
                "mode: auto (1) [12..13]",
                "flags: 0x05 track_output | automatic [13..14]",
            ],
        );
        check(
            Serverbound,
            OLDEST..=NEWEST,
            "set_command_minecart",
            &[7, 1, b'c', 1],
            &[
                "entity_id: 7 [1..2]",
                "command: \"c\" [2..4]",
                "track_output: true [4..5]",
            ],
        );
    }

    #[test]
    fn structures_and_jigsaws() {
        let body = [
            &pos()[..],
            &[0, 0, 1, b'n', 1, 0xff, 2, 3, 4, 5, 0, 1, 0],
            &f32s(&[1.0]),
            &[9, 8],
        ]
        .concat();
        let lines = |flags: &str| {
            [
                "block_pos: 1, 2, 3 [1..9]".to_string(),
                "update_type: update_data (0) [9..10]".into(),
                "mode: save (0) [10..11]".into(),
                "name: \"n\" [11..13]".into(),
                "offset: 1, -1, 2 [13..16]".into(),
                "size: 3, 4, 5 [16..19]".into(),
                "mirror: none (0) [19..20]".into(),
                "rotation: clockwise_90 (1) [20..21]".into(),
                "data: \"\" [21..22]".into(),
                "integrity: 1.0 [22..26]".into(),
                "seed: 9 [26..27]".into(),
                format!("flags: 0x08 {flags} [27..28]"),
            ]
        };
        check(
            Serverbound,
            OLDEST..=V1_21_5 - 1,
            "set_structure_block",
            &body,
            &lines("bit 3"),
        );
        check(
            Serverbound,
            V1_21_5..=NEWEST,
            "set_structure_block",
            &body,
            &lines("strict"),
        );
        let names = [1, b'a', 1, b'b', 1, b'c', 1, b'd', 1, b'e'];
        let named = [
            "block_pos: 1, 2, 3 [1..9]",
            "name: \"a\" [9..11]",
            "target: \"b\" [11..13]",
            "pool: \"c\" [13..15]",
            "final_state: \"d\" [15..17]",
            "joint: \"e\" [17..19]",
        ];
        check(
            Serverbound,
            OLDEST..=V1_20_3 - 1,
            "set_jigsaw_block",
            &[&pos()[..], &names].concat(),
            &named,
        );
        check(
            Serverbound,
            V1_20_3..=NEWEST,
            "set_jigsaw_block",
            &[&pos()[..], &names, &[2, 3]].concat(),
            &[
                &named[..],
                &[
                    "selection_priority: 2 [19..20]",
                    "placement_priority: 3 [20..21]",
                ],
            ]
            .concat(),
        );
        check(
            Serverbound,
            OLDEST..=NEWEST,
            "jigsaw_generate",
            &[&pos()[..], &[3, 1]].concat(),
            &[
                "block_pos: 1, 2, 3 [1..9]",
                "levels: 3 [9..10]",
                "keep_jigsaws: true [10..11]",
            ],
        );
    }

    #[test]
    fn test_blocks() {
        check(
            Serverbound,
            V1_21_5..=NEWEST,
            "set_test_block",
            &[&pos()[..], &[2, 2, b'h', b'i']].concat(),
            &[
                "block_pos: 1, 2, 3 [1..9]",
                "mode: fail (2) [9..10]",
                "message: \"hi\" [10..13]",
            ],
        );
        check(
            Serverbound,
            V1_21_5..=NEWEST,
            "test_instance_block_action",
            &[
                &pos()[..],
                &[6, 1, 3, b'a', b':', b'b', 1, 2, 3, 0, 0, 1, 0],
            ]
            .concat(),
            &[
                "block_pos: 1, 2, 3 [1..9]",
                "action: run (6) [9..10]",
                "data:  [10..22]",
                "  test: \"a:b\" [10..15]",
                "  size: 1, 2, 3 [15..18]",
                "  rotation: none (0) [18..19]",
                "  ignore_entities: false [19..20]",
                "  status: running (1) [20..21]",
                "  error_message: absent [21..22]",
            ],
        );
    }

    #[test]
    fn queries_and_menus() {
        check(
            Serverbound,
            OLDEST..=NEWEST,
            "block_entity_tag_query",
            &[&[3][..], &pos()].concat(),
            &["transaction_id: 3 [1..2]", "block_pos: 1, 2, 3 [2..10]"],
        );
        check(
            Serverbound,
            OLDEST..=NEWEST,
            "entity_tag_query",
            &[3, 7],
            &["transaction_id: 3 [1..2]", "entity_id: 7 [2..3]"],
        );
        check(
            Serverbound,
            OLDEST..=NEWEST,
            "set_beacon",
            &[1, 5, 0],
            &[
                "primary_effect: #5 [1..3]",
                "secondary_effect: absent [3..4]",
            ],
        );
        check(
            Serverbound,
            OLDEST..=NEWEST,
            "select_trade",
            &[2],
            &["item: 2 [1..2]"],
        );
        check(
            Serverbound,
            OLDEST..=NEWEST,
            "rename_item",
            &[2, b'h', b'i'],
            &["name: \"hi\" [1..4]"],
        );
        check(
            Serverbound,
            OLDEST..=NEWEST,
            "edit_book",
            &[1, 1, 1, b'p', 1, 1, b't'],
            &[
                "slot: 1 [1..2]",
                "pages: 1 item [2..5]",
                "  0: \"p\" [3..5]",
                "title: \"t\" [5..8]",
            ],
        );
    }
}
