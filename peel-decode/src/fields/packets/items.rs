//! Item stacks and the container packets that carry them.

use peel_trace::Direction::{Clientbound, Serverbound};
use pomme_protocol::ClientRegistry;

use super::super::reader::{Reader, Result};
use super::super::{Decoder, NEWEST, Name, OLDEST, PacketDecoder};
use super::components::{removed_component, typed_component};
use super::text::component;
use super::{V1_20_3, V1_20_5, V1_21, V1_21_2, V1_21_5, V1_21_11, V26_1, game, nbt};

pub static DECODERS: &[PacketDecoder] = &[
    game(Clientbound, "container_set_slot", OLDEST..=NEWEST, |d| {
        container_id(d, true)?;
        d.var_int("state_id")?;
        d.i16("slot")?;
        item_stack(d, "item")
    }),
    game(Clientbound, "container_set_content", OLDEST..=NEWEST, |d| {
        container_id(d, false)?;
        d.var_int("state_id")?;
        d.list("items", item_stack)?;
        item_stack(d, "carried_item")
    }),
    game(Clientbound, "set_cursor_item", V1_21_2..=NEWEST, |d| {
        item_stack(d, "contents")
    }),
    game(Clientbound, "set_player_inventory", V1_21_2..=NEWEST, |d| {
        d.var_int("slot")?;
        item_stack(d, "contents")
    }),
    game(Clientbound, "container_close", OLDEST..=NEWEST, |d| {
        container_id(d, false)
    }),
    game(Serverbound, "container_close", OLDEST..=NEWEST, |d| {
        container_id(d, true)
    }),
    game(Clientbound, "container_set_data", OLDEST..=NEWEST, |d| {
        container_id(d, false)?;
        d.i16("id")?;
        d.i16("value")?;
        Ok(())
    }),
    game(Clientbound, "open_screen", OLDEST..=NEWEST, |d| {
        d.var_int("container_id")?;
        d.raw_id("menu")?;
        component(d, "title")
    }),
    game(
        Clientbound,
        "horse_screen_open",
        OLDEST..=V1_21_11 - 1,
        mount_screen_open,
    ),
    game(
        Clientbound,
        "mount_screen_open",
        V1_21_11..=NEWEST,
        mount_screen_open,
    ),
    game(Clientbound, "merchant_offers", OLDEST..=NEWEST, |d| {
        d.var_int("container_id")?;
        d.list("offers", |d, name| d.group(name, merchant_offer))?;
        d.var_int("villager_level")?;
        d.var_int("villager_xp")?;
        d.bool("show_progress")?;
        d.bool("can_restock")?;
        Ok(())
    }),
    game(Serverbound, "container_click", OLDEST..=NEWEST, |d| {
        container_id(d, true)?;
        d.var_int("state_id")?;
        d.i16("slot")?;
        d.i8("button")?;
        d.variant(
            "click_type",
            Reader::var_int,
            &[
                "pickup",
                "quick_move",
                "swap",
                "clone",
                "throw",
                "quick_craft",
                "pickup_all",
            ],
        )?;
        d.list("changed_slots", |d, name| {
            d.group(name, |d| {
                d.i16("slot")?;
                remote_stack(d, "item")
            })
        })?;
        remote_stack(d, "carried_item")
    }),
    game(
        Serverbound,
        "container_button_click",
        OLDEST..=NEWEST,
        |d| {
            if d.protocol() >= V1_20_5 {
                d.var_int("container_id")?;
                d.var_int("button_id")?;
            } else {
                d.i8("container_id")?;
                d.i8("button_id")?;
            }
            Ok(())
        },
    ),
    game(
        Serverbound,
        "container_slot_state_changed",
        V1_20_3..=NEWEST,
        |d| {
            d.var_int("slot_id")?;
            d.var_int("container_id")?;
            d.bool("new_state")?;
            Ok(())
        },
    ),
    game(
        Serverbound,
        "set_creative_mode_slot",
        OLDEST..=NEWEST,
        |d| {
            d.i16("slot")?;
            if d.protocol() >= V1_21_5 {
                stack(d, "item", true)
            } else {
                item_stack(d, "item")
            }
        },
    ),
];

/// A container id: a byte, signed in some packets, until 1.21.2 sent a
/// `VarInt`.
pub fn container_id(d: &mut Decoder, signed: bool) -> Result<()> {
    if d.protocol() >= V1_21_2 {
        d.var_int("container_id")?;
    } else if signed {
        d.i8("container_id")?;
    } else {
        d.u8("container_id")?;
    }
    Ok(())
}

/// `ClientboundHorseScreenOpenPacket`, renamed for mounts in 1.21.11. The
/// middle value counted the container's slots until 1.21 sent its columns.
fn mount_screen_open(d: &mut Decoder) -> Result<()> {
    container_id(d, false)?;
    d.var_int(if d.protocol() >= V1_21 {
        "inventory_columns"
    } else {
        "size"
    })?;
    d.i32("entity_id")?;
    Ok(())
}

/// `MerchantOffer`. Costs were item stacks until 1.20.5 sent `ItemCost`.
fn merchant_offer(d: &mut Decoder) -> Result<()> {
    if d.protocol() >= V1_20_5 {
        item_cost(d, "cost_a".into())?;
        item_stack(d, "result")?;
        d.optional("cost_b", item_cost)?;
    } else {
        item_stack(d, "cost_a")?;
        item_stack(d, "result")?;
        item_stack(d, "cost_b")?;
    }
    d.bool("out_of_stock")?;
    for name in ["uses", "max_uses", "xp", "special_price_diff"] {
        d.i32(name)?;
    }
    d.f32("price_multiplier")?;
    d.i32("demand")?;
    Ok(())
}

/// `ItemCost`: an item, a count and the exact components it must have.
fn item_cost(d: &mut Decoder, name: Name) -> Result<()> {
    counted(d, name, |d| {
        let item = d.registry("item", ClientRegistry::Item)?;
        let count = d.var_int("count")?;
        d.list("components", |d, _| typed_component(d, false))?;
        Ok((count, item))
    })
}

/// `ItemStack.OPTIONAL_STREAM_CODEC`, or `FriendlyByteBuf.readItem` before
/// 1.20.5.
pub fn item_stack(d: &mut Decoder, name: impl Into<Name>) -> Result<()> {
    let name = name.into();
    if d.protocol() >= V1_20_5 {
        return stack(d, name, false);
    }
    d.prefixed(Reader::bool, |d, present| {
        if !present {
            return d.note(name, "empty");
        }
        counted(d, name, |d| {
            let item = d.registry("item", ClientRegistry::Item)?;
            let count = d.i8("count")?;
            nbt::root(d, "tag")?;
            Ok((count.into(), item))
        })
    })
}

/// A count, then when it is positive an item and a component patch. The
/// creative-mode slot sends each added component with its length first.
fn stack(d: &mut Decoder, name: impl Into<Name>, delimited: bool) -> Result<()> {
    let name = name.into();
    if d.reader().clone().var_int()? <= 0 {
        d.read(name, Reader::var_int, |_| "empty".into())?;
        return Ok(());
    }
    counted(d, name, |d| {
        let count = d.var_int("count")?;
        let item = d.registry("item", ClientRegistry::Item)?;
        patch(d, delimited)?;
        Ok((count, item))
    })
}

/// `ItemStackTemplate`, from 26.1: never empty, and the item comes first.
pub fn item_template(d: &mut Decoder, name: impl Into<Name>) -> Result<()> {
    counted(d, name.into(), |d| {
        let item = d.registry("item", ClientRegistry::Item)?;
        let count = d.var_int("count")?;
        patch(d, false)?;
        Ok((count, item))
    })
}

/// A stack in a component or particle, which became a template in 26.1.
pub fn stack_or_template(d: &mut Decoder, name: Name) -> Result<()> {
    if d.protocol() >= V26_1 {
        item_template(d, name)
    } else {
        item_stack(d, name)
    }
}

/// What the client says a slot holds: a full stack until 1.21.5 sent a
/// `HashedStack`, with a CRC32C per component instead of its value.
fn remote_stack(d: &mut Decoder, name: &'static str) -> Result<()> {
    if d.protocol() < V1_21_5 {
        return item_stack(d, name);
    }
    d.prefixed(Reader::bool, |d, present| {
        if !present {
            return d.note(name, "empty");
        }
        counted(d, name.into(), |d| {
            let item = d.registry("item", ClientRegistry::Item)?;
            let count = d.var_int("count")?;
            d.list("added", |d, name| {
                d.group(name, |d| {
                    d.registry("type", ClientRegistry::DataComponentType)?;
                    d.read("hash", Reader::i32, |hash| format!("{hash:08x}"))?;
                    Ok(())
                })
            })?;
            d.list("removed", |d, name| {
                d.registry(name, ClientRegistry::DataComponentType)
                    .map(drop)
            })?;
            Ok((count, item))
        })
    })
}

/// Records a stack's fields under `name`, shown as its count and item.
fn counted(
    d: &mut Decoder,
    name: Name,
    fields: impl FnOnce(&mut Decoder) -> Result<(i32, i32)>,
) -> Result<()> {
    let (count, item) = d.group(name, fields)?;
    let item = d.registry_entry(ClientRegistry::Item, item);
    d.show_last(format!("{count} × {item}"));
    Ok(())
}

/// `DataComponentPatch`: the added and removed counts, each added component
/// type and value, then the removed types.
fn patch(d: &mut Decoder, delimited: bool) -> Result<()> {
    let (added, removed) = d.group("components", |d| {
        let added = d.reader().count()?;
        let removed = d.reader().count()?;
        for _ in 0..added {
            typed_component(d, delimited)?;
        }
        for _ in 0..removed {
            removed_component(d)?;
        }
        Ok((added, removed))
    })?;
    d.show_last(match (added, removed) {
        (0, 0) => "none".into(),
        (added, removed) => format!("{added} added, {removed} removed"),
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::ops::RangeInclusive;

    use peel_trace::Direction;
    use peel_trace::Phase::Game;

    use super::super::super::Decoded;
    use super::super::super::testing::{
        assert_decodes, decode, entry, f32s, protocols, registry_id,
    };
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

    #[test]
    fn stacks_before_1_20_5() {
        for protocol in protocols(OLDEST..=V1_20_5 - 1) {
            let stone = entry(protocol, ClientRegistry::Item, 1);
            check(
                Clientbound,
                protocol..=protocol,
                "container_set_slot",
                &[0xff, 5, 0, 3, 1, 1, 2, 0],
                &[
                    "container_id: -1 [1..2]",
                    "state_id: 5 [2..3]",
                    "slot: 3 [3..5]",
                    &format!("item: 2 × {stone} [5..9]"),
                    &format!("  item: {stone} [6..7]"),
                    "  count: 2 [7..8]",
                    "  tag: none [8..9]",
                ],
            );
        }
        check(
            Clientbound,
            OLDEST..=V1_20_5 - 1,
            "container_set_slot",
            &[0, 5, 0, 3, 0],
            &[
                "container_id: 0 [1..2]",
                "state_id: 5 [2..3]",
                "slot: 3 [3..5]",
                "item: empty [5..6]",
            ],
        );
    }

    #[test]
    fn stacks_with_component_patches() {
        check(
            Clientbound,
            V1_20_5..=NEWEST,
            "container_set_slot",
            &[0, 5, 0, 3, 0],
            &[
                "container_id: 0 [1..2]",
                "state_id: 5 [2..3]",
                "slot: 3 [3..5]",
                "item: empty [5..6]",
            ],
        );
        for protocol in protocols(V1_20_5..=NEWEST) {
            let stone = entry(protocol, ClientRegistry::Item, 1);
            let id = |name| registry_id(protocol, ClientRegistry::DataComponentType, name);
            let body = [
                &[0, 5, 0, 3, 1, 1, 2, 1][..],
                &[id("max_damage"), 5],
                &[id("custom_name"), 8, 0, 3],
                b"Bob",
                &[id("damage")],
            ]
            .concat();
            check(
                Clientbound,
                protocol..=protocol,
                "container_set_slot",
                &body,
                &[
                    "container_id: 0 [1..2]",
                    "state_id: 5 [2..3]",
                    "slot: 3 [3..5]",
                    &format!("item: 1 × {stone} [5..19]"),
                    "  count: 1 [5..6]",
                    &format!("  item: {stone} [6..7]"),
                    "  components: 2 added, 1 removed [7..19]",
                    "    max_damage: 5 [9..11]",
                    "    custom_name: \"Bob\" [11..18]",
                    "    damage: removed [18..19]",
                ],
            );
        }
    }

    #[test]
    fn creative_slots_prefix_component_lengths_from_1_21_5() {
        let stone = entry(NEWEST, ClientRegistry::Item, 1);
        let lines = |end: usize| {
            vec![
                "slot: 3 [1..3]".to_string(),
                format!("item: 1 × {stone} [3..{end}]"),
                "  count: 1 [3..4]".into(),
                format!("  item: {stone} [4..5]"),
                format!("  components: 1 added, 0 removed [5..{end}]"),
                format!("    max_damage: 5 [7..{end}]"),
            ]
        };
        check(
            Serverbound,
            V1_20_5..=V1_21_5 - 1,
            "set_creative_mode_slot",
            &[0, 3, 1, 1, 1, 0, 2, 5],
            &lines(9),
        );
        check(
            Serverbound,
            V1_21_5..=NEWEST,
            "set_creative_mode_slot",
            &[0, 3, 1, 1, 1, 0, 2, 1, 5],
            &lines(10),
        );
    }

    #[test]
    fn clicks_send_hashed_stacks_from_1_21_5() {
        let stone = entry(NEWEST, ClientRegistry::Item, 1);
        let body = [
            &[0, 5, 0, 3, 0, 0, 1, 0, 3, 1, 1, 2, 1, 2][..],
            &0xabcdi32.to_be_bytes(),
            &[0, 0],
        ]
        .concat();
        check(
            Serverbound,
            V1_21_5..=NEWEST,
            "container_click",
            &body,
            &[
                "container_id: 0 [1..2]",
                "state_id: 5 [2..3]",
                "slot: 3 [3..5]",
                "button: 0 [5..6]",
                "click_type: pickup (0) [6..7]",
                "changed_slots: 1 item [7..20]",
                "  0:  [8..20]",
                "    slot: 3 [8..10]",
                &format!("    item: 2 × {stone} [10..20]"),
                &format!("      item: {stone} [11..12]"),
                "      count: 2 [12..13]",
                "      added: 1 item [13..19]",
                "        0:  [14..19]",
                "          type: max_damage (#2) [14..15]",
                "          hash: 0000abcd [15..19]",
                "      removed: 0 items [19..20]",
                "carried_item: empty [20..21]",
            ],
        );
    }

    #[test]
    fn container_ids_became_var_ints_in_1_21_2() {
        let ids: [(&[u8], &str); 2] = [(&[200], "[1..2]"), (&[0xc8, 1], "[1..3]")];
        for (range, (id, at)) in [OLDEST..=V1_21_2 - 1, V1_21_2..=NEWEST]
            .into_iter()
            .zip(ids)
        {
            let line = format!("container_id: 200 {at}");
            check(Clientbound, range.clone(), "container_close", id, &[&line]);
            let end = id.len() + 1;
            check(
                Clientbound,
                range,
                "container_set_data",
                &[id, &[0, 1, 0, 2]].concat(),
                &[
                    line,
                    format!("id: 1 [{end}..{}]", end + 2),
                    format!("value: 2 [{}..{}]", end + 2, end + 4),
                ],
            );
        }
        check(
            Serverbound,
            OLDEST..=V1_21_2 - 1,
            "container_close",
            &[0xff],
            &["container_id: -1 [1..2]"],
        );
        check(
            Serverbound,
            V1_21_2..=NEWEST,
            "container_close",
            &[0xc8, 1],
            &["container_id: 200 [1..3]"],
        );
    }

    #[test]
    fn container_contents() {
        check(
            Clientbound,
            OLDEST..=NEWEST,
            "container_set_content",
            &[0, 5, 1, 0, 0],
            &[
                "container_id: 0 [1..2]",
                "state_id: 5 [2..3]",
                "items: 1 item [3..5]",
                "  0: empty [4..5]",
                "carried_item: empty [5..6]",
            ],
        );
        check(
            Clientbound,
            V1_21_2..=NEWEST,
            "set_cursor_item",
            &[0],
            &["contents: empty [1..2]"],
        );
        check(
            Clientbound,
            V1_21_2..=NEWEST,
            "set_player_inventory",
            &[4, 0],
            &["slot: 4 [1..2]", "contents: empty [2..3]"],
        );
    }

    #[test]
    fn screens() {
        check(
            Clientbound,
            V1_20_3..=NEWEST,
            "open_screen",
            &[1, 7, 8, 0, 2, b'h', b'i'],
            &[
                "container_id: 1 [1..2]",
                "menu: #7 [2..3]",
                "title: \"hi\" [3..8]",
            ],
        );
        let body = [&[1, 3][..], &9i32.to_be_bytes()].concat();
        let lines = |middle: &str| {
            [
                "container_id: 1 [1..2]".to_string(),
                format!("{middle}: 3 [2..3]"),
                "entity_id: 9 [3..7]".into(),
            ]
        };
        check(
            Clientbound,
            OLDEST..=V1_21 - 1,
            "horse_screen_open",
            &body,
            &lines("size"),
        );
        check(
            Clientbound,
            V1_21..=V1_21_11 - 1,
            "horse_screen_open",
            &body,
            &lines("inventory_columns"),
        );
        check(
            Clientbound,
            V1_21_11..=NEWEST,
            "mount_screen_open",
            &body,
            &lines("inventory_columns"),
        );
        check(
            Serverbound,
            OLDEST..=NEWEST,
            "container_button_click",
            &[1, 2],
            &["container_id: 1 [1..2]", "button_id: 2 [2..3]"],
        );
        check(
            Serverbound,
            V1_20_3..=NEWEST,
            "container_slot_state_changed",
            &[3, 1, 1],
            &[
                "slot_id: 3 [1..2]",
                "container_id: 1 [2..3]",
                "new_state: true [3..4]",
            ],
        );
    }

    #[test]
    fn merchant_offers() {
        for protocol in protocols(OLDEST..=NEWEST) {
            let stone = entry(protocol, ClientRegistry::Item, 1);
            let (costs, cost_lines): (&[u8], _) = if protocol >= V1_20_5 {
                (
                    &[1, 2, 0, 1, 1, 0, 0, 0],
                    [
                        format!("    cost_a: 2 × {stone} [3..6]"),
                        format!("      item: {stone} [3..4]"),
                        "      count: 2 [4..5]".into(),
                        "      components: 0 items [5..6]".into(),
                        format!("    result: 1 × {stone} [6..10]"),
                        "      count: 1 [6..7]".into(),
                        format!("      item: {stone} [7..8]"),
                        "      components: none [8..10]".into(),
                        "    cost_b: absent [10..11]".into(),
                    ],
                )
            } else {
                (
                    &[1, 1, 2, 0, 1, 1, 1, 0, 0],
                    [
                        format!("    cost_a: 2 × {stone} [3..7]"),
                        format!("      item: {stone} [4..5]"),
                        "      count: 2 [5..6]".into(),
                        "      tag: none [6..7]".into(),
                        format!("    result: 1 × {stone} [7..11]"),
                        format!("      item: {stone} [8..9]"),
                        "      count: 1 [9..10]".into(),
                        "      tag: none [10..11]".into(),
                        "    cost_b: empty [11..12]".into(),
                    ],
                )
            };
            let body = [
                &[1, 1][..],
                costs,
                &[0],
                &1i32.to_be_bytes(),
                &12i32.to_be_bytes(),
                &2i32.to_be_bytes(),
                &0i32.to_be_bytes(),
                &f32s(&[0.05]),
                &0i32.to_be_bytes(),
                &[1, 0, 1, 1],
            ]
            .concat();
            let at = 3 + costs.len();
            let mut lines = vec![
                "container_id: 1 [1..2]".to_string(),
                format!("offers: 1 item [2..{}]", at + 25),
                format!("  0:  [3..{}]", at + 25),
            ];
            lines.extend(cost_lines);
            lines.extend([
                format!("    out_of_stock: false [{at}..{}]", at + 1),
                format!("    uses: 1 [{}..{}]", at + 1, at + 5),
                format!("    max_uses: 12 [{}..{}]", at + 5, at + 9),
                format!("    xp: 2 [{}..{}]", at + 9, at + 13),
                format!("    special_price_diff: 0 [{}..{}]", at + 13, at + 17),
                format!("    price_multiplier: 0.05 [{}..{}]", at + 17, at + 21),
                format!("    demand: 0 [{}..{}]", at + 21, at + 25),
                format!("villager_level: 1 [{}..{}]", at + 25, at + 26),
                format!("villager_xp: 0 [{}..{}]", at + 26, at + 27),
                format!("show_progress: true [{}..{}]", at + 27, at + 28),
                format!("can_restock: true [{}..{}]", at + 28, at + 29),
            ]);
            check(
                Clientbound,
                protocol..=protocol,
                "merchant_offers",
                &body,
                &lines,
            );
        }
    }

    /// A trace from a stranger can nest a stack in a component in a stack
    /// forever; decoding must stop rather than run out of stack.
    #[test]
    fn deeply_nested_components_stop_decoding() {
        let stone = registry_id(NEWEST, ClientRegistry::Item, "stone");
        let bundle = registry_id(NEWEST, ClientRegistry::DataComponentType, "bundle_contents");
        let mut body = vec![0, 0, 0, 0];
        for _ in 0..10_000 {
            // One stack: count, item, one added component, none removed, then
            // its one-item list, whose item is the next stack.
            body.extend([1, stone, 1, 0, bundle, 1]);
        }
        let decoded = decode(
            NEWEST,
            Game,
            Direction::Clientbound,
            "container_set_slot",
            &body,
        );
        assert!(
            matches!(&decoded, Decoded::Failed { reason, .. } if reason.contains("nest deeper")),
            "{decoded:?}"
        );
    }
}
