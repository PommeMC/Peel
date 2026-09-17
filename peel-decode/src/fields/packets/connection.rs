//! Handshake, status and login packets.

use peel_trace::Direction::{Clientbound, Serverbound};
use peel_trace::Phase;

use super::super::reader::{Reader, Result};
use super::super::{Decoder, NEWEST, OLDEST, PacketDecoder};
use super::session::profile_properties;
use super::text::json_component;
use super::{GAME, V1_20_2, V1_20_5, V1_21, V1_21_2, V26_2, decoder, uuid};

const HANDSHAKE: &[Phase] = &[Phase::Handshake];
const STATUS: &[Phase] = &[Phase::Status];
const LOGIN: &[Phase] = &[Phase::Login];

pub static DECODERS: &[PacketDecoder] = &[
    decoder(HANDSHAKE, Serverbound, "intention", OLDEST..=NEWEST, |d| {
        d.var_int("protocol_version")?;
        d.string("host")?;
        d.u16("port")?;
        // `ClientIntent`; 1.20.5 added transfers.
        let intents: &[&str] = &["", "status", "login", "transfer"];
        let known = if d.protocol() >= V1_20_5 { 4 } else { 3 };
        d.variant("intent", Reader::var_int, &intents[..known])?;
        Ok(())
    }),
    decoder(
        STATUS,
        Serverbound,
        "status_request",
        OLDEST..=NEWEST,
        |_| Ok(()),
    ),
    decoder(
        STATUS,
        Clientbound,
        "status_response",
        OLDEST..=NEWEST,
        |d| {
            d.string("json")?;
            Ok(())
        },
    ),
    decoder(STATUS, Serverbound, "ping_request", OLDEST..=NEWEST, time),
    decoder(GAME, Serverbound, "ping_request", V1_20_2..=NEWEST, time),
    decoder(STATUS, Clientbound, "pong_response", OLDEST..=NEWEST, time),
    decoder(GAME, Clientbound, "pong_response", V1_20_2..=NEWEST, time),
    decoder(LOGIN, Serverbound, "hello", OLDEST..=NEWEST, |d| {
        d.string("name")?;
        if d.protocol() >= V1_20_2 {
            d.uuid("profile_id")?;
        } else {
            d.optional("profile_id", uuid)?;
        }
        Ok(())
    }),
    decoder(
        LOGIN,
        Clientbound,
        "login_compression",
        OLDEST..=NEWEST,
        |d| {
            d.var_int("threshold")?;
            Ok(())
        },
    ),
    decoder(
        LOGIN,
        Clientbound,
        "login_disconnect",
        OLDEST..=NEWEST,
        |d| json_component(d, "reason"),
    ),
    decoder(LOGIN, Clientbound, "game_profile", OLDEST..=V1_21, |d| {
        game_profile(d)?;
        if d.protocol() >= V1_20_5 {
            d.bool("strict_error_handling")?;
        }
        Ok(())
    }),
    decoder(
        LOGIN,
        Clientbound,
        "login_finished",
        V1_21_2..=NEWEST,
        |d| {
            game_profile(d)?;
            if d.protocol() >= V26_2 {
                d.uuid("session_id")?;
            }
            Ok(())
        },
    ),
    decoder(LOGIN, Clientbound, "hello", OLDEST..=NEWEST, |d| {
        d.string("server_id")?;
        d.byte_array("public_key")?;
        d.byte_array("challenge")?;
        if d.protocol() >= V1_20_5 {
            d.bool("should_authenticate")?;
        }
        Ok(())
    }),
    decoder(LOGIN, Serverbound, "key", OLDEST..=NEWEST, |d| {
        d.byte_array("key")?;
        d.byte_array("encrypted_challenge")?;
        Ok(())
    }),
    decoder(LOGIN, Clientbound, "custom_query", OLDEST..=NEWEST, |d| {
        d.var_int("transaction_id")?;
        d.string("channel")?;
        d.rest("payload")?;
        Ok(())
    }),
    decoder(
        LOGIN,
        Serverbound,
        "custom_query",
        OLDEST..=OLDEST,
        query_answer,
    ),
    decoder(
        LOGIN,
        Serverbound,
        "custom_query_answer",
        V1_20_2..=NEWEST,
        query_answer,
    ),
    decoder(
        LOGIN,
        Serverbound,
        "login_acknowledged",
        V1_20_2..=NEWEST,
        |_| Ok(()),
    ),
];

/// A login query's answer: its transaction and an optional payload filling
/// the rest of the frame.
fn query_answer(d: &mut Decoder) -> Result<()> {
    d.var_int("transaction_id")?;
    d.optional("payload", |d, name| d.rest(name))?;
    Ok(())
}

fn time(d: &mut Decoder) -> Result<()> {
    d.i64("time")?;
    Ok(())
}

/// `ByteBufCodecs.GAME_PROFILE`, and `FriendlyByteBuf.readGameProfile` before
/// it, which has the same bytes with looser limits.
pub fn game_profile(d: &mut Decoder) -> Result<()> {
    d.uuid("uuid")?;
    d.string("name")?;
    profile_properties(d)
}

#[cfg(test)]
mod tests {
    use peel_trace::Direction;

    use super::super::super::testing::{NO_FIELDS, assert_decodes};
    use super::super::{V1_20_3, V26_1};
    use super::*;

    const UUID: [u8; 16] = [
        0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 1, 2, 3, 4, 5, 6, 7, 8,
    ];
    const UUID_TEXT: &str = "12345678-9abc-def0-0102-030405060708";

    fn check(
        phase: Phase,
        direction: Direction,
        range: std::ops::RangeInclusive<i32>,
        name: &str,
        body: &[u8],
        expected: &[impl AsRef<str>],
    ) {
        assert_decodes(range, phase, direction, name, body, expected);
    }

    #[test]
    fn intention() {
        let body = [&[0x89, 0x06, 9][..], b"localhost", &[0x63, 0xdd, 3]].concat();
        let lines = |intent: &str| {
            [
                "protocol_version: 777 [1..3]".to_string(),
                "host: \"localhost\" [3..13]".to_string(),
                "port: 25565 [13..15]".to_string(),
                format!("intent: {intent} (3) [15..16]"),
            ]
        };
        for (range, intent) in [
            (OLDEST..=V1_20_3, "unknown"),
            (V1_20_5..=NEWEST, "transfer"),
        ] {
            let expected = lines(intent);
            check(
                Phase::Handshake,
                Serverbound,
                range,
                "intention",
                &body,
                &expected,
            );
        }
    }

    #[test]
    fn status_and_ping() {
        check(
            Phase::Status,
            Serverbound,
            OLDEST..=NEWEST,
            "status_request",
            &[],
            NO_FIELDS,
        );
        check(
            Phase::Status,
            Clientbound,
            OLDEST..=NEWEST,
            "status_response",
            b"\x02{}",
            &["json: \"{}\" [1..4]"],
        );
        for (phase, range) in [
            (Phase::Status, OLDEST..=NEWEST),
            (Phase::Game, V1_20_2..=NEWEST),
        ] {
            for (direction, name) in [
                (Serverbound, "ping_request"),
                (Clientbound, "pong_response"),
            ] {
                check(
                    phase,
                    direction,
                    range.clone(),
                    name,
                    &5i64.to_be_bytes(),
                    &["time: 5 [1..9]"],
                );
            }
        }
    }

    #[test]
    fn hello() {
        check(
            Phase::Login,
            Serverbound,
            OLDEST..=OLDEST,
            "hello",
            &[&[3][..], b"Bob", &[1], &UUID].concat(),
            &[
                "name: \"Bob\" [1..5]",
                &format!("profile_id: {UUID_TEXT} [5..22]"),
            ],
        );
        check(
            Phase::Login,
            Serverbound,
            V1_20_2..=NEWEST,
            "hello",
            &[&[3][..], b"Bob", &UUID].concat(),
            &[
                "name: \"Bob\" [1..5]",
                &format!("profile_id: {UUID_TEXT} [5..21]"),
            ],
        );
    }

    #[test]
    fn login_packets() {
        check(
            Phase::Login,
            Clientbound,
            OLDEST..=NEWEST,
            "login_compression",
            &[0x80, 0x02],
            &["threshold: 256 [1..3]"],
        );
        check(
            Phase::Login,
            Serverbound,
            V1_20_2..=NEWEST,
            "login_acknowledged",
            &[],
            NO_FIELDS,
        );
        check(
            Phase::Login,
            Clientbound,
            OLDEST..=NEWEST,
            "login_disconnect",
            b"\x0d{\"text\":\"no\"}",
            &[
                "reason: \"no\" [1..15]",
                "  json: \"{\\\"text\\\":\\\"no\\\"}\" [1..15]",
            ],
        );
    }

    #[test]
    fn game_profile_by_version() {
        let property = [
            &[1][..],
            &[8],
            b"textures",
            &[3],
            b"e30",
            &[1],
            &[3],
            b"sig",
        ]
        .concat();
        let profile = [&UUID[..], &[3], b"Bob", &property].concat();
        let lines = [
            format!("uuid: {UUID_TEXT} [1..17]"),
            "name: \"Bob\" [17..21]".into(),
            "properties: 1 item [21..40]".into(),
            "  0:  [22..40]".into(),
            "    name: \"textures\" [22..31]".into(),
            "    value: \"e30\" [31..35]".into(),
            "    signature: \"sig\" [35..40]".into(),
        ];
        let expected = |extra: &[&str]| -> Vec<String> {
            lines
                .iter()
                .cloned()
                .chain(extra.iter().map(|s| s.to_string()))
                .collect()
        };
        let cases = [
            (
                "game_profile",
                OLDEST..=V1_20_3,
                vec![],
                expected(NO_FIELDS),
            ),
            (
                "game_profile",
                V1_20_5..=V1_21,
                vec![1],
                expected(&["strict_error_handling: true [40..41]"]),
            ),
            (
                "login_finished",
                V1_21_2..=V26_1,
                vec![],
                expected(NO_FIELDS),
            ),
            (
                "login_finished",
                V26_2..=NEWEST,
                UUID.to_vec(),
                expected(&[&format!("session_id: {UUID_TEXT} [40..56]")]),
            ),
        ];
        for (name, range, tail, expected) in cases {
            check(
                Phase::Login,
                Clientbound,
                range,
                name,
                &[&profile[..], &tail].concat(),
                &expected,
            );
        }
    }

    #[test]
    fn encryption_and_queries() {
        let hello = [1, b's', 2, 9, 9, 1, 7];
        let hello_lines = [
            "server_id: \"s\" [1..3]",
            "public_key: 2 bytes [3..6]",
            "challenge: 1 byte [6..8]",
        ];
        check(
            Phase::Login,
            Clientbound,
            OLDEST..=V1_20_5 - 1,
            "hello",
            &hello,
            &hello_lines,
        );
        check(
            Phase::Login,
            Clientbound,
            V1_20_5..=NEWEST,
            "hello",
            &[&hello[..], &[1]].concat(),
            &[&hello_lines[..], &["should_authenticate: true [8..9]"]].concat(),
        );
        check(
            Phase::Login,
            Serverbound,
            OLDEST..=NEWEST,
            "key",
            &[2, 9, 9, 1, 7],
            &["key: 2 bytes [1..4]", "encrypted_challenge: 1 byte [4..6]"],
        );
        check(
            Phase::Login,
            Clientbound,
            OLDEST..=NEWEST,
            "custom_query",
            &[3, 3, b'a', b':', b'b', 9, 9],
            &[
                "transaction_id: 3 [1..2]",
                "channel: \"a:b\" [2..6]",
                "payload: 2 bytes [6..8]",
            ],
        );
        check(
            Phase::Login,
            Serverbound,
            OLDEST..=OLDEST,
            "custom_query",
            &[3, 1, 9],
            &["transaction_id: 3 [1..2]", "payload: 1 byte [2..4]"],
        );
        check(
            Phase::Login,
            Serverbound,
            V1_20_2..=NEWEST,
            "custom_query_answer",
            &[3, 0],
            &["transaction_id: 3 [1..2]", "payload: absent [2..3]"],
        );
    }
}
