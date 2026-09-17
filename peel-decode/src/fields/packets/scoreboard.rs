//! Scoreboard packets: objectives, scores, display slots and teams.

use peel_trace::Direction::Clientbound;

use super::super::reader::{Reader, Result};
use super::super::{Decoder, NEWEST, OLDEST, PacketDecoder};
use super::text::{component, optional_component};
use super::{V1_20_2, V1_20_3, V1_21_5, V26_2, byte_enum, game, string, variant};

pub static DECODERS: &[PacketDecoder] = &[
    game(Clientbound, "set_objective", OLDEST..=NEWEST, |d| {
        d.string("name")?;
        let method = d.variant("method", Reader::i8, &["add", "remove", "change"])?;
        if method == 0 || method == 2 {
            component(d, "display_name")?;
            d.variant("render_type", Reader::var_int, &["integer", "hearts"])?;
            if d.protocol() >= V1_20_3 {
                number_format(d)?;
            }
        }
        Ok(())
    }),
    game(Clientbound, "set_score", OLDEST..=V1_20_3 - 1, |d| {
        d.string("owner")?;
        let method = d.variant("method", Reader::var_int, &["change", "remove"])?;
        d.string("objective")?;
        if method != 1 {
            d.var_int("score")?;
        }
        Ok(())
    }),
    game(Clientbound, "set_score", V1_20_3..=NEWEST, |d| {
        d.string("owner")?;
        d.string("objective")?;
        d.var_int("score")?;
        optional_component(d, "display")?;
        number_format(d)
    }),
    game(Clientbound, "reset_score", V1_20_3..=NEWEST, |d| {
        d.string("owner")?;
        d.optional("objective", string)?;
        Ok(())
    }),
    game(Clientbound, "set_display_objective", OLDEST..=NEWEST, |d| {
        byte_enum(d, "slot", DISPLAY_SLOTS, V1_20_2)?;
        d.string("objective")?;
        Ok(())
    }),
    game(Clientbound, "set_player_team", OLDEST..=NEWEST, |d| {
        d.string("name")?;
        let method = d.variant(
            "method",
            Reader::i8,
            &["add", "remove", "change", "join", "leave"],
        )?;
        if method == 0 || method == 2 {
            d.group("parameters", team_parameters)?;
        }
        if matches!(method, 0 | 3 | 4) {
            d.list("players", string)?;
        }
        Ok(())
    }),
];

/// `DisplaySlot`: the list, sidebar, below name, then a sidebar per team
/// color.
const DISPLAY_SLOTS: &[&str] = &[
    "list",
    "sidebar",
    "below_name",
    "sidebar.team.black",
    "sidebar.team.dark_blue",
    "sidebar.team.dark_green",
    "sidebar.team.dark_aqua",
    "sidebar.team.dark_red",
    "sidebar.team.dark_purple",
    "sidebar.team.gold",
    "sidebar.team.gray",
    "sidebar.team.dark_gray",
    "sidebar.team.blue",
    "sidebar.team.green",
    "sidebar.team.aqua",
    "sidebar.team.red",
    "sidebar.team.light_purple",
    "sidebar.team.yellow",
    "sidebar.team.white",
];

/// `ChatFormatting`, whose first 16 entries are also `TeamColor`.
const CHAT_FORMATTING: &[&str] = &[
    "black",
    "dark_blue",
    "dark_green",
    "dark_aqua",
    "dark_red",
    "dark_purple",
    "gold",
    "gray",
    "dark_gray",
    "blue",
    "green",
    "aqua",
    "red",
    "light_purple",
    "yellow",
    "white",
    "obfuscated",
    "bold",
    "strikethrough",
    "underline",
    "italic",
    "reset",
];

/// An optional `NumberFormat`, whose type is a registry `pomme-protocol` has
/// no table for.
fn number_format(d: &mut Decoder) -> Result<()> {
    d.prefixed(Reader::bool, |d, present| {
        if !present {
            return d.note("number_format", "absent");
        }
        let kind = d.raw_id("number_format")?;
        Err(d.reader().unsupported(format!("number format #{kind}")))
    })
}

/// `ClientboundSetPlayerTeamPacket.Parameters`. 1.21.5 sent the rules as
/// enums instead of names, and 26.2 reordered the fields and made the color
/// an optional `TeamColor`.
fn team_parameters(d: &mut Decoder) -> Result<()> {
    let protocol = d.protocol();
    component(d, "display_name")?;
    if protocol >= V26_2 {
        component(d, "prefix")?;
        component(d, "suffix")?;
        team_rules(d)?;
        d.optional("color", |d, name| variant(d, name, &CHAT_FORMATTING[..16]))?;
        return team_options(d);
    }
    team_options(d)?;
    if protocol >= V1_21_5 {
        team_rules(d)?;
    } else {
        d.string("name_tag_visibility")?;
        d.string("collision_rule")?;
    }
    d.variant("color", Reader::var_int, CHAT_FORMATTING)?;
    component(d, "prefix")?;
    component(d, "suffix")
}

fn team_rules(d: &mut Decoder) -> Result<()> {
    d.variant(
        "name_tag_visibility",
        Reader::var_int,
        &[
            "always",
            "never",
            "hide_for_other_teams",
            "hide_for_own_team",
        ],
    )?;
    d.variant(
        "collision_rule",
        Reader::var_int,
        &["always", "never", "push_other_teams", "push_own_team"],
    )?;
    Ok(())
}

fn team_options(d: &mut Decoder) -> Result<()> {
    d.flags(
        "options",
        Reader::u8,
        &["allow_friendly_fire", "see_friendly_invisibles"],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::ops::RangeInclusive;

    use peel_trace::Phase::Game;

    use super::super::super::testing::{assert_decodes, assert_partial};
    use super::*;

    fn check(range: RangeInclusive<i32>, name: &str, body: &[u8], expected: &[impl AsRef<str>]) {
        assert_decodes(range, Game, Clientbound, name, body, expected);
    }

    /// A network NBT string tag holding one character.
    fn text(c: u8) -> [u8; 4] {
        [8, 0, 1, c]
    }

    #[test]
    fn objectives() {
        let head = [
            "name: \"o\" [1..3]",
            "method: add (0) [3..4]",
            "display_name: \"T\" [4..8]",
            "render_type: hearts (1) [8..9]",
        ];
        check(
            V1_20_3..=NEWEST,
            "set_objective",
            &[&[1, b'o', 0][..], &text(b'T'), &[1, 0]].concat(),
            &[&head[..], &["number_format: absent [9..10]"]].concat(),
        );
        assert_partial(
            V1_20_3..=NEWEST,
            Game,
            Clientbound,
            "set_objective",
            &[&[1, b'o', 0][..], &text(b'T'), &[1, 1, 2]].concat(),
            &[&head[..], &["number_format: #2 [9..11]"]].concat(),
            (11, "number format #2 isn't decoded yet"),
        );
        check(
            OLDEST..=NEWEST,
            "set_objective",
            &[1, b'o', 1],
            &["name: \"o\" [1..3]", "method: remove (1) [3..4]"],
        );
        check(
            OLDEST..=NEWEST,
            "set_display_objective",
            &[1, 1, b'o'],
            &["slot: sidebar (1) [1..2]", "objective: \"o\" [2..4]"],
        );
    }

    #[test]
    fn scores() {
        check(
            OLDEST..=V1_20_3 - 1,
            "set_score",
            &[1, b'p', 0, 1, b'o', 5],
            &[
                "owner: \"p\" [1..3]",
                "method: change (0) [3..4]",
                "objective: \"o\" [4..6]",
                "score: 5 [6..7]",
            ],
        );
        check(
            OLDEST..=V1_20_3 - 1,
            "set_score",
            &[1, b'p', 1, 1, b'o'],
            &[
                "owner: \"p\" [1..3]",
                "method: remove (1) [3..4]",
                "objective: \"o\" [4..6]",
            ],
        );
        check(
            V1_20_3..=NEWEST,
            "set_score",
            &[1, b'p', 1, b'o', 5, 0, 0],
            &[
                "owner: \"p\" [1..3]",
                "objective: \"o\" [3..5]",
                "score: 5 [5..6]",
                "display: absent [6..7]",
                "number_format: absent [7..8]",
            ],
        );
        check(
            V1_20_3..=NEWEST,
            "reset_score",
            &[1, b'p', 1, 1, b'o'],
            &["owner: \"p\" [1..3]", "objective: \"o\" [3..6]"],
        );
    }

    #[test]
    fn teams_by_version() {
        let head = [
            "name: \"t\" [1..3]".to_string(),
            "method: change (2) [3..4]".into(),
        ];
        let team = |parameters: &[u8], lines: &[&str]| {
            let body = [&[1, b't', 2][..], parameters].concat();
            let end = 3 + parameters.len() + 1;
            let mut expected = head.to_vec();
            expected.push(format!("parameters:  [4..{end}]"));
            expected.extend(lines.iter().map(|line| format!("  {line}")));
            (body, expected)
        };
        let (body, expected) = team(
            &[
                &text(b'D')[..],
                &[1, 6],
                b"always",
                &[5],
                b"never",
                &[12],
                &text(b'P'),
                &text(b'S'),
            ]
            .concat(),
            &[
                "display_name: \"D\" [4..8]",
                "options: 0x01 allow_friendly_fire [8..9]",
                "name_tag_visibility: \"always\" [9..16]",
                "collision_rule: \"never\" [16..22]",
                "color: red (12) [22..23]",
                "prefix: \"P\" [23..27]",
                "suffix: \"S\" [27..31]",
            ],
        );
        check(V1_20_3..=V1_21_5 - 1, "set_player_team", &body, &expected);
        let (body, expected) = team(
            &[&text(b'D')[..], &[1, 0, 1, 12], &text(b'P'), &text(b'S')].concat(),
            &[
                "display_name: \"D\" [4..8]",
                "options: 0x01 allow_friendly_fire [8..9]",
                "name_tag_visibility: always (0) [9..10]",
                "collision_rule: never (1) [10..11]",
                "color: red (12) [11..12]",
                "prefix: \"P\" [12..16]",
                "suffix: \"S\" [16..20]",
            ],
        );
        check(V1_21_5..=V26_2 - 1, "set_player_team", &body, &expected);
        let (body, expected) = team(
            &[&text(b'D')[..], &text(b'P'), &text(b'S'), &[0, 1, 1, 12, 1]].concat(),
            &[
                "display_name: \"D\" [4..8]",
                "prefix: \"P\" [8..12]",
                "suffix: \"S\" [12..16]",
                "name_tag_visibility: always (0) [16..17]",
                "collision_rule: never (1) [17..18]",
                "color: red (12) [18..20]",
                "options: 0x01 allow_friendly_fire [20..21]",
            ],
        );
        check(V26_2..=NEWEST, "set_player_team", &body, &expected);
        check(
            OLDEST..=NEWEST,
            "set_player_team",
            &[1, b't', 3, 1, 1, b'p'],
            &[
                "name: \"t\" [1..3]",
                "method: join (3) [3..4]",
                "players: 1 item [4..7]",
                "  0: \"p\" [5..7]",
            ],
        );
    }
}
