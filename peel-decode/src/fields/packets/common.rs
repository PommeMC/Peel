//! Packets shared by the configuration and game phases, and cookies, which
//! login uses too. 1.20.1 has no configuration phase, so they are game
//! packets there.

use peel_trace::Direction::{Clientbound, Serverbound};
use peel_trace::Phase;

use super::super::reader::{Reader, Result};
use super::super::{Decoder, NEWEST, OLDEST, PacketDecoder};
use super::components::string_entry;
use super::text::{component, optional_component};
use super::{
    CONFIGURATION, GAME, V1_20_2, V1_20_3, V1_20_5, V1_21, V1_21_2, V1_21_6, V1_21_9, V26_3,
    decoder, holder, in_configuration, in_game, nbt, nbt_tag, string, uuid,
};

const COOKIE_PHASES: &[Phase] = &[Phase::Login, Phase::Configuration, Phase::Game];
const COMMON_PHASES: &[Phase] = &[Phase::Configuration, Phase::Game];

pub static DECODERS: &[PacketDecoder] = &[
    decoder(
        COOKIE_PHASES,
        Clientbound,
        "cookie_request",
        V1_20_5..=NEWEST,
        |d| {
            d.string("key")?;
            Ok(())
        },
    ),
    decoder(
        COOKIE_PHASES,
        Serverbound,
        "cookie_response",
        V1_20_5..=NEWEST,
        |d| {
            d.string("key")?;
            d.optional("payload", |d, name| d.byte_array(name))?;
            Ok(())
        },
    ),
    decoder(
        COMMON_PHASES,
        Clientbound,
        "store_cookie",
        V1_20_5..=NEWEST,
        |d| {
            d.string("key")?;
            d.byte_array("payload")?;
            Ok(())
        },
    ),
    decoder(
        COMMON_PHASES,
        Clientbound,
        "transfer",
        V1_20_5..=NEWEST,
        |d| {
            d.string("host")?;
            d.var_int("port")?;
            Ok(())
        },
    ),
    decoder(
        COMMON_PHASES,
        Clientbound,
        "custom_report_details",
        V1_21..=NEWEST,
        |d| {
            d.list("details", string_entry)?;
            Ok(())
        },
    ),
    decoder(
        COMMON_PHASES,
        Clientbound,
        "server_links",
        V1_21..=NEWEST,
        |d| {
            d.list("links", |d, name| d.group(name, server_link))?;
            Ok(())
        },
    ),
    decoder(
        GAME,
        Clientbound,
        "resource_pack",
        OLDEST..=V1_20_2,
        resource_pack,
    ),
    decoder(
        CONFIGURATION,
        Clientbound,
        "resource_pack",
        V1_20_2..=V1_20_2,
        resource_pack,
    ),
    decoder(
        COMMON_PHASES,
        Clientbound,
        "resource_pack_push",
        V1_20_3..=NEWEST,
        |d| {
            d.uuid("id")?;
            resource_pack(d)
        },
    ),
    decoder(
        COMMON_PHASES,
        Clientbound,
        "resource_pack_pop",
        V1_20_3..=NEWEST,
        |d| {
            d.optional("id", uuid)?;
            Ok(())
        },
    ),
    in_game(Serverbound, "resource_pack", resource_pack_response),
    in_configuration(Serverbound, "resource_pack", resource_pack_response),
    decoder(GAME, Clientbound, "show_dialog", V1_21_6..=NEWEST, |d| {
        holder(d, "dialog", None, |d| nbt_tag(d, "value".into()))
    }),
    decoder(
        CONFIGURATION,
        Clientbound,
        "show_dialog",
        V1_21_6..=NEWEST,
        |d| nbt_tag(d, "dialog".into()),
    ),
    decoder(
        COMMON_PHASES,
        Clientbound,
        "clear_dialog",
        V1_21_6..=NEWEST,
        |_| Ok(()),
    ),
    decoder(
        COMMON_PHASES,
        Serverbound,
        "custom_click_action",
        V1_21_6..=NEWEST,
        |d| {
            d.string("id")?;
            d.prefixed(
                |r| r.count(),
                |d, len| {
                    let end = d.reader().pos() + len;
                    nbt::root(d, "payload")?;
                    if d.reader().pos() != end {
                        return Err(d
                            .reader()
                            .error(format!("the payload should take {len} bytes")));
                    }
                    Ok(())
                },
            )
        },
    ),
    decoder(
        CONFIGURATION,
        Clientbound,
        "code_of_conduct",
        V1_21_9..=NEWEST,
        |d| {
            d.string("code_of_conduct")?;
            Ok(())
        },
    ),
    decoder(
        CONFIGURATION,
        Serverbound,
        "accept_code_of_conduct",
        V1_21_9..=NEWEST,
        |_| Ok(()),
    ),
    decoder(
        CONFIGURATION,
        Clientbound,
        "reset_chat",
        V1_20_5..=NEWEST,
        |_| Ok(()),
    ),
    in_game(Clientbound, "keep_alive", keep_alive),
    in_configuration(Clientbound, "keep_alive", keep_alive),
    in_game(Serverbound, "keep_alive", keep_alive),
    in_configuration(Serverbound, "keep_alive", keep_alive),
    in_game(Clientbound, "ping", ping),
    in_configuration(Clientbound, "ping", ping),
    in_game(Serverbound, "pong", ping),
    in_configuration(Serverbound, "pong", ping),
    in_game(Clientbound, "custom_payload", custom_payload),
    in_configuration(Clientbound, "custom_payload", custom_payload),
    in_game(Serverbound, "custom_payload", custom_payload),
    in_configuration(Serverbound, "custom_payload", custom_payload),
    in_game(Serverbound, "client_information", client_information),
    in_configuration(Serverbound, "client_information", client_information),
    in_game(Clientbound, "disconnect", disconnect),
    in_configuration(Clientbound, "disconnect", disconnect),
    in_configuration(Clientbound, "finish_configuration", |_| Ok(())),
    in_configuration(Serverbound, "finish_configuration", |_| Ok(())),
    decoder(
        GAME,
        Clientbound,
        "start_configuration",
        V1_20_2..=NEWEST,
        |_| Ok(()),
    ),
    decoder(
        GAME,
        Serverbound,
        "configuration_acknowledged",
        V1_20_2..=NEWEST,
        |_| Ok(()),
    ),
    decoder(
        CONFIGURATION,
        Clientbound,
        "select_known_packs",
        V1_20_5..=NEWEST,
        known_packs,
    ),
    decoder(
        CONFIGURATION,
        Serverbound,
        "select_known_packs",
        V1_20_5..=NEWEST,
        known_packs,
    ),
    decoder(
        GAME,
        Clientbound,
        "post_effects",
        V26_3..=NEWEST,
        post_effects,
    ),
    decoder(
        CONFIGURATION,
        Clientbound,
        "post_effects",
        V26_3..=NEWEST,
        post_effects,
    ),
];

fn keep_alive(d: &mut Decoder) -> Result<()> {
    d.i64("id")?;
    Ok(())
}

fn ping(d: &mut Decoder) -> Result<()> {
    d.i32("id")?;
    Ok(())
}

/// A channel, then the rest of the frame. Vanilla parses only
/// `minecraft:brand` beyond that.
fn custom_payload(d: &mut Decoder) -> Result<()> {
    if d.string("channel")? == "minecraft:brand" {
        d.string("brand")?;
    } else {
        d.rest("data")?;
    }
    Ok(())
}

/// `ClientInformation`.
fn client_information(d: &mut Decoder) -> Result<()> {
    d.string("language")?;
    d.i8("view_distance")?;
    d.variant(
        "chat_visibility",
        Reader::var_int,
        &["full", "system", "hidden"],
    )?;
    d.bool("chat_colors")?;
    d.flags(
        "model_customisation",
        Reader::u8,
        &[
            "cape",
            "jacket",
            "left_sleeve",
            "right_sleeve",
            "left_pants_leg",
            "right_pants_leg",
            "hat",
        ],
    )?;
    d.variant("main_hand", Reader::var_int, &["left", "right"])?;
    d.bool("text_filtering_enabled")?;
    d.bool("allows_listing")?;
    if d.protocol() >= V1_21_2 {
        d.variant(
            "particle_status",
            Reader::var_int,
            &["all", "decreased", "minimal"],
        )?;
    }
    Ok(())
}

fn resource_pack(d: &mut Decoder) -> Result<()> {
    d.string("url")?;
    d.string("hash")?;
    d.bool("required")?;
    optional_component(d, "prompt")
}

/// The client's progress with a resource pack; 1.20.3 added the pack id and
/// the last four actions.
fn resource_pack_response(d: &mut Decoder) -> Result<()> {
    let actions = &[
        "successfully_loaded",
        "declined",
        "failed_download",
        "accepted",
        "downloaded",
        "invalid_url",
        "failed_reload",
        "discarded",
    ];
    let known = if d.protocol() >= V1_20_3 {
        d.uuid("id")?;
        8
    } else {
        4
    };
    d.variant("action", Reader::var_int, &actions[..known])?;
    Ok(())
}

/// `ServerLinks.UntrustedEntry`: a known link type or a custom label, then the
/// URL.
fn server_link(d: &mut Decoder) -> Result<()> {
    d.prefixed(Reader::bool, |d, known| {
        if known {
            d.variant(
                "type",
                Reader::var_int,
                &[
                    "report_bug",
                    "community_guidelines",
                    "support",
                    "status",
                    "feedback",
                    "community",
                    "website",
                    "forums",
                    "news",
                    "announcements",
                ],
            )
            .map(drop)
        } else {
            component(d, "label")
        }
    })?;
    d.string("url")?;
    Ok(())
}

fn disconnect(d: &mut Decoder) -> Result<()> {
    component(d, "reason")
}

/// `KnownPack` list.
fn known_packs(d: &mut Decoder) -> Result<()> {
    d.list("packs", |d, name| {
        d.group(name, |d| {
            d.string("namespace")?;
            d.string("id")?;
            d.string("version")?;
            Ok(())
        })
    })?;
    Ok(())
}

/// The post-processing shaders to apply, by id.
fn post_effects(d: &mut Decoder) -> Result<()> {
    d.list("post_effects", string)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::ops::RangeInclusive;

    use peel_trace::{Direction, Phase};

    use super::super::super::testing::{NO_FIELDS, assert_decodes};
    use super::super::V1_20_3;
    use super::*;

    /// Checks both phases a common packet lives in.
    fn check(
        direction: Direction,
        name: &str,
        range: RangeInclusive<i32>,
        body: &[u8],
        expected: &[impl AsRef<str>],
    ) {
        let (start, end) = (*range.start(), *range.end());
        assert_decodes(start..=end, Phase::Game, direction, name, body, expected);
        assert_decodes(
            start.max(V1_20_2)..=end,
            Phase::Configuration,
            direction,
            name,
            body,
            expected,
        );
    }

    #[test]
    fn keep_alive_and_ping() {
        for direction in [Clientbound, Serverbound] {
            check(
                direction,
                "keep_alive",
                OLDEST..=NEWEST,
                &(-2i64).to_be_bytes(),
                &["id: -2 [1..9]"],
            );
        }
        check(
            Clientbound,
            "ping",
            OLDEST..=NEWEST,
            &7i32.to_be_bytes(),
            &["id: 7 [1..5]"],
        );
        check(
            Serverbound,
            "pong",
            OLDEST..=NEWEST,
            &7i32.to_be_bytes(),
            &["id: 7 [1..5]"],
        );
    }

    #[test]
    fn custom_payload_reads_the_brand() {
        let brand = [&[15][..], b"minecraft:brand", &[7], b"vanilla"].concat();
        let other = [&[8][..], b"mod:data", &[1, 2, 3]].concat();
        for direction in [Clientbound, Serverbound] {
            check(
                direction,
                "custom_payload",
                OLDEST..=NEWEST,
                &brand,
                &[
                    "channel: \"minecraft:brand\" [1..17]",
                    "brand: \"vanilla\" [17..25]",
                ],
            );
            check(
                direction,
                "custom_payload",
                OLDEST..=NEWEST,
                &other,
                &["channel: \"mod:data\" [1..10]", "data: 3 bytes [10..13]"],
            );
        }
    }

    #[test]
    fn client_information_gains_particles_in_1_21_2() {
        let body = [&[5][..], b"en_us", &[12, 0, 1, 0x7f, 1, 0, 1]].concat();
        let expected = [
            "language: \"en_us\" [1..7]",
            "view_distance: 12 [7..8]",
            "chat_visibility: full (0) [8..9]",
            "chat_colors: true [9..10]",
            "model_customisation: 0x7f cape | jacket | left_sleeve | right_sleeve | left_pants_leg | right_pants_leg | hat [10..11]",
            "main_hand: right (1) [11..12]",
            "text_filtering_enabled: false [12..13]",
            "allows_listing: true [13..14]",
        ];
        check(
            Serverbound,
            "client_information",
            OLDEST..=V1_21_2 - 1,
            &body,
            &expected,
        );
        check(
            Serverbound,
            "client_information",
            V1_21_2..=NEWEST,
            &[&body[..], &[2]].concat(),
            &[&expected[..], &["particle_status: minimal (2) [14..15]"]].concat(),
        );
    }

    #[test]
    fn disconnect_switches_to_nbt_in_1_20_3() {
        check(
            Clientbound,
            "disconnect",
            OLDEST..=V1_20_2,
            b"\x04\"hi\"",
            &["reason: \"hi\" [1..6]", "  json: \"\\\"hi\\\"\" [1..6]"],
        );
        check(
            Clientbound,
            "disconnect",
            V1_20_3..=NEWEST,
            b"\x08\x00\x02hi",
            &["reason: \"hi\" [1..6]"],
        );
    }

    #[test]
    fn configuration_packets() {
        for direction in [Clientbound, Serverbound] {
            assert_decodes(
                V1_20_2..=NEWEST,
                Phase::Configuration,
                direction,
                "finish_configuration",
                &[],
                NO_FIELDS,
            );
            assert_decodes(
                V1_20_5..=NEWEST,
                Phase::Configuration,
                direction,
                "select_known_packs",
                &[&[1, 9][..], b"minecraft", &[4], b"core", &[4], b"26.3"].concat(),
                &[
                    "packs: 1 item [1..22]",
                    "  0:  [2..22]",
                    "    namespace: \"minecraft\" [2..12]",
                    "    id: \"core\" [12..17]",
                    "    version: \"26.3\" [17..22]",
                ],
            );
        }
        assert_decodes(
            V1_20_2..=NEWEST,
            Phase::Game,
            Clientbound,
            "start_configuration",
            &[],
            NO_FIELDS,
        );
        assert_decodes(
            V1_20_2..=NEWEST,
            Phase::Game,
            Serverbound,
            "configuration_acknowledged",
            &[],
            NO_FIELDS,
        );
    }

    #[test]
    fn post_effects() {
        let body = [&[1, 14][..], b"minecraft:blur"].concat();
        check(
            Clientbound,
            "post_effects",
            V26_3..=NEWEST,
            &body,
            &[
                "post_effects: 1 item [1..17]",
                "  0: \"minecraft:blur\" [2..17]",
            ],
        );
    }

    #[test]
    fn cookies_and_transfers() {
        let key = [3, b'a', b':', b'b'];
        let request = ["key: \"a:b\" [1..5]"];
        let response = [&key[..], &[1, 2, 9, 9]].concat();
        let response_lines = ["key: \"a:b\" [1..5]", "payload: 2 bytes [5..9]"];
        check(
            Clientbound,
            "cookie_request",
            V1_20_5..=NEWEST,
            &key,
            &request,
        );
        check(
            Serverbound,
            "cookie_response",
            V1_20_5..=NEWEST,
            &response,
            &response_lines,
        );
        assert_decodes(
            V1_20_5..=NEWEST,
            Phase::Login,
            Clientbound,
            "cookie_request",
            &key,
            &request,
        );
        assert_decodes(
            V1_20_5..=NEWEST,
            Phase::Login,
            Serverbound,
            "cookie_response",
            &response,
            &response_lines,
        );
        check(
            Clientbound,
            "store_cookie",
            V1_20_5..=NEWEST,
            &[&key[..], &[1, 7]].concat(),
            &["key: \"a:b\" [1..5]", "payload: 1 byte [5..7]"],
        );
        check(
            Clientbound,
            "transfer",
            V1_20_5..=NEWEST,
            &[1, b'h', 0xdd, 0xc7, 0x01],
            &["host: \"h\" [1..3]", "port: 25565 [3..6]"],
        );
    }

    #[test]
    fn report_details_and_links() {
        check(
            Clientbound,
            "custom_report_details",
            V1_21..=NEWEST,
            &[1, 1, b'k', 1, b'v'],
            &["details: 1 item [1..6]", "  0: k: v [2..6]"],
        );
        check(
            Clientbound,
            "server_links",
            V1_21..=NEWEST,
            &[2, 1, 6, 1, b'u', 0, 8, 0, 1, b'L', 1, b'w'],
            &[
                "links: 2 items [1..13]",
                "  0:  [2..6]",
                "    type: website (6) [2..4]",
                "    url: \"u\" [4..6]",
                "  1:  [6..13]",
                "    label: \"L\" [6..11]",
                "    url: \"w\" [11..13]",
            ],
        );
    }

    #[test]
    fn resource_packs() {
        let pack = [1, b'u', 1, b'h', 1, 0];
        check(
            Clientbound,
            "resource_pack",
            OLDEST..=V1_20_2,
            &pack,
            &[
                "url: \"u\" [1..3]",
                "hash: \"h\" [3..5]",
                "required: true [5..6]",
                "prompt: absent [6..7]",
            ],
        );
        let uuid = [0x12; 16];
        let id = "id: 12121212-1212-1212-1212-121212121212";
        check(
            Clientbound,
            "resource_pack_push",
            V1_20_3..=NEWEST,
            &[&uuid[..], &pack].concat(),
            &[
                &format!("{id} [1..17]"),
                "url: \"u\" [17..19]",
                "hash: \"h\" [19..21]",
                "required: true [21..22]",
                "prompt: absent [22..23]",
            ],
        );
        check(
            Clientbound,
            "resource_pack_pop",
            V1_20_3..=NEWEST,
            &[&[1][..], &uuid].concat(),
            &[format!("{id} [1..18]")],
        );
        check(
            Serverbound,
            "resource_pack",
            OLDEST..=V1_20_3 - 1,
            &[3],
            &["action: accepted (3) [1..2]"],
        );
        check(
            Serverbound,
            "resource_pack",
            V1_20_3..=NEWEST,
            &[&uuid[..], &[7]].concat(),
            &[&format!("{id} [1..17]"), "action: discarded (7) [17..18]"],
        );
    }

    #[test]
    fn dialogs_and_codes_of_conduct() {
        assert_decodes(
            V1_21_6..=NEWEST,
            Phase::Game,
            Clientbound,
            "show_dialog",
            &[0, 10, 0],
            &["dialog:  [1..4]", "  value: 0 keys [2..4]"],
        );
        assert_decodes(
            V1_21_6..=NEWEST,
            Phase::Game,
            Clientbound,
            "show_dialog",
            &[3],
            &["dialog: #2 [1..2]"],
        );
        assert_decodes(
            V1_21_6..=NEWEST,
            Phase::Configuration,
            Clientbound,
            "show_dialog",
            &[10, 0],
            &["dialog: 0 keys [1..3]"],
        );
        check(
            Clientbound,
            "clear_dialog",
            V1_21_6..=NEWEST,
            &[],
            NO_FIELDS,
        );
        check(
            Serverbound,
            "custom_click_action",
            V1_21_6..=NEWEST,
            &[3, b'a', b':', b'b', 2, 10, 0],
            &["id: \"a:b\" [1..5]", "payload: 0 keys [5..8]"],
        );
        check(
            Serverbound,
            "custom_click_action",
            V1_21_6..=NEWEST,
            &[3, b'a', b':', b'b', 1, 0],
            &["id: \"a:b\" [1..5]", "payload: none [5..7]"],
        );
        for (direction, name, body, expected) in [
            (
                Clientbound,
                "code_of_conduct",
                &[2, b'h', b'i'][..],
                &["code_of_conduct: \"hi\" [1..4]"][..],
            ),
            (Serverbound, "accept_code_of_conduct", &[], NO_FIELDS),
        ] {
            assert_decodes(
                V1_21_9..=NEWEST,
                Phase::Configuration,
                direction,
                name,
                body,
                expected,
            );
        }
        assert_decodes(
            V1_20_5..=NEWEST,
            Phase::Configuration,
            Clientbound,
            "reset_chat",
            &[],
            NO_FIELDS,
        );
    }
}
