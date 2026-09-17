//! Packets shared by the configuration and game phases. 1.20.1 has no
//! configuration phase, so they are game packets there.

use peel_trace::Direction::{self, Clientbound, Serverbound};
use peel_trace::Phase;

use super::super::reader::{Reader, Result};
use super::super::{Decoder, NEWEST, OLDEST, PacketDecoder};
use super::text::component;
use super::{GAME, V1_20_2, V1_20_5, V1_21_2, decoder};

const CONFIGURATION: &[Phase] = &[Phase::Configuration];

const fn in_game(
    direction: Direction,
    name: &'static str,
    decode: fn(&mut Decoder) -> Result<()>,
) -> PacketDecoder {
    decoder(GAME, direction, name, OLDEST..=NEWEST, decode)
}

const fn in_configuration(
    direction: Direction,
    name: &'static str,
    decode: fn(&mut Decoder) -> Result<()>,
) -> PacketDecoder {
    decoder(CONFIGURATION, direction, name, V1_20_2..=NEWEST, decode)
}

pub static DECODERS: &[PacketDecoder] = &[
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

#[cfg(test)]
mod tests {
    use std::ops::RangeInclusive;

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
}
