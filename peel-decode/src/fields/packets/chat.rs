//! Chat packets and the signed-chat codecs they share.

use peel_trace::Direction::{Clientbound, Serverbound};

use super::super::decoder::set_bits;
use super::super::reader::{Reader, Result};
use super::super::{Decoder, NEWEST, Name, OLDEST, PacketDecoder};
use super::text::{component, optional_component};
use super::{V1_20_5, V1_21, V1_21_5, V26_3, game, nbt, string, variant};

pub static DECODERS: &[PacketDecoder] = &[
    game(Clientbound, "system_chat", OLDEST..=NEWEST, |d| {
        component(d, "content")?;
        d.bool("overlay")?;
        Ok(())
    }),
    game(Clientbound, "player_chat", OLDEST..=NEWEST, |d| {
        if d.protocol() >= V1_21_5 {
            d.var_int("global_index")?;
        }
        d.uuid("sender")?;
        d.var_int("index")?;
        d.optional("signature", signature)?;
        d.group("body", |d| {
            d.string("content")?;
            d.i64("timestamp")?;
            d.i64("salt")?;
            d.list("last_seen", packed_signature)?;
            Ok(())
        })?;
        optional_component(d, "unsigned_content")?;
        filter_mask(d)?;
        chat_type(d)
    }),
    game(Clientbound, "disguised_chat", OLDEST..=NEWEST, |d| {
        component(d, "message")?;
        chat_type(d)
    }),
    game(Clientbound, "delete_chat", OLDEST..=NEWEST, |d| {
        packed_signature(d, "message_signature".into())
    }),
    game(Clientbound, "command_suggestions", OLDEST..=NEWEST, |d| {
        d.var_int("id")?;
        d.var_int("start")?;
        d.var_int("length")?;
        d.list("suggestions", |d, name| {
            d.group(name, |d| {
                d.string("text")?;
                optional_component(d, "tooltip")
            })
        })?;
        Ok(())
    }),
    game(
        Clientbound,
        "custom_chat_completions",
        OLDEST..=NEWEST,
        |d| {
            d.variant("action", Reader::var_int, &["add", "remove", "set"])?;
            d.list("entries", string)?;
            Ok(())
        },
    ),
    game(Clientbound, "set_action_bar_text", OLDEST..=NEWEST, text),
    game(Clientbound, "set_title_text", OLDEST..=NEWEST, text),
    game(Clientbound, "set_subtitle_text", OLDEST..=NEWEST, text),
    game(Clientbound, "set_titles_animation", OLDEST..=NEWEST, |d| {
        d.i32("fade_in")?;
        d.i32("stay")?;
        d.i32("fade_out")?;
        Ok(())
    }),
    game(Clientbound, "clear_titles", OLDEST..=NEWEST, |d| {
        d.bool("reset_times")?;
        Ok(())
    }),
    game(Clientbound, "boss_event", OLDEST..=NEWEST, |d| {
        d.uuid("id")?;
        let operation = d.variant(
            "operation",
            Reader::var_int,
            &[
                "add",
                "remove",
                "update_progress",
                "update_name",
                "update_style",
                "update_properties",
            ],
        )?;
        if matches!(operation, 0 | 3) {
            component(d, "name")?;
        }
        if matches!(operation, 0 | 2) {
            d.f32("progress")?;
        }
        if matches!(operation, 0 | 4) {
            d.variant(
                "color",
                Reader::var_int,
                &["pink", "blue", "red", "green", "yellow", "purple", "white"],
            )?;
            d.variant(
                "overlay",
                Reader::var_int,
                &[
                    "progress",
                    "notched_6",
                    "notched_10",
                    "notched_12",
                    "notched_20",
                ],
            )?;
        }
        if matches!(operation, 0 | 5) {
            d.flags(
                "flags",
                Reader::u8,
                &["darken_screen", "play_music", "create_world_fog"],
            )?;
        }
        Ok(())
    }),
    game(Clientbound, "tab_list", OLDEST..=NEWEST, |d| {
        component(d, "header")?;
        component(d, "footer")
    }),
    game(Clientbound, "server_data", OLDEST..=NEWEST, |d| {
        component(d, "motd")?;
        d.optional("icon", |d, name| d.byte_array(name))?;
        if d.protocol() < V1_20_5 {
            d.bool("enforces_secure_chat")?;
        }
        Ok(())
    }),
    game(Serverbound, "chat", OLDEST..=NEWEST, |d| {
        d.string("message")?;
        d.i64("timestamp")?;
        d.i64("salt")?;
        d.optional("signature", signature)?;
        last_seen_update(d)
    }),
    game(
        Serverbound,
        "chat_command",
        OLDEST..=V1_20_5 - 1,
        signed_command,
    ),
    game(Serverbound, "chat_command", V1_20_5..=NEWEST, |d| {
        d.string("command")?;
        Ok(())
    }),
    game(
        Serverbound,
        "chat_command_signed",
        V1_20_5..=NEWEST,
        signed_command,
    ),
    game(Serverbound, "chat_ack", OLDEST..=NEWEST, |d| {
        d.var_int("offset")?;
        Ok(())
    }),
    game(
        Serverbound,
        "chat_session_update",
        OLDEST..=NEWEST,
        chat_session,
    ),
];

/// `RemoteChatSession.Data`: the session id and its profile public key.
pub fn chat_session(d: &mut Decoder) -> Result<()> {
    d.uuid("session_id")?;
    d.i64("expires_at")?;
    d.byte_array("public_key")?;
    d.byte_array("key_signature")?;
    Ok(())
}

fn text(d: &mut Decoder) -> Result<()> {
    component(d, "text")
}

/// `MessageSignature`: 256 bytes, no length.
fn signature(d: &mut Decoder, name: Name) -> Result<()> {
    d.bytes(name, 256)?;
    Ok(())
}

/// `MessageSignature.Packed`: 0 and a full signature, or a cached signature's
/// index plus one.
fn packed_signature(d: &mut Decoder, name: Name) -> Result<()> {
    d.prefixed(Reader::var_int, |d, id| match id {
        0 => signature(d, name),
        id => d.note(name, &format!("cached #{}", id - 1)),
    })
}

/// `FilterMask`: a type, then a bit set for partial filtering, as longs until
/// 26.3 sent bytes.
fn filter_mask(d: &mut Decoder) -> Result<()> {
    let kind = d.variant(
        "filter",
        Reader::var_int,
        &["pass_through", "fully_filtered", "partially_filtered"],
    )?;
    if kind == 2 {
        if d.protocol() >= V26_3 {
            d.byte_array("mask")?;
        } else {
            d.long_array("mask")?;
        }
    }
    Ok(())
}

/// `ChatType.Bound`: the chat type, by registry id until 1.21 made it a
/// holder that can carry the type inline, then the sender and target names.
fn chat_type(d: &mut Decoder) -> Result<()> {
    d.group("chat_type", |d| {
        if d.protocol() >= V1_21 {
            d.prefixed(Reader::var_int, |d, holder| match holder {
                0 => d.group("type", |d| {
                    decoration(d, "chat")?;
                    decoration(d, "narration")
                }),
                id => d.note("type", &format!("#{}", id - 1)),
            })?;
        } else {
            d.raw_id("type")?;
        }
        component(d, "name")?;
        optional_component(d, "target_name")
    })
}

/// `ChatTypeDecoration`.
fn decoration(d: &mut Decoder, name: &'static str) -> Result<()> {
    d.group(name, |d| {
        d.string("translation_key")?;
        d.list("parameters", |d, name| {
            variant(d, name, &["sender", "target", "content"])
        })?;
        nbt::root(d, "style")?;
        Ok(())
    })
}

/// `ServerboundChatCommandPacket` before 1.20.5, and
/// `ServerboundChatCommandSignedPacket` after.
fn signed_command(d: &mut Decoder) -> Result<()> {
    d.string("command")?;
    d.i64("timestamp")?;
    d.i64("salt")?;
    d.list("argument_signatures", |d, name| {
        d.group(name, |d| {
            d.string("name")?;
            signature(d, "signature".into())
        })
    })?;
    last_seen_update(d)
}

/// `LastSeenMessages.Update`: an offset and 20 acknowledgement bits, plus a
/// checksum from 1.21.5.
fn last_seen_update(d: &mut Decoder) -> Result<()> {
    d.group("last_seen", |d| {
        d.var_int("offset")?;
        d.read(
            "acknowledged",
            |r| r.bytes(3, "20 bits"),
            |bytes| set_bits((0..20).map(|i| bytes[i / 8] >> (i % 8) & 1 == 1)),
        )?;
        if d.protocol() >= V1_21_5 {
            d.i8("checksum")?;
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use std::ops::RangeInclusive;

    use peel_trace::Direction;
    use peel_trace::Phase::Game;

    use super::super::super::testing::assert_decodes;
    use super::super::{V1_20_3, V1_21_5};
    use super::*;

    const UUID: [u8; 16] = [0x12; 16];
    const UUID_TEXT: &str = "12121212-1212-1212-1212-121212121212";
    /// A network NBT string tag.
    const BOB: &[u8] = &[8, 0, 3, b'B', b'o', b'b'];

    fn check(
        direction: Direction,
        range: RangeInclusive<i32>,
        name: &str,
        body: &[u8],
        expected: &[impl AsRef<str>],
    ) {
        assert_decodes(range, Game, direction, name, body, expected);
    }

    fn lines(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|s| s.to_string()).collect()
    }

    /// The signed body shared by every `player_chat` test, starting at `at`.
    fn body_lines(at: usize) -> Vec<String> {
        vec![
            format!("body:  [{at}..{}]", at + 21),
            format!("  content: \"hi\" [{at}..{}]", at + 3),
            format!("  timestamp: 1000 [{}..{}]", at + 3, at + 11),
            format!("  salt: 7 [{}..{}]", at + 11, at + 19),
            format!("  last_seen: 1 item [{}..{}]", at + 19, at + 21),
            format!("    0: cached #2 [{}..{}]", at + 20, at + 21),
        ]
    }

    fn body() -> Vec<u8> {
        [
            &[2, b'h', b'i'][..],
            &1000i64.to_be_bytes(),
            &7i64.to_be_bytes(),
            &[1, 3],
        ]
        .concat()
    }

    #[test]
    fn player_chat_before_the_global_index() {
        let bytes =
            |chat_type: u8| [&UUID[..], &[0, 0], &body(), &[0, 0, chat_type], BOB, &[0]].concat();
        let mut expected = lines(&[
            &format!("sender: {UUID_TEXT} [1..17]"),
            "index: 0 [17..18]",
            "signature: absent [18..19]",
        ]);
        expected.extend(body_lines(19));
        expected.extend(lines(&[
            "unsigned_content: absent [40..41]",
            "filter: pass_through (0) [41..42]",
            "chat_type:  [42..50]",
            "  type: #0 [42..43]",
            "  name: \"Bob\" [43..49]",
            "  target_name: absent [49..50]",
        ]));
        check(
            Clientbound,
            V1_20_3..=V1_21 - 1,
            "player_chat",
            &bytes(0),
            &expected,
        );
        check(
            Clientbound,
            V1_21..=V1_21_5 - 1,
            "player_chat",
            &bytes(1),
            &expected,
        );
    }

    #[test]
    fn player_chat_masks_by_version() {
        let bytes = |mask: &[u8]| {
            [
                &[5][..],
                &UUID,
                &[0, 0],
                &body(),
                &[0, 2],
                mask,
                &[1],
                BOB,
                &[0],
            ]
            .concat()
        };
        let expected = |mask: &str, end: usize| {
            let mut lines = lines(&[
                "global_index: 5 [1..2]",
                &format!("sender: {UUID_TEXT} [2..18]"),
                "index: 0 [18..19]",
                "signature: absent [19..20]",
            ]);
            lines.extend(body_lines(20));
            lines.extend([
                "unsigned_content: absent [41..42]".to_string(),
                "filter: partially_filtered (2) [42..43]".to_string(),
                format!("mask: {mask} [43..{end}]"),
                format!("chat_type:  [{end}..{}]", end + 8),
                format!("  type: #0 [{end}..{}]", end + 1),
                format!("  name: \"Bob\" [{}..{}]", end + 1, end + 7),
                format!("  target_name: absent [{}..{}]", end + 7, end + 8),
            ]);
            lines
        };
        check(
            Clientbound,
            V1_21_5..=V26_3 - 1,
            "player_chat",
            &bytes(&[&[1][..], &[0; 8]].concat()),
            &expected("1 long", 52),
        );
        check(
            Clientbound,
            V26_3..=NEWEST,
            "player_chat",
            &bytes(&[1, 5]),
            &expected("1 byte", 45),
        );
    }

    #[test]
    fn inline_chat_type() {
        let decoration = [&[4][..], b"chat", &[1, 2], &[10, 0]].concat();
        let body = [
            &[8, 0, 1, b'm'][..],
            &[0],
            &decoration,
            &decoration,
            &[8, 0, 1, b'n'],
            &[0],
        ]
        .concat();
        let decoration_lines = |name: &str, at: usize| {
            [
                format!("    {name}:  [{at}..{}]", at + 9),
                format!("      translation_key: \"chat\" [{at}..{}]", at + 5),
                format!("      parameters: 1 item [{}..{}]", at + 5, at + 7),
                format!("        0: content (2) [{}..{}]", at + 6, at + 7),
                format!("      style: 0 keys [{}..{}]", at + 7, at + 9),
            ]
        };
        let mut expected = lines(&[
            "message: \"m\" [1..5]",
            "chat_type:  [5..29]",
            "  type:  [5..24]",
        ]);
        expected.extend(decoration_lines("chat", 6));
        expected.extend(decoration_lines("narration", 15));
        expected.extend(lines(&[
            "  name: \"n\" [24..28]",
            "  target_name: absent [28..29]",
        ]));
        check(
            Clientbound,
            V1_21..=NEWEST,
            "disguised_chat",
            &body,
            &expected,
        );
    }

    #[test]
    fn simple_clientbound() {
        let hi = [8, 0, 2, b'h', b'i'];
        check(
            Clientbound,
            V1_20_3..=NEWEST,
            "system_chat",
            &[&hi[..], &[1]].concat(),
            &["content: \"hi\" [1..6]", "overlay: true [6..7]"],
        );
        for name in ["set_action_bar_text", "set_title_text", "set_subtitle_text"] {
            check(
                Clientbound,
                V1_20_3..=NEWEST,
                name,
                &hi,
                &["text: \"hi\" [1..6]"],
            );
        }
        check(
            Clientbound,
            OLDEST..=NEWEST,
            "set_titles_animation",
            &[
                &10i32.to_be_bytes()[..],
                &70i32.to_be_bytes(),
                &20i32.to_be_bytes(),
            ]
            .concat(),
            &[
                "fade_in: 10 [1..5]",
                "stay: 70 [5..9]",
                "fade_out: 20 [9..13]",
            ],
        );
        check(
            Clientbound,
            V1_20_3..=NEWEST,
            "tab_list",
            &[&hi[..], &hi].concat(),
            &["header: \"hi\" [1..6]", "footer: \"hi\" [6..11]"],
        );
        let motd = ["motd: \"hi\" [1..6]", "icon: absent [6..7]"];
        check(
            Clientbound,
            V1_20_3..=V1_20_3,
            "server_data",
            &[&hi[..], &[0, 1]].concat(),
            &[&motd[..], &["enforces_secure_chat: true [7..8]"]].concat(),
        );
        check(
            Clientbound,
            V1_20_5..=NEWEST,
            "server_data",
            &[&hi[..], &[0]].concat(),
            &motd,
        );
    }

    #[test]
    fn chat_message() {
        let bytes = [
            &[2, b'h', b'i'][..],
            &5i64.to_be_bytes(),
            &6i64.to_be_bytes(),
            &[0, 0, 0x01, 0x00, 0x08],
        ]
        .concat();
        let expected = lines(&[
            "message: \"hi\" [1..4]",
            "timestamp: 5 [4..12]",
            "salt: 6 [12..20]",
            "signature: absent [20..21]",
            "last_seen:  [21..25]",
            "  offset: 0 [21..22]",
            "  acknowledged: 0, 19 [22..25]",
        ]);
        check(Serverbound, OLDEST..=V1_21_5 - 1, "chat", &bytes, &expected);
        let mut newer = expected.clone();
        newer[4] = "last_seen:  [21..26]".into();
        newer.push("  checksum: 42 [25..26]".into());
        check(
            Serverbound,
            V1_21_5..=NEWEST,
            "chat",
            &[&bytes[..], &[42]].concat(),
            &newer,
        );
    }

    #[test]
    fn commands() {
        check(
            Serverbound,
            V1_20_5..=NEWEST,
            "chat_command",
            &[2, b't', b'p'],
            &["command: \"tp\" [1..4]"],
        );
        let signed = [
            &[2, b't', b'p'][..],
            &5i64.to_be_bytes(),
            &6i64.to_be_bytes(),
            &[1, 3, b'm', b's', b'g'],
            &[0; 256],
            &[1, 0, 0, 0],
        ]
        .concat();
        let expected = lines(&[
            "command: \"tp\" [1..4]",
            "timestamp: 5 [4..12]",
            "salt: 6 [12..20]",
            "argument_signatures: 1 item [20..281]",
            "  0:  [21..281]",
            "    name: \"msg\" [21..25]",
            "    signature: 256 bytes [25..281]",
            "last_seen:  [281..285]",
            "  offset: 1 [281..282]",
            "  acknowledged: none [282..285]",
        ]);
        check(
            Serverbound,
            OLDEST..=V1_20_5 - 1,
            "chat_command",
            &signed,
            &expected,
        );
        check(
            Serverbound,
            V1_20_5..=V1_21_5 - 1,
            "chat_command_signed",
            &signed,
            &expected,
        );
        check(
            Serverbound,
            OLDEST..=NEWEST,
            "chat_ack",
            &[3],
            &["offset: 3 [1..2]"],
        );
        check(
            Serverbound,
            OLDEST..=NEWEST,
            "chat_session_update",
            &[&UUID[..], &9i64.to_be_bytes(), &[2, 0xa, 0xb, 1, 0xc]].concat(),
            &[
                &format!("session_id: {UUID_TEXT} [1..17]"),
                "expires_at: 9 [17..25]",
                "public_key: 2 bytes [25..28]",
                "key_signature: 1 byte [28..30]",
            ],
        );
    }

    #[test]
    fn deleting_and_suggesting() {
        check(
            Clientbound,
            OLDEST..=NEWEST,
            "delete_chat",
            &[5],
            &["message_signature: cached #4 [1..2]"],
        );
        check(
            Clientbound,
            OLDEST..=NEWEST,
            "delete_chat",
            &[&[0][..], &[7; 256]].concat(),
            &["message_signature: 256 bytes [1..258]"],
        );
        check(
            Clientbound,
            V1_20_3..=NEWEST,
            "command_suggestions",
            &[1, 0, 3, 1, 2, b't', b'p', 1, 8, 0, 1, b'x'],
            &[
                "id: 1 [1..2]",
                "start: 0 [2..3]",
                "length: 3 [3..4]",
                "suggestions: 1 item [4..13]",
                "  0:  [5..13]",
                "    text: \"tp\" [5..8]",
                "    tooltip: \"x\" [8..13]",
            ],
        );
        check(
            Clientbound,
            OLDEST..=NEWEST,
            "custom_chat_completions",
            &[2, 1, 1, b'a'],
            &[
                "action: set (2) [1..2]",
                "entries: 1 item [2..5]",
                "  0: \"a\" [3..5]",
            ],
        );
    }

    #[test]
    fn titles_and_boss_bars() {
        check(
            Clientbound,
            OLDEST..=NEWEST,
            "clear_titles",
            &[1],
            &["reset_times: true [1..2]"],
        );
        let id = format!("id: {UUID_TEXT} [1..17]");
        check(
            Clientbound,
            V1_20_3..=NEWEST,
            "boss_event",
            &[
                &UUID[..],
                &[0, 8, 0, 1, b'N'],
                &0.5f32.to_be_bytes(),
                &[2, 1, 5],
            ]
            .concat(),
            &[
                &id,
                "operation: add (0) [17..18]",
                "name: \"N\" [18..22]",
                "progress: 0.5 [22..26]",
                "color: red (2) [26..27]",
                "overlay: notched_6 (1) [27..28]",
                "flags: 0x05 darken_screen | create_world_fog [28..29]",
            ],
        );
        check(
            Clientbound,
            OLDEST..=NEWEST,
            "boss_event",
            &[&UUID[..], &[4, 6, 4]].concat(),
            &[
                &id,
                "operation: update_style (4) [17..18]",
                "color: white (6) [18..19]",
                "overlay: notched_20 (4) [19..20]",
            ],
        );
        check(
            Clientbound,
            OLDEST..=NEWEST,
            "boss_event",
            &[&UUID[..], &[1]].concat(),
            &[&id, "operation: remove (1) [17..18]"],
        );
    }
}
