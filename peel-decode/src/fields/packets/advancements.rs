//! The advancement tree, the player's progress and statistics.

use peel_trace::Direction::Clientbound;

use super::super::reader::{Reader, Result};
use super::super::{Decoder, NEWEST, OLDEST, PacketDecoder};
use super::items::stack_or_template;
use super::text::component;
use super::{V1_20_2, V1_21_5, V26_3, game, string};

pub static DECODERS: &[PacketDecoder] = &[
    game(
        Clientbound,
        "select_advancements_tab",
        OLDEST..=NEWEST,
        |d| {
            d.optional("tab", string)?;
            Ok(())
        },
    ),
    game(Clientbound, "award_stats", OLDEST..=NEWEST, |d| {
        d.list("stats", |d, name| {
            d.group(name, |d| {
                d.raw_id("type")?;
                d.raw_id("value")?;
                d.var_int("amount")?;
                Ok(())
            })
        })?;
        Ok(())
    }),
    game(Clientbound, "update_advancements", OLDEST..=NEWEST, |d| {
        d.bool("reset")?;
        d.list("added", |d, name| d.group(name, advancement))?;
        d.list("removed", string)?;
        d.list("progress", |d, name| {
            d.group(name, |d| {
                d.string("id")?;
                d.list("criteria", |d, name| {
                    d.group(name, |d| {
                        d.string("name")?;
                        d.optional("obtained_at", |d, name| d.i64(name))?;
                        Ok(())
                    })
                })?;
                Ok(())
            })
        })?;
        if d.protocol() >= V1_21_5 {
            d.bool("show_advancements")?;
        }
        Ok(())
    }),
];

/// An id and an `Advancement`. 1.20.1 also listed the criteria names, and
/// 26.3 moved the tab position out of the display to after it.
fn advancement(d: &mut Decoder) -> Result<()> {
    let protocol = d.protocol();
    d.string("id")?;
    d.optional("parent", string)?;
    d.optional("display", |d, name| d.group(name, display))?;
    if protocol < V1_20_2 {
        d.list("criteria", string)?;
    }
    d.list("requirements", |d, name| d.list(name, string).map(drop))?;
    d.bool("sends_telemetry_event")?;
    if protocol >= V26_3 {
        d.f32("x")?;
        d.f32("y")?;
    }
    Ok(())
}

/// `DisplayInfo`.
fn display(d: &mut Decoder) -> Result<()> {
    component(d, "title")?;
    component(d, "description")?;
    stack_or_template(d, "icon".into())?;
    d.variant("type", Reader::var_int, &["task", "challenge", "goal"])?;
    let flags = d.flags(
        "flags",
        Reader::i32,
        &["has_background", "show_toast", "hidden"],
    )?;
    if flags & 1 != 0 {
        d.string("background")?;
    }
    if d.protocol() < V26_3 {
        d.f32("x")?;
        d.f32("y")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use peel_trace::Phase::Game;
    use pomme_protocol::ClientRegistry;

    use super::super::super::testing::{assert_decodes, entry, f32s, protocols};
    use super::super::{V1_20_5, V26_1};
    use super::*;

    /// Pushes `text` spanning `len` bytes from `at`, then moves `at` past it.
    fn push(lines: &mut Vec<String>, at: &mut usize, text: &str, len: usize) {
        lines.push(format!("{text} [{at}..{}]", *at + len));
        *at += len;
    }

    #[test]
    fn advancements_by_version() {
        for protocol in protocols(OLDEST..=NEWEST) {
            let mut body = vec![0, 1, 1, b'a', 0, 0];
            let mut lines = vec![
                "reset: false [1..2]".to_string(),
                "added: 1 item [2..END]".into(),
                "  0:  [3..END]".into(),
                "    id: \"a\" [3..5]".into(),
                "    parent: absent [5..6]".into(),
                "    display: absent [6..7]".into(),
            ];
            let mut at = 7;
            if protocol < V1_20_2 {
                body.extend([1, 1, b'c']);
                lines.push(format!("    criteria: 1 item [{at}..{}]", at + 3));
                at += 1;
                push(&mut lines, &mut at, "      0: \"c\"", 2);
            }
            body.extend([1, 1, 1, b'c', 0]);
            lines.push(format!("    requirements: 1 item [{at}..{}]", at + 4));
            at += 1;
            lines.push(format!("      0: 1 item [{at}..{}]", at + 3));
            at += 1;
            push(&mut lines, &mut at, "        0: \"c\"", 2);
            push(&mut lines, &mut at, "    sends_telemetry_event: false", 1);
            if protocol >= V26_3 {
                body.extend(f32s(&[1.0, 2.0]));
                push(&mut lines, &mut at, "    x: 1.0", 4);
                push(&mut lines, &mut at, "    y: 2.0", 4);
            }
            let added_end = at;
            body.extend([0, 1, 1, b'a', 1, 1, b'c', 1]);
            body.extend(5i64.to_be_bytes());
            push(&mut lines, &mut at, "removed: 0 items", 1);
            lines.push(format!("progress: 1 item [{at}..{}]", at + 15));
            lines.push(format!("  0:  [{}..{}]", at + 1, at + 15));
            at += 1;
            push(&mut lines, &mut at, "    id: \"a\"", 2);
            lines.push(format!("    criteria: 1 item [{at}..{}]", at + 12));
            lines.push(format!("      0:  [{}..{}]", at + 1, at + 12));
            at += 1;
            push(&mut lines, &mut at, "        name: \"c\"", 2);
            push(&mut lines, &mut at, "        obtained_at: 5", 9);
            if protocol >= V1_21_5 {
                body.push(1);
                push(&mut lines, &mut at, "show_advancements: true", 1);
            }
            let lines: Vec<String> = lines
                .iter()
                .map(|line| line.replace("END", &added_end.to_string()))
                .collect();
            assert_decodes(
                protocol..=protocol,
                Game,
                Clientbound,
                "update_advancements",
                &body,
                &lines,
            );
        }
    }

    #[test]
    fn display_icons_and_positions() {
        let text = |c| [8, 0, 1, c];
        for protocol in protocols(V1_20_5..=NEWEST) {
            let stone = entry(protocol, ClientRegistry::Item, 1);
            let (icon, icon_lines): (&[u8], _) = if protocol >= V26_1 {
                (
                    &[1, 2, 0, 0],
                    [
                        format!("        item: {stone} [15..16]"),
                        "        count: 2 [16..17]".into(),
                    ],
                )
            } else {
                (
                    &[2, 1, 0, 0],
                    [
                        "        count: 2 [15..16]".into(),
                        format!("        item: {stone} [16..17]"),
                    ],
                )
            };
            let mut body = [
                &[0, 1, 1, b'a', 0, 1][..],
                &text(b'T'),
                &text(b'D'),
                icon,
                &[0],
            ]
            .concat();
            body.extend(1i32.to_be_bytes());
            body.extend([1, b'b']);
            let mut display = vec![
                "      title: \"T\" [7..11]".to_string(),
                "      description: \"D\" [11..15]".into(),
                format!("      icon: 2 × {stone} [15..19]"),
            ];
            display.extend(icon_lines);
            display.extend([
                "        components: none [17..19]".into(),
                "      type: task (0) [19..20]".into(),
                "      flags: 0x01 has_background [20..24]".into(),
                "      background: \"b\" [24..26]".into(),
            ]);
            let mut tail = vec![];
            let mut at = 26;
            if protocol < V26_3 {
                body.extend(f32s(&[1.0, 2.0]));
                display.extend([
                    "      x: 1.0 [26..30]".into(),
                    "      y: 2.0 [30..34]".into(),
                ]);
                at = 34;
            }
            let display_end = at;
            body.extend([0, 0]);
            tail.push(format!("    requirements: 0 items [{at}..{}]", at + 1));
            tail.push(format!(
                "    sends_telemetry_event: false [{}..{}]",
                at + 1,
                at + 2
            ));
            at += 2;
            if protocol >= V26_3 {
                body.extend(f32s(&[1.0, 2.0]));
                tail.push(format!("    x: 1.0 [{at}..{}]", at + 4));
                tail.push(format!("    y: 2.0 [{}..{}]", at + 4, at + 8));
                at += 8;
            }
            let mut lines = vec![
                "reset: false [1..2]".to_string(),
                format!("added: 1 item [2..{at}]"),
                format!("  0:  [3..{at}]"),
                "    id: \"a\" [3..5]".into(),
                "    parent: absent [5..6]".into(),
                format!("    display:  [6..{display_end}]"),
            ];
            lines.extend(display);
            lines.extend(tail);
            body.extend([0, 0]);
            lines.push(format!("removed: 0 items [{at}..{}]", at + 1));
            lines.push(format!("progress: 0 items [{}..{}]", at + 1, at + 2));
            if protocol >= V1_21_5 {
                body.push(0);
                lines.push(format!("show_advancements: false [{}..{}]", at + 2, at + 3));
            }
            assert_decodes(
                protocol..=protocol,
                Game,
                Clientbound,
                "update_advancements",
                &body,
                &lines,
            );
        }
    }

    #[test]
    fn select_advancements_tab() {
        assert_decodes(
            OLDEST..=NEWEST,
            Game,
            Clientbound,
            "select_advancements_tab",
            &[1, 3, b'a', b':', b'b'],
            &["tab: \"a:b\" [1..6]"],
        );
    }

    #[test]
    fn award_stats() {
        assert_decodes(
            OLDEST..=NEWEST,
            Game,
            Clientbound,
            "award_stats",
            &[1, 8, 3, 5],
            &[
                "stats: 1 item [1..5]",
                "  0:  [2..5]",
                "    type: #8 [2..3]",
                "    value: #3 [3..4]",
                "    amount: 5 [4..5]",
            ],
        );
    }
}
