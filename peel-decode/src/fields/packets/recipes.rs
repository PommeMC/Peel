//! Recipe book packets. 1.21.2 replaced the recipe ids of `recipe` with
//! display entries and moved recipe data to the server.

use peel_trace::Direction::{Clientbound, Serverbound};
use pomme_protocol::ClientRegistry;

use super::super::reader::{Reader, Result};
use super::super::{Decoder, NEWEST, Name, OLDEST, PacketDecoder};
use super::items::{container_id, item_stack};
use super::{V1_20_3, V1_20_5, V1_21_2, game, holder_set, string, var_int};

pub static DECODERS: &[PacketDecoder] = &[
    game(Serverbound, "place_recipe", OLDEST..=NEWEST, |d| {
        container_id(d, true)?;
        recipe_id(d)?;
        d.bool("use_max_items")?;
        Ok(())
    }),
    game(Clientbound, "place_ghost_recipe", OLDEST..=NEWEST, |d| {
        container_id(d, true)?;
        if d.protocol() < V1_21_2 {
            return recipe_id(d);
        }
        let display = d.raw_id("display")?;
        Err(d.reader().unsupported(format!("recipe display #{display}")))
    }),
    game(
        Serverbound,
        "recipe_book_seen_recipe",
        OLDEST..=NEWEST,
        recipe_id,
    ),
    game(
        Serverbound,
        "recipe_book_change_settings",
        OLDEST..=NEWEST,
        |d| {
            d.variant("book_type", Reader::var_int, RECIPE_BOOKS)?;
            d.bool("is_open")?;
            d.bool("is_filtering")?;
            Ok(())
        },
    ),
    game(Clientbound, "recipe", OLDEST..=V1_21_2 - 1, |d| {
        let state = d.variant("state", Reader::var_int, &["init", "add", "remove"])?;
        book_settings(d)?;
        identifiers(d, "recipes")?;
        if state == 0 {
            identifiers(d, "to_highlight")?;
        }
        Ok(())
    }),
    game(
        Clientbound,
        "recipe_book_settings",
        V1_21_2..=NEWEST,
        book_settings,
    ),
    game(Clientbound, "recipe_book_add", V1_21_2..=NEWEST, |d| {
        d.list("entries", |d, name| {
            d.group(name, |d| {
                d.var_int("display_id")?;
                let display = d.raw_id("display")?;
                Err(d.reader().unsupported(format!("recipe display #{display}")))
            })
        })?;
        d.bool("replace")?;
        Ok(())
    }),
    game(Clientbound, "recipe_book_remove", V1_21_2..=NEWEST, |d| {
        d.list("display_ids", var_int)?;
        Ok(())
    }),
    game(Clientbound, "update_recipes", OLDEST..=V1_21_2 - 1, |d| {
        d.list("recipes", |d, name| {
            d.group(name, |d| {
                if d.protocol() >= V1_20_5 {
                    d.string("id")?;
                    let serializer = d.raw_id("serializer")?;
                    return Err(d
                        .reader()
                        .unsupported(format!("recipe serializer #{serializer}")));
                }
                let serializer = d.string("serializer")?.to_owned();
                d.string("id")?;
                recipe(
                    d,
                    serializer.strip_prefix("minecraft:").unwrap_or(&serializer),
                )
            })
        })?;
        Ok(())
    }),
    game(Clientbound, "update_recipes", V1_21_2..=NEWEST, |d| {
        d.list("property_sets", |d, name| {
            d.group(name, |d| {
                d.string("key")?;
                d.list("items", |d, name| {
                    d.registry(name, ClientRegistry::Item).map(drop)
                })?;
                Ok(())
            })
        })?;
        d.list("stonecutter_recipes", |d, name| {
            d.group(name, |d| {
                holder_set(d, "input", Some(ClientRegistry::Item))?;
                let display = d.raw_id("option_display")?;
                Err(d.reader().unsupported(format!("slot display #{display}")))
            })
        })?;
        Ok(())
    }),
];

fn identifiers(d: &mut Decoder, name: &'static str) -> Result<()> {
    d.list(name, string)?;
    Ok(())
}

/// A recipe as its serializer writes it, before 1.20.5 sent serializers by
/// a registry id `pomme-protocol` has no table for. 1.20.3 moved a shaped
/// recipe's size after its group and category.
fn recipe(d: &mut Decoder, serializer: &str) -> Result<()> {
    match serializer {
        "crafting_shaped" => {
            let (width, height) = if d.protocol() >= V1_20_3 {
                d.string("group")?;
                crafting_category(d)?;
                (d.var_int("width")?, d.var_int("height")?)
            } else {
                let size = (d.var_int("width")?, d.var_int("height")?);
                d.string("group")?;
                crafting_category(d)?;
                size
            };
            let cells = width
                .checked_mul(height)
                .ok_or_else(|| d.reader().error("the recipe is larger than any frame"))?;
            d.group("ingredients", |d| {
                for index in 0..cells {
                    ingredient(d, index.to_string().into())?;
                }
                Ok(())
            })?;
            item_stack(d, "result")?;
            d.bool("show_notification")?;
        }
        "crafting_shapeless" => {
            d.string("group")?;
            crafting_category(d)?;
            d.list("ingredients", ingredient)?;
            item_stack(d, "result")?;
        }
        "smelting" | "blasting" | "smoking" | "campfire_cooking" => {
            d.string("group")?;
            d.variant("category", Reader::var_int, &["food", "blocks", "misc"])?;
            ingredient(d, "ingredient".into())?;
            item_stack(d, "result")?;
            d.f32("experience")?;
            d.var_int("cooking_time")?;
        }
        "stonecutting" => {
            d.string("group")?;
            ingredient(d, "ingredient".into())?;
            item_stack(d, "result")?;
        }
        "smithing_transform" | "smithing_trim" => {
            for name in ["template", "base", "addition"] {
                ingredient(d, name.into())?;
            }
            if serializer == "smithing_transform" {
                item_stack(d, "result")?;
            }
        }
        special
            if special.starts_with("crafting_special_") || special == "crafting_decorated_pot" =>
        {
            crafting_category(d)?;
        }
        other => {
            return Err(d
                .reader()
                .error(format!("recipe serializer {other} doesn't exist")));
        }
    }
    Ok(())
}

fn crafting_category(d: &mut Decoder) -> Result<()> {
    d.variant(
        "category",
        Reader::var_int,
        &["building", "redstone", "equipment", "misc"],
    )?;
    Ok(())
}

/// An ingredient before 1.21.2: the item stacks that match it.
fn ingredient(d: &mut Decoder, name: Name) -> Result<()> {
    d.list(name, item_stack).map(drop)
}

/// `RecipeBookType`.
const RECIPE_BOOKS: &[&str] = &["crafting", "furnace", "blast_furnace", "smoker"];

/// A recipe the player picked: its id until 1.21.2 sent a display id.
fn recipe_id(d: &mut Decoder) -> Result<()> {
    if d.protocol() >= V1_21_2 {
        d.var_int("recipe")?;
    } else {
        d.string("recipe")?;
    }
    Ok(())
}

/// `RecipeBookSettings`: whether each recipe book is open and filtering.
fn book_settings(d: &mut Decoder) -> Result<()> {
    for book in RECIPE_BOOKS {
        d.bool(format!("{book}_open"))?;
        d.bool(format!("{book}_filtering"))?;
    }
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

    const SETTINGS: [u8; 8] = [1, 0, 0, 0, 0, 0, 0, 1];

    fn settings_lines(at: usize) -> Vec<String> {
        let values = [
            "true", "false", "false", "false", "false", "false", "false", "true",
        ];
        ["crafting", "furnace", "blast_furnace", "smoker"]
            .iter()
            .flat_map(|book| [format!("{book}_open"), format!("{book}_filtering")])
            .zip(values)
            .enumerate()
            .map(|(i, (name, value))| format!("{name}: {value} [{}..{}]", at + i, at + i + 1))
            .collect()
    }

    #[test]
    fn recipe_before_1_21_2() {
        let body = [&[0][..], &SETTINGS, &[1, 3], b"a:b", &[0]].concat();
        let mut expected = vec!["state: init (0) [1..2]".to_string()];
        expected.extend(settings_lines(2));
        expected.extend([
            "recipes: 1 item [10..15]".into(),
            "  0: \"a:b\" [11..15]".into(),
            "to_highlight: 0 items [15..16]".into(),
        ]);
        check(OLDEST..=V1_21_2 - 1, "recipe", &body, &expected);

        let body = [&[1][..], &SETTINGS, &[0]].concat();
        let mut expected = vec!["state: add (1) [1..2]".to_string()];
        expected.extend(settings_lines(2));
        expected.push("recipes: 0 items [10..11]".into());
        check(OLDEST..=V1_21_2 - 1, "recipe", &body, &expected);
    }

    #[test]
    fn recipe_book() {
        check(
            V1_21_2..=NEWEST,
            "recipe_book_settings",
            &SETTINGS,
            &settings_lines(1),
        );
        check(
            V1_21_2..=NEWEST,
            "recipe_book_remove",
            &[2, 3, 4],
            &[
                "display_ids: 2 items [1..4]",
                "  0: 3 [2..3]",
                "  1: 4 [3..4]",
            ],
        );
        assert_partial(
            V1_21_2..=NEWEST,
            Game,
            Clientbound,
            "recipe_book_add",
            &[1, 7, 2],
            &[
                "entries:  [1..4]",
                "  0:  [2..4]",
                "    display_id: 7 [2..3]",
                "    display: #2 [3..4]",
            ],
            (4, "recipe display #2 isn't decoded yet"),
        );
    }

    #[test]
    fn update_recipes_stops_at_slot_displays() {
        let air = "air (#0)";
        assert_partial(
            V1_21_2..=NEWEST,
            Game,
            Clientbound,
            "update_recipes",
            &[&[1, 3][..], b"a:b", &[1, 0], &[1, 2, 0, 4]].concat(),
            &[
                "property_sets: 1 item [1..8]",
                "  0:  [2..8]",
                "    key: \"a:b\" [2..6]",
                "    items: 1 item [6..8]",
                &format!("      0: {air} [7..8]"),
                "stonecutter_recipes:  [8..12]",
                "  0:  [9..12]",
                "    input: 1 item [9..11]",
                &format!("      0: {air} [10..11]"),
                "    option_display: #4 [11..12]",
            ],
            (12, "slot display #4 isn't decoded yet"),
        );
        assert_partial(
            V1_21_2..=NEWEST,
            Game,
            Clientbound,
            "update_recipes",
            &[&[0, 1, 0, 14][..], b"minecraft:logs", &[0]].concat(),
            &[
                "property_sets: 0 items [1..2]",
                "stonecutter_recipes:  [2..20]",
                "  0:  [3..20]",
                "    input: #minecraft:logs [3..19]",
                "    option_display: #0 [19..20]",
            ],
            (20, "slot display #0 isn't decoded yet"),
        );
    }

    #[test]
    fn update_recipes_before_1_21_2() {
        let shaped = |layout: [u8; 4]| {
            [
                &[1, 25][..],
                b"minecraft:crafting_shaped",
                &[3],
                b"a:b",
                &layout,
                &[0, 0, 1],
            ]
            .concat()
        };
        let lines = |first: [&'static str; 2], second: [&'static str; 2]| {
            [
                &[
                    "recipes: 1 item [1..39]",
                    "  0:  [2..39]",
                    "    serializer: \"minecraft:crafting_shaped\" [2..28]",
                    "    id: \"a:b\" [28..32]",
                ][..],
                &first,
                &second,
                &[
                    "    ingredients:  [36..37]",
                    "      0: 0 items [36..37]",
                    "    result: empty [37..38]",
                    "    show_notification: true [38..39]",
                ],
            ]
            .concat()
        };
        check(
            OLDEST..=V1_20_3 - 1,
            "update_recipes",
            &shaped([1, 1, 0, 3]),
            &lines(
                ["    width: 1 [32..33]", "    height: 1 [33..34]"],
                [
                    "    group: \"\" [34..35]",
                    "    category: misc (3) [35..36]",
                ],
            ),
        );
        check(
            V1_20_3..=V1_20_5 - 1,
            "update_recipes",
            &shaped([0, 3, 1, 1]),
            &lines(
                [
                    "    group: \"\" [32..33]",
                    "    category: misc (3) [33..34]",
                ],
                ["    width: 1 [34..35]", "    height: 1 [35..36]"],
            ),
        );
        assert_partial(
            V1_20_5..=V1_21_2 - 1,
            Game,
            Clientbound,
            "update_recipes",
            &[&[1, 3][..], b"a:b", &[0]].concat(),
            &[
                "recipes:  [1..7]",
                "  0:  [2..7]",
                "    id: \"a:b\" [2..6]",
                "    serializer: #0 [6..7]",
            ],
            (7, "recipe serializer #0 isn't decoded yet"),
        );
    }

    #[test]
    fn placing_recipes() {
        assert_decodes(
            OLDEST..=V1_21_2 - 1,
            Game,
            Serverbound,
            "place_recipe",
            &[&[0xff, 3][..], b"a:b", &[1]].concat(),
            &[
                "container_id: -1 [1..2]",
                "recipe: \"a:b\" [2..6]",
                "use_max_items: true [6..7]",
            ],
        );
        assert_decodes(
            V1_21_2..=NEWEST,
            Game,
            Serverbound,
            "place_recipe",
            &[2, 9, 0],
            &[
                "container_id: 2 [1..2]",
                "recipe: 9 [2..3]",
                "use_max_items: false [3..4]",
            ],
        );
        check(
            OLDEST..=V1_21_2 - 1,
            "place_ghost_recipe",
            &[&[1, 3][..], b"a:b"].concat(),
            &["container_id: 1 [1..2]", "recipe: \"a:b\" [2..6]"],
        );
        assert_partial(
            V1_21_2..=NEWEST,
            Game,
            Clientbound,
            "place_ghost_recipe",
            &[1, 2],
            &["container_id: 1 [1..2]", "display: #2 [2..3]"],
            (3, "recipe display #2 isn't decoded yet"),
        );
        assert_decodes(
            V1_21_2..=NEWEST,
            Game,
            Serverbound,
            "recipe_book_seen_recipe",
            &[9],
            &["recipe: 9 [1..2]"],
        );
        assert_decodes(
            OLDEST..=NEWEST,
            Game,
            Serverbound,
            "recipe_book_change_settings",
            &[2, 1, 0],
            &[
                "book_type: blast_furnace (2) [1..2]",
                "is_open: true [2..3]",
                "is_filtering: false [3..4]",
            ],
        );
    }
}
