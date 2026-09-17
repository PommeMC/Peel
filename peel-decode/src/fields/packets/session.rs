//! Packets that set up a play session: joining a world, respawning, synced
//! registries, tags, enabled features, the player list and commands.

use peel_trace::Direction::Clientbound;

use super::super::decoder::variant_label;
use super::super::reader::{Reader, Result};
use super::super::{Decoder, NEWEST, OLDEST, PacketDecoder};
use super::chat::chat_session;
use super::text::optional_component;
use super::{
    CONFIGURATION, GAME, V1_20_2, V1_20_3, V1_20_5, V1_21_2, V1_21_4, V1_21_6, V26_2, V26_3,
    decoder, game, global_pos, in_configuration, in_game, nbt, nbt_tag, string, uuid, var_int,
};

pub static DECODERS: &[PacketDecoder] = &[
    game(Clientbound, "login", OLDEST..=OLDEST, |d| {
        d.i32("entity_id")?;
        d.bool("hardcore")?;
        game_types(d)?;
        dimensions(d)?;
        nbt::root(d, "registries")?;
        d.string("dimension_type")?;
        d.string("dimension")?;
        d.i64("hashed_seed")?;
        distances(d)?;
        d.bool("reduced_debug_info")?;
        d.bool("show_death_screen")?;
        world_flags(d)
    }),
    game(Clientbound, "login", V1_20_2..=NEWEST, |d| {
        d.i32("entity_id")?;
        d.bool("hardcore")?;
        dimensions(d)?;
        distances(d)?;
        d.bool("reduced_debug_info")?;
        d.bool("show_death_screen")?;
        d.bool("do_limited_crafting")?;
        spawn_info(d)?;
        if d.protocol() >= V26_2 {
            d.bool("online_mode")?;
        }
        if d.protocol() >= V1_20_5 {
            d.bool("enforces_secure_chat")?;
        }
        Ok(())
    }),
    game(Clientbound, "respawn", OLDEST..=OLDEST, |d| {
        d.string("dimension_type")?;
        d.string("dimension")?;
        d.i64("hashed_seed")?;
        game_types(d)?;
        d.bool("is_debug")?;
        d.bool("is_flat")?;
        data_to_keep(d)?;
        death_location(d)?;
        d.var_int("portal_cooldown")?;
        Ok(())
    }),
    game(Clientbound, "respawn", V1_20_2..=NEWEST, |d| {
        spawn_info(d)?;
        data_to_keep(d)
    }),
    decoder(
        CONFIGURATION,
        Clientbound,
        "registry_data",
        V1_20_2..=V1_20_3,
        |d| {
            nbt::root(d, "registries")?;
            Ok(())
        },
    ),
    decoder(
        CONFIGURATION,
        Clientbound,
        "registry_data",
        V1_20_5..=NEWEST,
        |d| {
            d.string("registry")?;
            d.list("entries", |d, name| {
                d.group(name, |d| {
                    d.string("id")?;
                    d.optional("data", nbt_tag)?;
                    Ok(())
                })
            })?;
            Ok(())
        },
    ),
    in_game(Clientbound, "update_tags", update_tags),
    in_configuration(Clientbound, "update_tags", update_tags),
    decoder(
        GAME,
        Clientbound,
        "update_enabled_features",
        OLDEST..=OLDEST,
        enabled_features,
    ),
    in_configuration(Clientbound, "update_enabled_features", enabled_features),
    game(Clientbound, "player_info_update", OLDEST..=NEWEST, |d| {
        let known = match d.protocol() {
            V1_21_4.. => 8,
            V1_21_2.. => 7,
            _ => 6,
        };
        let actions = d.flags("actions", Reader::u8, &PLAYER_INFO_ACTIONS[..known])?;
        d.list("entries", |d, name| {
            d.group(name, |d| {
                d.uuid("profile_id")?;
                for action in (0..known).filter(|bit| actions & 1 << bit != 0) {
                    player_info_action(d, action)?;
                }
                Ok(())
            })
        })?;
        Ok(())
    }),
    game(Clientbound, "player_info_remove", OLDEST..=NEWEST, |d| {
        d.list("profile_ids", uuid)?;
        Ok(())
    }),
    game(Clientbound, "commands", OLDEST..=NEWEST, |d| {
        d.list("nodes", |d, name| d.group(name, command_node))?;
        d.var_int("root_index")?;
        Ok(())
    }),
];

/// `ClientboundPlayerInfoUpdatePacket.Action`, applied in this order; 1.21.2
/// added `update_list_order` and 1.21.4 `update_hat`.
const PLAYER_INFO_ACTIONS: &[&str] = &[
    "add_player",
    "initialize_chat",
    "update_game_mode",
    "update_listed",
    "update_latency",
    "update_display_name",
    "update_list_order",
    "update_hat",
];

fn player_info_action(d: &mut Decoder, action: usize) -> Result<()> {
    match action {
        0 => {
            d.string("name")?;
            profile_properties(d)?;
        }
        1 => {
            d.optional("chat_session", |d, name| d.group(name, chat_session))?;
        }
        2 => {
            d.variant("game_mode", Reader::var_int, GAME_TYPES)?;
        }
        3 => {
            d.bool("listed")?;
        }
        4 => {
            d.var_int("latency")?;
        }
        5 => optional_component(d, "display_name")?,
        6 => {
            d.var_int("list_order")?;
        }
        _ => {
            d.bool("show_hat")?;
        }
    }
    Ok(())
}

/// `ByteBufCodecs.GAME_PROFILE_PROPERTIES`.
pub fn profile_properties(d: &mut Decoder) -> Result<()> {
    d.list("properties", |d, name| {
        d.group(name, |d| {
            d.string("name")?;
            d.string("value")?;
            d.optional("signature", string)?;
            Ok(())
        })
    })?;
    Ok(())
}

/// `ClientboundCommandsPacket.Entry`. The low two flag bits are the node
/// type: root, literal or argument. 1.21.6 added `restricted`.
fn command_node(d: &mut Decoder) -> Result<()> {
    let names = &[
        "literal",
        "argument",
        "executable",
        "redirect",
        "custom_suggestions",
        "restricted",
    ];
    let known = if d.protocol() >= V1_21_6 { 6 } else { 5 };
    let flags = d.flags("flags", Reader::u8, &names[..known])?;
    d.list("children", var_int)?;
    if flags & 0x08 != 0 {
        d.var_int("redirect")?;
    }
    match flags & 0x03 {
        0 => Ok(()),
        1 => d.string("name").map(drop),
        2 => {
            d.string("name")?;
            let parser = d.raw_id("parser")?;
            Err(d.reader().unsupported(format!("argument parser #{parser}")))
        }
        _ => Err(d.reader().error("node type 3 doesn't exist")),
    }
}

pub const GAME_TYPES: &[&str] = &["survival", "creative", "adventure", "spectator"];

/// `GameType` and the previous one: bytes with -1 for none until 26.3 sent a
/// `VarInt` and an optional `VarInt`.
fn game_types(d: &mut Decoder) -> Result<()> {
    let none_or = |previous: Option<i64>| match previous {
        Some(id) => variant_label(GAME_TYPES, id),
        None => "none".into(),
    };
    if d.protocol() >= V26_3 {
        d.variant("game_type", Reader::var_int, GAME_TYPES)?;
        d.read("previous_game_type", Reader::optional_var_int, |&id| {
            none_or(id.map(i64::from))
        })?;
    } else {
        d.variant("game_type", Reader::u8, GAME_TYPES)?;
        d.read("previous_game_type", Reader::i8, |&id| {
            none_or((id >= 0).then_some(i64::from(id)))
        })?;
    }
    Ok(())
}

fn dimensions(d: &mut Decoder) -> Result<()> {
    d.list("dimensions", string)?;
    Ok(())
}

fn distances(d: &mut Decoder) -> Result<()> {
    d.var_int("max_players")?;
    d.var_int("view_distance")?;
    d.var_int("simulation_distance")?;
    Ok(())
}

fn world_flags(d: &mut Decoder) -> Result<()> {
    d.bool("is_debug")?;
    d.bool("is_flat")?;
    death_location(d)?;
    d.var_int("portal_cooldown")?;
    Ok(())
}

fn death_location(d: &mut Decoder) -> Result<()> {
    d.optional("death_location", global_pos)?;
    Ok(())
}

/// `CommonPlayerSpawnInfo`, from 1.20.2.
fn spawn_info(d: &mut Decoder) -> Result<()> {
    if d.protocol() >= V1_20_5 {
        d.raw_id("dimension_type")?;
    } else {
        d.string("dimension_type")?;
    }
    d.string("dimension")?;
    d.i64("hashed_seed")?;
    game_types(d)?;
    world_flags(d)?;
    if d.protocol() >= V1_21_2 {
        d.var_int("sea_level")?;
    }
    Ok(())
}

fn data_to_keep(d: &mut Decoder) -> Result<()> {
    d.flags("data_to_keep", Reader::u8, &["attributes", "entity_data"])?;
    Ok(())
}

/// Tags by registry, each a list of registry ids.
fn update_tags(d: &mut Decoder) -> Result<()> {
    d.list("registries", |d, name| {
        d.group(name, |d| {
            d.string("registry")?;
            d.list("tags", |d, name| {
                d.group(name, |d| {
                    d.string("name")?;
                    d.list("ids", var_int)?;
                    Ok(())
                })
            })?;
            Ok(())
        })
    })?;
    Ok(())
}

fn enabled_features(d: &mut Decoder) -> Result<()> {
    d.list("features", string)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::ops::RangeInclusive;

    use peel_trace::Phase;

    use super::super::super::testing::{assert_decodes, assert_partial, block_pos};
    use super::super::{V1_21, V26_1};
    use super::*;

    fn check(range: RangeInclusive<i32>, name: &str, body: &[u8], expected: &[impl AsRef<str>]) {
        assert_decodes(range, Phase::Game, Clientbound, name, body, expected);
    }

    /// A length-prefixed string.
    fn text(s: &str) -> Vec<u8> {
        [&[s.len() as u8][..], s.as_bytes()].concat()
    }

    /// `CommonPlayerSpawnInfo` bytes and lines for a protocol range, starting
    /// at `at`, with survival and no previous game type.
    fn spawn_info(range: &RangeInclusive<i32>, at: usize) -> (Vec<u8>, Vec<String>) {
        let start = *range.start();
        let dimension_type = if start >= V1_20_5 {
            vec![0]
        } else {
            text("minecraft:overworld")
        };
        let game_types: &[u8] = if start >= V26_3 { &[0, 0] } else { &[0, 0xff] };
        let mut bytes = [
            &dimension_type[..],
            &text("minecraft:overworld"),
            &7i64.to_be_bytes(),
            game_types,
            &[0, 1, 0, 0],
        ]
        .concat();
        let mut lines = Vec::new();
        let mut pos = at;
        let mut push = |line: String, len: usize| {
            lines.push(format!("{line} [{pos}..{}]", pos + len));
            pos += len;
        };
        if start >= V1_20_5 {
            push("dimension_type: #0".into(), 1);
        } else {
            push("dimension_type: \"minecraft:overworld\"".into(), 20);
        }
        push("dimension: \"minecraft:overworld\"".into(), 20);
        push("hashed_seed: 7".into(), 8);
        push("game_type: survival (0)".into(), 1);
        push("previous_game_type: none".into(), 1);
        push("is_debug: false".into(), 1);
        push("is_flat: true".into(), 1);
        push("death_location: absent".into(), 1);
        push("portal_cooldown: 0".into(), 1);
        if start >= V1_21_2 {
            bytes.push(63);
            push("sea_level: 63".into(), 1);
        }
        (bytes, lines)
    }

    #[test]
    fn respawn_by_version() {
        for range in [
            V1_20_2..=V1_20_3,
            V1_20_5..=V1_21,
            V1_21_2..=V26_2,
            V26_3..=NEWEST,
        ] {
            let (info, mut expected) = spawn_info(&range, 1);
            let end = 1 + info.len();
            expected.push(format!(
                "data_to_keep: 0x03 attributes | entity_data [{end}..{}]",
                end + 1
            ));
            check(range, "respawn", &[&info[..], &[3]].concat(), &expected);
        }
    }

    #[test]
    fn respawn_in_1_20_1_keeps_data_before_the_death_location() {
        let body = [
            &text("minecraft:overworld")[..],
            &text("minecraft:the_nether"),
            &7i64.to_be_bytes(),
            &[1, 0, 0, 0, 1],
            &[1],
            &text("minecraft:overworld"),
            &block_pos(1, 2, 3),
            &[5],
        ]
        .concat();
        check(
            OLDEST..=OLDEST,
            "respawn",
            &body,
            &[
                "dimension_type: \"minecraft:overworld\" [1..21]",
                "dimension: \"minecraft:the_nether\" [21..42]",
                "hashed_seed: 7 [42..50]",
                "game_type: creative (1) [50..51]",
                "previous_game_type: survival (0) [51..52]",
                "is_debug: false [52..53]",
                "is_flat: false [53..54]",
                "data_to_keep: 0x01 attributes [54..55]",
                "death_location:  [55..84]",
                "  dimension: \"minecraft:overworld\" [56..76]",
                "  block_pos: 1, 2, 3 [76..84]",
                "portal_cooldown: 5 [84..85]",
            ],
        );
    }

    #[test]
    fn login_by_version() {
        let head = [
            &5i32.to_be_bytes()[..],
            &[0, 1],
            &text("minecraft:overworld"),
            &[20, 12, 10, 0, 1, 0],
        ]
        .concat();
        let head_lines = [
            "entity_id: 5 [1..5]",
            "hardcore: false [5..6]",
            "dimensions: 1 item [6..27]",
            "  0: \"minecraft:overworld\" [7..27]",
            "max_players: 20 [27..28]",
            "view_distance: 12 [28..29]",
            "simulation_distance: 10 [29..30]",
            "reduced_debug_info: false [30..31]",
            "show_death_screen: true [31..32]",
            "do_limited_crafting: false [32..33]",
        ];
        for (range, tail) in [
            (V1_20_2..=V1_20_3, &[][..]),
            (V1_20_5..=V1_21, &[1][..]),
            (V1_21_2..=V26_1, &[1][..]),
            (V26_2..=V26_2, &[0, 1][..]),
            (V26_3..=NEWEST, &[0, 1][..]),
        ] {
            let (info, info_lines) = spawn_info(&range, 33);
            let mut expected: Vec<String> = head_lines.iter().map(|s| s.to_string()).collect();
            expected.extend(info_lines);
            let mut at = 33 + info.len();
            if *range.start() >= V26_2 {
                expected.push(format!("online_mode: false [{at}..{}]", at + 1));
                at += 1;
            }
            if *range.start() >= V1_20_5 {
                expected.push(format!("enforces_secure_chat: true [{at}..{}]", at + 1));
            }
            check(
                range,
                "login",
                &[&head[..], &info, tail].concat(),
                &expected,
            );
        }
    }

    #[test]
    fn login_in_1_20_1_carries_registries() {
        let body = [
            &5i32.to_be_bytes()[..],
            &[1, 2, 0xff],
            &[1],
            &text("minecraft:overworld"),
            &[10, 0, 0, 0],
            &text("minecraft:overworld"),
            &text("minecraft:overworld"),
            &7i64.to_be_bytes(),
            &[20, 12, 10, 0, 1, 0, 0, 0, 0],
        ]
        .concat();
        check(
            OLDEST..=OLDEST,
            "login",
            &body,
            &[
                "entity_id: 5 [1..5]",
                "hardcore: true [5..6]",
                "game_type: adventure (2) [6..7]",
                "previous_game_type: none [7..8]",
                "dimensions: 1 item [8..29]",
                "  0: \"minecraft:overworld\" [9..29]",
                "registries: 0 keys [29..33]",
                "dimension_type: \"minecraft:overworld\" [33..53]",
                "dimension: \"minecraft:overworld\" [53..73]",
                "hashed_seed: 7 [73..81]",
                "max_players: 20 [81..82]",
                "view_distance: 12 [82..83]",
                "simulation_distance: 10 [83..84]",
                "reduced_debug_info: false [84..85]",
                "show_death_screen: true [85..86]",
                "is_debug: false [86..87]",
                "is_flat: false [87..88]",
                "death_location: absent [88..89]",
                "portal_cooldown: 0 [89..90]",
            ],
        );
    }

    #[test]
    fn registries_tags_and_features() {
        let entry = [&text("minecraft:plains")[..], &[1, 10, 0]].concat();
        assert_decodes(
            V1_20_5..=NEWEST,
            Phase::Configuration,
            Clientbound,
            "registry_data",
            &[&text("minecraft:worldgen/biome")[..], &[1], &entry].concat(),
            &[
                "registry: \"minecraft:worldgen/biome\" [1..26]",
                "entries: 1 item [26..47]",
                "  0:  [27..47]",
                "    id: \"minecraft:plains\" [27..44]",
                "    data: 0 keys [44..47]",
            ],
        );
        assert_decodes(
            V1_20_2..=V1_20_3,
            Phase::Configuration,
            Clientbound,
            "registry_data",
            &[10, 0],
            &["registries: 0 keys [1..3]"],
        );

        let tags = [
            &[1][..],
            &text("minecraft:block"),
            &[1],
            &text("minecraft:logs"),
            &[2, 5, 6],
        ]
        .concat();
        let tag_lines = [
            "registries: 1 item [1..37]",
            "  0:  [2..37]",
            "    registry: \"minecraft:block\" [2..18]",
            "    tags: 1 item [18..37]",
            "      0:  [19..37]",
            "        name: \"minecraft:logs\" [19..34]",
            "        ids: 2 items [34..37]",
            "          0: 5 [35..36]",
            "          1: 6 [36..37]",
        ];
        assert_decodes(
            OLDEST..=NEWEST,
            Phase::Game,
            Clientbound,
            "update_tags",
            &tags,
            &tag_lines,
        );
        assert_decodes(
            V1_20_2..=NEWEST,
            Phase::Configuration,
            Clientbound,
            "update_tags",
            &tags,
            &tag_lines,
        );

        let features = [&[1][..], &text("minecraft:vanilla")].concat();
        let feature_lines = [
            "features: 1 item [1..20]",
            "  0: \"minecraft:vanilla\" [2..20]",
        ];
        assert_decodes(
            OLDEST..=OLDEST,
            Phase::Game,
            Clientbound,
            "update_enabled_features",
            &features,
            &feature_lines,
        );
        assert_decodes(
            V1_20_2..=NEWEST,
            Phase::Configuration,
            Clientbound,
            "update_enabled_features",
            &features,
            &feature_lines,
        );
    }

    #[test]
    fn player_info_update() {
        let uuid = [0x12; 16];
        let profile_id = "12121212-1212-1212-1212-121212121212";
        let add = [
            &[1, 1][..],
            &uuid,
            &text("Bob"),
            &[1],
            &text("textures"),
            &text("abc"),
            &[1],
            &text("sig"),
        ]
        .concat();
        check(
            OLDEST..=NEWEST,
            "player_info_update",
            &add,
            &[
                "actions: 0x01 add_player [1..2]",
                "entries: 1 item [2..42]",
                "  0:  [3..42]",
                &format!("    profile_id: {profile_id} [3..19]"),
                "    name: \"Bob\" [19..23]",
                "    properties: 1 item [23..42]",
                "      0:  [24..42]",
                "        name: \"textures\" [24..33]",
                "        value: \"abc\" [33..37]",
                "        signature: \"sig\" [37..42]",
            ],
        );

        let session = [
            &[0x3e, 1][..],
            &uuid,
            &[1],
            &uuid,
            &9i64.to_be_bytes(),
            &[1, 0xa, 0],
            &[3, 1, 0, 0],
        ]
        .concat();
        check(
            OLDEST..=NEWEST,
            "player_info_update",
            &session,
            &[
                "actions: 0x3e initialize_chat | update_game_mode | update_listed | update_latency | update_display_name [1..2]",
                "entries: 1 item [2..51]",
                "  0:  [3..51]",
                &format!("    profile_id: {profile_id} [3..19]"),
                "    chat_session:  [19..47]",
                &format!("      session_id: {profile_id} [20..36]"),
                "      expires_at: 9 [36..44]",
                "      public_key: 1 byte [44..46]",
                "      key_signature: 0 bytes [46..47]",
                "    game_mode: spectator (3) [47..48]",
                "    listed: true [48..49]",
                "    latency: 0 [49..50]",
                "    display_name: absent [50..51]",
            ],
        );

        check(
            V1_21_2..=V1_21_2,
            "player_info_update",
            &[&[0x40, 1][..], &uuid, &[5]].concat(),
            &[
                "actions: 0x40 update_list_order [1..2]",
                "entries: 1 item [2..20]",
                "  0:  [3..20]",
                &format!("    profile_id: {profile_id} [3..19]"),
                "    list_order: 5 [19..20]",
            ],
        );
        check(
            V1_21_4..=NEWEST,
            "player_info_update",
            &[&[0xc0, 1][..], &uuid, &[5, 1]].concat(),
            &[
                "actions: 0xc0 update_list_order | update_hat [1..2]",
                "entries: 1 item [2..21]",
                "  0:  [3..21]",
                &format!("    profile_id: {profile_id} [3..19]"),
                "    list_order: 5 [19..20]",
                "    show_hat: true [20..21]",
            ],
        );
    }

    #[test]
    fn player_info_remove() {
        check(
            OLDEST..=NEWEST,
            "player_info_remove",
            &[&[1][..], &[0x12; 16]].concat(),
            &[
                "profile_ids: 1 item [1..18]",
                "  0: 12121212-1212-1212-1212-121212121212 [2..18]",
            ],
        );
    }

    #[test]
    fn commands() {
        let root = [0, 1, 1];
        let literal = [&[0x0d, 0, 0][..], &text("tp")].concat();
        check(
            OLDEST..=NEWEST,
            "commands",
            &[&[2][..], &root, &literal, &[0]].concat(),
            &[
                "nodes: 2 items [1..11]",
                "  0:  [2..5]",
                "    flags: none [2..3]",
                "    children: 1 item [3..5]",
                "      0: 1 [4..5]",
                "  1:  [5..11]",
                "    flags: 0x0d literal | executable | redirect [5..6]",
                "    children: 0 items [6..7]",
                "    redirect: 0 [7..8]",
                "    name: \"tp\" [8..11]",
                "root_index: 0 [11..12]",
            ],
        );
        assert_partial(
            OLDEST..=NEWEST,
            Phase::Game,
            Clientbound,
            "commands",
            &[&[1, 2, 0][..], &text("x"), &[5]].concat(),
            &[
                "nodes:  [1..7]",
                "  0:  [2..7]",
                "    flags: 0x02 argument [2..3]",
                "    children: 0 items [3..4]",
                "    name: \"x\" [4..6]",
                "    parser: #5 [6..7]",
            ],
            (7, "argument parser #5 isn't decoded yet"),
        );
    }
}
