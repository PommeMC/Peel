//! Data component values, keyed by the component type's registry name.
//! Values carry no length, so decoding stops at a value Peel can't read.

use pomme_protocol::ClientRegistry;

use super::super::reader::{Reader, Result};
use super::super::{Decoder, Name};
use super::items::{item_stack, item_template, stack_or_template};
use super::session::profile_properties;
use super::text::component;
use super::{
    V1_21, V1_21_2, V1_21_4, V1_21_5, V1_21_6, V1_21_9, V1_21_11, V26_1, V26_3, global_pos, holder,
    holder_set, nbt_tag, registry_name, sound_event, string, tag_key, variant,
};

/// `TypedDataComponent`: a component type id, then its value, recorded under
/// the type's name. The creative-mode slot sends the value's length between
/// them from 1.21.5.
pub fn typed_component(d: &mut Decoder, delimited: bool) -> Result<()> {
    d.prefixed(Reader::var_int, |d, id| {
        let kind = registry_name(
            d,
            ClientRegistry::DataComponentType,
            id,
            "data component type",
        )?;
        if !delimited {
            return value(d, kind, kind.into());
        }
        let len = d.reader().count()?;
        let end = d.reader().pos() + len;
        value(d, kind, kind.into())?;
        if d.reader().pos() != end {
            return Err(d.reader().error(format!("{kind} should take {len} bytes")));
        }
        Ok(())
    })
}

/// A component type id in a patch's removed list.
pub fn removed_component(d: &mut Decoder) -> Result<()> {
    d.prefixed(Reader::var_int, |d, id| {
        let kind = registry_name(
            d,
            ClientRegistry::DataComponentType,
            id,
            "data component type",
        )?;
        d.note(kind, "removed")
    })
}

fn value(d: &mut Decoder, kind: &str, name: Name) -> Result<()> {
    let protocol = d.protocol();
    match kind {
        "custom_data"
        | "intangible_projectile"
        | "map_decorations"
        | "debug_stick_state"
        | "recipes"
        | "lock"
        | "container_loot"
        | "bucket_entity_data" => nbt_tag(d, name),
        "max_stack_size"
        | "max_damage"
        | "damage"
        | "repair_cost"
        | "map_id"
        | "ominous_bottle_amplifier"
        | "enchantable"
        | "villager_food"
        | "additional_trade_cost" => d.var_int(name).map(drop),
        "unbreakable" if protocol < V1_21_5 => {
            d.group(name, |d| d.bool("show_in_tooltip").map(drop))
        }
        "unbreakable"
        | "hide_additional_tooltip"
        | "hide_tooltip"
        | "fire_resistant"
        | "creative_slot_lock"
        | "glider"
        | "waxed" => d.note(name, "set"),
        "custom_name" | "item_name" => component(d, name),
        "lore" => d.list(name, component).map(drop),
        "rarity" => variant(d, name, &["common", "uncommon", "rare", "epic"]),
        "enchantments" | "stored_enchantments" => with_tooltip_flag(d, name, |d, name| {
            d.list(name, |d, name| {
                d.read(
                    name,
                    |r| Ok((r.var_int()?, r.var_int()?)),
                    |(id, level)| format!("#{id} level {level}"),
                )
                .map(drop)
            })
            .map(drop)
        }),
        "can_place_on" | "can_break" => with_tooltip_flag(d, name, |d, name| {
            d.list(name, |d, name| d.group(name, block_predicate))
                .map(drop)
        }),
        "attribute_modifiers" => with_tooltip_flag(d, name, |d, name| {
            d.list(name, |d, name| d.group(name, attribute_modifier))
                .map(drop)
        }),
        "custom_model_data" if protocol < V1_21_4 => d.var_int(name).map(drop),
        "custom_model_data" => d.group(name, |d| {
            d.list("floats", |d, name| d.f32(name).map(drop))?;
            d.list("flags", |d, name| d.bool(name).map(drop))?;
            d.list("strings", string)?;
            d.list("colors", rgb)?;
            Ok(())
        }),
        "tooltip_display" => d.group(name, |d| {
            d.bool("hide_tooltip")?;
            d.list("hidden_components", |d, name| {
                d.registry(name, ClientRegistry::DataComponentType)
                    .map(drop)
            })?;
            Ok(())
        }),
        "enchantment_glint_override" => d.bool(name).map(drop),
        "food" => d.group(name, food),
        "consumable" => d.group(name, |d| {
            d.f32("consume_seconds")?;
            use_animation(d)?;
            sound_event(d, "sound")?;
            d.bool("has_consume_particles")?;
            consume_effects(d, "on_consume_effects")
        }),
        "use_remainder" | "sulfur_cube_content" => stack_or_template(d, name),
        "use_cooldown" => d.group(name, |d| {
            d.f32("seconds")?;
            d.optional("cooldown_group", string)?;
            Ok(())
        }),
        "damage_resistant" if protocol < V26_1 => tag_key(d, name),
        "damage_resistant" => holder_set(d, name, None),
        "tool" => d.group(name, tool),
        "weapon" => d.group(name, |d| {
            d.var_int("item_damage_per_attack")?;
            d.f32("disable_blocking_for_seconds")?;
            Ok(())
        }),
        "attack_range" => floats(
            d,
            name,
            &[
                "min_reach",
                "max_reach",
                "min_creative_reach",
                "max_creative_reach",
                "hitbox_margin",
                "mob_factor",
            ],
        ),
        "use_effects" => d.group(name, |d| {
            d.bool("can_sprint")?;
            d.bool("interact_vibrations")?;
            d.f32("speed_multiplier")?;
            Ok(())
        }),
        "minimum_attack_charge" | "potion_duration_scale" => d.f32(name).map(drop),
        "equippable" => d.group(name, equippable),
        "repairable" => holder_set(d, name, Some(ClientRegistry::Item)),
        "item_model" | "tooltip_style" | "note_block_sound" => string(d, name),
        "death_protection" => d.group(name, |d| consume_effects(d, "death_effects")),
        "blocks_attacks" => d.group(name, blocks_attacks),
        "piercing_weapon" => d.group(name, |d| {
            d.bool("deals_knockback")?;
            d.bool("dismounts")?;
            optional_sounds(d)
        }),
        "kinetic_weapon" => d.group(name, kinetic_weapon),
        "swing_animation" | "attack_animation" | "interact_animation" => d.group(name, |d| {
            d.variant("type", Reader::var_int, &["none", "whack", "stab"])?;
            d.var_int("duration")?;
            Ok(())
        }),
        "chicken/variant" | "zombie_nautilus/variant" | "damage_type" if protocol < V26_1 => {
            either_holder(d, name, |d, name| d.raw_id(name).map(drop))
        }
        "villager/variant"
        | "wolf/variant"
        | "wolf/sound_variant"
        | "pig/variant"
        | "pig/sound_variant"
        | "cow/variant"
        | "cow/sound_variant"
        | "chicken/variant"
        | "chicken/sound_variant"
        | "zombie_nautilus/variant"
        | "frog/variant"
        | "cat/variant"
        | "cat/sound_variant"
        | "damage_type"
        | "block_transformer"
        | "provides_pottery_pattern" => d.raw_id(name).map(drop),
        "dye"
        | "base_color"
        | "wolf/collar"
        | "tropical_fish/base_color"
        | "tropical_fish/pattern_color"
        | "cat/collar"
        | "sheep/color"
        | "shulker/color"
        | "cushion/color" => dye_color(d, name),
        "dyed_color" => with_tooltip_flag(d, name, rgb),
        "map_color" => rgb(d, name),
        "map_post_processing" => variant(d, name, &["lock", "scale"]),
        "charged_projectiles" | "bundle_contents" => d.list(name, stack_or_template).map(drop),
        "potion_contents" => d.group(name, |d| {
            d.optional("potion", |d, name| d.raw_id(name))?;
            d.optional("custom_color", rgb)?;
            d.list("custom_effects", mob_effect_instance)?;
            if d.protocol() >= V1_21_2 {
                d.optional("custom_name", string)?;
            }
            Ok(())
        }),
        "suspicious_stew_effects" => d
            .list(name, |d, name| {
                d.read(
                    name,
                    |r| Ok((r.var_int()?, r.var_int()?)),
                    |(id, duration)| format!("#{id} for {duration} ticks"),
                )
                .map(drop)
            })
            .map(drop),
        "writable_book_content" => d
            .list(name, |d, name| filterable(d, name, string))
            .map(drop),
        "written_book_content" => d.group(name, |d| {
            filterable(d, "title".into(), string)?;
            d.string("author")?;
            d.var_int("generation")?;
            d.list("pages", |d, name| filterable(d, name, component))?;
            d.bool("resolved")?;
            Ok(())
        }),
        "trim" => with_tooltip_flag(d, name, |d, name| {
            d.group(name, |d| {
                trim_material(d, "material".into())?;
                trim_pattern(d)
            })
        }),
        "entity_data" | "block_entity_data" if protocol < V1_21_9 => nbt_tag(d, name),
        "entity_data" => typed_entity_data(d, name, ClientRegistry::EntityType),
        "block_entity_data" => typed_entity_data(d, name, ClientRegistry::BlockEntityType),
        "instrument" => instrument(d, name),
        "provides_trim_material" if protocol < V26_1 => either_holder(d, name, trim_material),
        "provides_trim_material" => trim_material(d, name),
        "jukebox_playable" => with_tooltip_flag(d, name, |d, name| {
            if d.protocol() < V26_1 {
                either_holder(d, name, jukebox_song)
            } else {
                jukebox_song(d, name)
            }
        }),
        "provides_banner_patterns" if protocol < V26_1 => tag_key(d, name),
        "provides_banner_patterns" => holder_set(d, name, None),
        "lodestone_tracker" => d.group(name, |d| {
            d.optional("target", global_pos)?;
            d.bool("tracked")?;
            Ok(())
        }),
        "firework_explosion" => d.group(name, firework_explosion),
        "fireworks" => d.group(name, |d| {
            d.var_int("flight_duration")?;
            d.list("explosions", |d, name| d.group(name, firework_explosion))?;
            Ok(())
        }),
        "profile" => d.group(name, profile),
        "banner_patterns" => d
            .list(name, |d, name| {
                d.group(name, |d| {
                    holder(d, "pattern", None, |d| {
                        d.string("asset_id")?;
                        d.string("translation_key")?;
                        Ok(())
                    })?;
                    dye_color(d, "color".into())
                })
            })
            .map(drop),
        "pot_decorations" if protocol < V26_3 => d
            .list(name, |d, name| {
                d.registry(name, ClientRegistry::Item).map(drop)
            })
            .map(drop),
        "pot_decorations" => d.group(name, |d| {
            for side in ["back", "left", "right", "front"] {
                d.optional(side, item_template)?;
            }
            Ok(())
        }),
        "container" if protocol < V26_1 => d.list(name, item_stack).map(drop),
        "container" => d
            .list(name, |d, name| d.optional(name, item_template).map(drop))
            .map(drop),
        "block_state" => d.list(name, |d, name| string_entry(d, name)).map(drop),
        "bees" => d
            .list(name, |d, name| {
                d.group(name, |d| {
                    if d.protocol() < V1_21_9 {
                        nbt_tag(d, "entity_data".into())?;
                    } else {
                        typed_entity_data(d, "entity_data".into(), ClientRegistry::EntityType)?;
                    }
                    d.var_int("ticks_in_hive")?;
                    d.var_int("min_ticks_in_hive")?;
                    Ok(())
                })
            })
            .map(drop),
        "break_sound" => sound_event(d, name),
        "compostable" => resolvable_int(d, name),
        "cooking_fuel" => d.group(name, |d| {
            resolvable_int(d, "burn_time".into())?;
            resolvable_float(d, "speed_multiplier".into())
        }),
        "brewing_fuel" => d.group(name, |d| {
            resolvable_int(d, "uses".into())?;
            resolvable_float(d, "speed_multiplier".into())
        }),
        "mob_visibility" => d.group(name, |d| {
            holder_set(
                d,
                "targeting_entity_types",
                Some(ClientRegistry::EntityType),
            )?;
            d.f32("visibility")?;
            Ok(())
        }),
        "fox/variant" => variant(d, name, &["red", "snow"]),
        "salmon/size" => variant(d, name, &["small", "medium", "large"]),
        "parrot/variant" => variant(
            d,
            name,
            &["red_blue", "blue", "green", "yellow_blue", "gray"],
        ),
        "mooshroom/variant" => variant(d, name, &["red", "brown"]),
        "horse/variant" => variant(
            d,
            name,
            &[
                "white",
                "creamy",
                "chestnut",
                "brown",
                "black",
                "gray",
                "dark_brown",
            ],
        ),
        "llama/variant" => variant(d, name, &["creamy", "white", "brown", "gray"]),
        "axolotl/variant" => variant(d, name, &["lucy", "wild", "gold", "cyan", "blue"]),
        "rabbit/variant" => d
            .read(name, Reader::var_int, |&id| {
                let names = ["brown", "white", "black", "white_splotched", "gold", "salt"];
                let label = match id {
                    99 => "evil",
                    id => usize::try_from(id)
                        .ok()
                        .and_then(|i| names.get(i))
                        .unwrap_or(&"unknown"),
                };
                format!("{label} ({id})")
            })
            .map(drop),
        "tropical_fish/pattern" => d
            .read(name, Reader::var_int, |&packed| {
                let patterns: [[&str; 6]; 2] = [
                    ["kob", "sunstreak", "snooper", "dasher", "brinely", "spotty"],
                    [
                        "flopper",
                        "stripey",
                        "glitter",
                        "blockfish",
                        "betty",
                        "clayfish",
                    ],
                ];
                let label = usize::try_from(packed & 0xFF)
                    .ok()
                    .and_then(|size| patterns.get(size)?.get(usize::try_from(packed >> 8).ok()?))
                    .unwrap_or(&"unknown");
                format!("{label} ({packed})")
            })
            .map(drop),
        "painting/variant" => painting_variant(d, name),
        "sign_text_front" | "sign_text_back" => d.group(name, |d| {
            four_components(d, "messages".into())?;
            d.optional("filtered_messages", four_components)?;
            dye_color(d, "color".into())?;
            d.bool("has_glowing_text")?;
            Ok(())
        }),
        _ => Err(d.reader().unsupported(format!("data component {kind}"))),
    }
}

fn floats(d: &mut Decoder, name: Name, names: &[&'static str]) -> Result<()> {
    d.group(name, |d| {
        for &name in names {
            d.f32(name)?;
        }
        Ok(())
    })
}

/// A color as an int, shown in hex.
pub fn rgb(d: &mut Decoder, name: Name) -> Result<()> {
    d.read(name, Reader::i32, |color| format!("{color:06x}"))
        .map(drop)
}

fn string_entry(d: &mut Decoder, name: Name) -> Result<()> {
    d.read(
        name,
        |r| Ok((r.string()?, r.string()?)),
        |(key, value)| format!("{key}: {value}"),
    )
    .map(drop)
}

/// `DyeColor`.
pub fn dye_color(d: &mut Decoder, name: Name) -> Result<()> {
    variant(
        d,
        name,
        &[
            "white",
            "orange",
            "magenta",
            "light_blue",
            "yellow",
            "lime",
            "pink",
            "gray",
            "light_gray",
            "cyan",
            "purple",
            "blue",
            "brown",
            "green",
            "red",
            "black",
        ],
    )
}

/// Before 1.21.5 moved tooltips to `tooltip_display`, a value followed by
/// whether its tooltip shows.
fn with_tooltip_flag(
    d: &mut Decoder,
    name: Name,
    value: impl FnOnce(&mut Decoder, Name) -> Result<()>,
) -> Result<()> {
    if d.protocol() >= V1_21_5 {
        return value(d, name);
    }
    d.group(name, |d| {
        value(d, "value".into())?;
        d.bool("show_in_tooltip")?;
        Ok(())
    })
}

/// `EitherHolder`, from 1.21.5 until 26.1: true and a holder, or false and a
/// resource key.
fn either_holder(
    d: &mut Decoder,
    name: Name,
    holder: impl FnOnce(&mut Decoder, Name) -> Result<()>,
) -> Result<()> {
    d.prefixed(Reader::bool, |d, is_holder| {
        if is_holder {
            holder(d, name)
        } else {
            string(d, name)
        }
    })
}

/// `Filterable`: the raw value and an optional filtered one.
fn filterable(
    d: &mut Decoder,
    name: Name,
    value: fn(&mut Decoder, Name) -> Result<()>,
) -> Result<()> {
    d.group(name, |d| {
        value(d, "raw".into())?;
        d.optional("filtered", value)?;
        Ok(())
    })
}

fn four_components(d: &mut Decoder, name: Name) -> Result<()> {
    d.group(name, |d| {
        for index in 0..4 {
            component(d, index.to_string())?;
        }
        Ok(())
    })
}

/// An int or a float, or the key of a provider that computes it.
fn resolvable_int(d: &mut Decoder, name: Name) -> Result<()> {
    d.prefixed(Reader::bool, |d, constant| {
        if constant {
            d.i32(name).map(drop)
        } else {
            string(d, name)
        }
    })
}

fn resolvable_float(d: &mut Decoder, name: Name) -> Result<()> {
    d.prefixed(Reader::bool, |d, constant| {
        if constant {
            d.f32(name).map(drop)
        } else {
            string(d, name)
        }
    })
}

/// `TypedEntityData`, from 1.21.9: the type, then its NBT.
fn typed_entity_data(d: &mut Decoder, name: Name, registry: ClientRegistry) -> Result<()> {
    d.group(name, |d| {
        d.registry("type", registry)?;
        nbt_tag(d, "data".into())
    })
}

/// `MobEffectInstance`: the effect, then its details.
fn mob_effect_instance(d: &mut Decoder, name: Name) -> Result<()> {
    d.group(name, |d| {
        d.raw_id("effect")?;
        effect_details(d)
    })
}

/// `MobEffectInstance.Details`, which can hide a weaker effect of the same
/// type.
fn effect_details(d: &mut Decoder) -> Result<()> {
    d.var_int("amplifier")?;
    d.var_int("duration")?;
    d.bool("ambient")?;
    d.bool("show_particles")?;
    d.bool("show_icon")?;
    d.optional("hidden_effect", |d, name| d.group(name, effect_details))?;
    Ok(())
}

/// `ConsumeEffect`s dispatch on a registry `pomme-protocol` has no table
/// for, so a non-empty list stops decoding.
fn consume_effects(d: &mut Decoder, name: &'static str) -> Result<()> {
    d.list(name, |d, name| {
        let kind = d.raw_id(name)?;
        Err(d.reader().unsupported(format!("consume effect #{kind}")))
    })?;
    Ok(())
}

/// `FoodProperties`. Until 1.21.2 it also held the eating time, the item
/// left behind (from 1.21) and the effects.
fn food(d: &mut Decoder) -> Result<()> {
    d.var_int("nutrition")?;
    d.f32("saturation")?;
    d.bool("can_always_eat")?;
    if d.protocol() < V1_21_2 {
        d.f32("eat_seconds")?;
        if d.protocol() >= V1_21 {
            d.optional("using_converts_to", item_stack)?;
        }
        d.list("effects", |d, name| {
            d.group(name, |d| {
                mob_effect_instance(d, "effect".into())?;
                d.f32("probability")?;
                Ok(())
            })
        })?;
    }
    Ok(())
}

/// `ItemUseAnimation`: 1.21.4 added `bundle`, and 1.21.11 renamed `spear`
/// to `trident` for a new `spear`.
fn use_animation(d: &mut Decoder) -> Result<()> {
    let spear = if d.protocol() >= V1_21_11 {
        "trident"
    } else {
        "spear"
    };
    let names = [
        "none",
        "eat",
        "drink",
        "block",
        "bow",
        spear,
        "crossbow",
        "spyglass",
        "toot_horn",
        "brush",
        "bundle",
        "spear",
    ];
    let known = match d.protocol() {
        V1_21_11.. => 12,
        V1_21_4.. => 11,
        _ => 10,
    };
    d.variant("animation", Reader::var_int, &names[..known])?;
    Ok(())
}

fn tool(d: &mut Decoder) -> Result<()> {
    d.list("rules", |d, name| {
        d.group(name, |d| {
            holder_set(d, "blocks", None)?;
            d.optional("speed", |d, name| d.f32(name))?;
            d.optional("correct_for_drops", |d, name| d.bool(name))?;
            Ok(())
        })
    })?;
    d.f32("default_mining_speed")?;
    d.var_int("damage_per_block")?;
    if d.protocol() >= V1_21_5 {
        d.bool("can_destroy_blocks_in_creative")?;
    }
    Ok(())
}

/// `EquipmentSlot` by id, which isn't its ordinal; 1.21.5 added `saddle`.
fn equipment_slot(d: &mut Decoder) -> Result<()> {
    let names = [
        "mainhand", "feet", "legs", "chest", "head", "offhand", "body", "saddle",
    ];
    let known = if d.protocol() >= V1_21_5 { 8 } else { 7 };
    d.variant("slot", Reader::var_int, &names[..known])?;
    Ok(())
}

fn equippable(d: &mut Decoder) -> Result<()> {
    let protocol = d.protocol();
    equipment_slot(d)?;
    sound_event(d, "equip_sound")?;
    let asset = if protocol >= V1_21_4 {
        "asset_id"
    } else {
        "model"
    };
    d.optional(asset, string)?;
    d.optional("camera_overlay", string)?;
    d.optional("allowed_entities", |d, name| {
        holder_set(d, name, Some(ClientRegistry::EntityType))
    })?;
    for name in ["dispensable", "swappable", "damage_on_hurt"] {
        d.bool(name)?;
    }
    if protocol >= V1_21_5 {
        d.bool("equip_on_interact")?;
    }
    if protocol >= V1_21_6 {
        d.bool("can_be_sheared")?;
        sound_event(d, "shearing_sound")?;
    }
    Ok(())
}

/// `AttributeModifier.Operation`, from 1.20.5.
pub const ATTRIBUTE_OPERATIONS: &[&str] =
    &["add_value", "add_multiplied_base", "add_multiplied_total"];

/// `ItemAttributeModifiers.Entry`. 1.21 replaced the modifier's UUID and
/// name with an id, and 1.21.6 added how the tooltip shows it.
fn attribute_modifier(d: &mut Decoder) -> Result<()> {
    let protocol = d.protocol();
    d.registry("attribute", ClientRegistry::Attribute)?;
    if protocol < V1_21 {
        d.uuid("uuid")?;
        d.string("name")?;
    } else {
        d.string("id")?;
    }
    d.f64("amount")?;
    d.variant("operation", Reader::var_int, ATTRIBUTE_OPERATIONS)?;
    let groups = [
        "any", "mainhand", "offhand", "hand", "feet", "legs", "chest", "head", "armor", "body",
        "saddle",
    ];
    let known = if protocol >= V1_21_5 { 11 } else { 10 };
    d.variant("slot", Reader::var_int, &groups[..known])?;
    if protocol >= V1_21_6 {
        let display = d.variant(
            "display",
            Reader::var_int,
            &["default", "hidden", "override"],
        )?;
        if display == 2 {
            component(d, "text")?;
        }
    }
    Ok(())
}

/// `BlockPredicate`. 1.21.5 added component matchers, whose partial
/// predicates could name a component type directly from 1.21.11.
fn block_predicate(d: &mut Decoder) -> Result<()> {
    d.optional("blocks", |d, name| holder_set(d, name, None))?;
    d.optional("properties", |d, name| {
        d.list(name, |d, name| {
            d.group(name, |d| {
                d.string("name")?;
                d.prefixed(Reader::bool, |d, exact| {
                    if exact {
                        return string(d, "value".into());
                    }
                    d.group("range", |d| {
                        d.optional("min", string)?;
                        d.optional("max", string)?;
                        Ok(())
                    })
                })
            })
        })
    })?;
    d.optional("nbt", nbt_tag)?;
    if d.protocol() >= V1_21_5 {
        d.list("components", |d, _| typed_component(d, false))?;
        d.list("predicates", |d, name| {
            d.group(name, |d| {
                if d.protocol() >= V1_21_11 {
                    d.prefixed(Reader::bool, |d, is_predicate| {
                        if is_predicate {
                            d.raw_id("type").map(drop)
                        } else {
                            d.registry("type", ClientRegistry::DataComponentType)
                                .map(drop)
                        }
                    })?;
                } else {
                    d.raw_id("type")?;
                }
                nbt_tag(d, "predicate".into())
            })
        })?;
    }
    Ok(())
}

fn optional_sounds(d: &mut Decoder) -> Result<()> {
    d.optional("sound", sound_event)?;
    d.optional("hit_sound", sound_event)?;
    Ok(())
}

fn kinetic_weapon(d: &mut Decoder) -> Result<()> {
    d.var_int("contact_cooldown_ticks")?;
    d.var_int("delay_ticks")?;
    for name in [
        "dismount_conditions",
        "knockback_conditions",
        "damage_conditions",
    ] {
        d.optional(name, |d, name| {
            d.group(name, |d| {
                d.var_int("max_duration_ticks")?;
                d.f32("min_speed")?;
                d.f32("min_relative_speed")?;
                Ok(())
            })
        })?;
    }
    d.f32("forward_movement")?;
    d.f32("damage_multiplier")?;
    optional_sounds(d)
}

/// `BlocksAttacks`; 26.1 made the bypassing damage types a holder set.
fn blocks_attacks(d: &mut Decoder) -> Result<()> {
    d.f32("block_delay_seconds")?;
    d.f32("disable_cooldown_scale")?;
    d.list("damage_reductions", |d, name| {
        d.group(name, |d| {
            d.f32("horizontal_blocking_angle")?;
            d.optional("type", |d, name| holder_set(d, name, None))?;
            d.f32("base")?;
            d.f32("factor")?;
            Ok(())
        })
    })?;
    floats(d, "item_damage".into(), &["threshold", "base", "factor"])?;
    d.optional("bypassed_by", |d, name| {
        if d.protocol() >= V26_1 {
            holder_set(d, name, None)
        } else {
            tag_key(d, name)
        }
    })?;
    d.optional("block_sound", sound_event)?;
    d.optional("disable_sound", sound_event)?;
    Ok(())
}

/// A `TrimMaterial` holder. Its inline form has changed with how trims
/// pick their textures.
fn trim_material(d: &mut Decoder, name: Name) -> Result<()> {
    holder(d, name, None, |d| {
        let protocol = d.protocol();
        if protocol >= V26_3 {
            d.string("palette_id")?;
        } else if protocol >= V1_21_5 {
            d.group("assets", |d| {
                d.string("base")?;
                d.list("overrides", string_entry)?;
                Ok(())
            })?;
        } else {
            d.string("asset_name")?;
            d.registry("ingredient", ClientRegistry::Item)?;
            if protocol < V1_21_4 {
                d.f32("item_model_index")?;
            }
            if protocol < V1_21_2 {
                d.list("override_armor_materials", |d, name| {
                    d.read(
                        name,
                        |r| Ok((r.var_int()?, r.string()?)),
                        |(material, asset)| format!("#{material}: {asset}"),
                    )
                    .map(drop)
                })?;
            } else {
                d.list("override_armor_assets", string_entry)?;
            }
        }
        component(d, "description")
    })
}

/// A `TrimPattern` holder; 1.21.5 dropped the template item.
fn trim_pattern(d: &mut Decoder) -> Result<()> {
    holder(d, "pattern", None, |d| {
        d.string("asset_id")?;
        if d.protocol() < V1_21_5 {
            d.registry("template_item", ClientRegistry::Item)?;
        }
        component(d, "description")?;
        d.bool("decal")?;
        Ok(())
    })
}

/// An `Instrument` holder, wrapped in `EitherHolder` from 1.21.5 until 26.1.
fn instrument(d: &mut Decoder, name: Name) -> Result<()> {
    let inline = |d: &mut Decoder, name| {
        holder(d, name, None, |d| {
            let protocol = d.protocol();
            sound_event(d, "sound")?;
            if protocol >= V1_21_2 {
                d.f32("use_duration")?;
            } else {
                d.var_int("use_duration")?;
            }
            d.f32("range")?;
            if protocol >= V26_3 {
                d.var_int("durability_damage")?;
            }
            if protocol >= V1_21_2 {
                component(d, "description")?;
            }
            Ok(())
        })
    };
    if (V1_21_5..V26_1).contains(&d.protocol()) {
        either_holder(d, name, inline)
    } else {
        inline(d, name)
    }
}

/// A `JukeboxSong` holder.
fn jukebox_song(d: &mut Decoder, name: Name) -> Result<()> {
    holder(d, name, None, |d| {
        sound_event(d, "sound")?;
        component(d, "description")?;
        d.f32("length_in_seconds")?;
        d.var_int("comparator_output")?;
        Ok(())
    })
}

fn firework_explosion(d: &mut Decoder) -> Result<()> {
    d.variant(
        "shape",
        Reader::var_int,
        &["small_ball", "large_ball", "star", "creeper", "burst"],
    )?;
    d.list("colors", rgb)?;
    d.list("fade_colors", rgb)?;
    d.bool("has_trail")?;
    d.bool("has_twinkle")?;
    Ok(())
}

/// A `PaintingVariant` holder; 1.21.2 added the title and author.
pub fn painting_variant(d: &mut Decoder, name: Name) -> Result<()> {
    holder(d, name, None, |d| {
        d.var_int("width")?;
        d.var_int("height")?;
        d.string("asset_id")?;
        if d.protocol() >= V1_21_2 {
            d.optional("title", component)?;
            d.optional("author", component)?;
        }
        Ok(())
    })
}

/// `ResolvableProfile`. From 1.21.9 it is a full or partial profile, then
/// skin overrides.
pub fn profile(d: &mut Decoder) -> Result<()> {
    if d.protocol() < V1_21_9 {
        return partial_profile(d);
    }
    d.prefixed(Reader::bool, |d, full| {
        if full {
            d.group("game_profile", |d| {
                d.uuid("id")?;
                d.string("name")?;
                profile_properties(d)
            })
        } else {
            d.group("partial_profile", partial_profile)
        }
    })?;
    d.group("skin", |d| {
        for name in ["body", "cape", "elytra"] {
            d.optional(name, string)?;
        }
        d.optional("model", |d, name| {
            d.read(name, Reader::bool, |&slim| {
                if slim { "slim" } else { "wide" }.into()
            })
        })?;
        Ok(())
    })
}

fn partial_profile(d: &mut Decoder) -> Result<()> {
    d.optional("name", string)?;
    d.optional("id", |d, name| d.uuid(name))?;
    profile_properties(d)
}

#[cfg(test)]
mod tests {
    use std::ops::RangeInclusive;

    use peel_trace::Direction::Clientbound;
    use peel_trace::Phase::Game;
    use pomme_protocol::RegistryTable;

    use super::super::super::NEWEST;
    use super::super::super::testing::{
        assert_decodes, assert_partial, entry, f32s, f64s, protocols, registry_id,
    };
    use super::super::V1_20_5;
    use super::*;

    /// A `container_set_slot` whose stack has one component, and the lines
    /// before the component's own, which start at byte 9.
    fn stack_with(protocol: i32, name: &str, value: &[u8]) -> (Vec<u8>, Vec<String>) {
        let end = 10 + value.len();
        let stone = entry(protocol, ClientRegistry::Item, 1);
        let id = registry_id(protocol, ClientRegistry::DataComponentType, name);
        let body = [&[0, 0, 0, 0, 1, 1, 1, 0, id][..], value].concat();
        let lines = vec![
            "container_id: 0 [1..2]".to_string(),
            "state_id: 0 [2..3]".into(),
            "slot: 0 [3..5]".into(),
            format!("item: 1 × {stone} [5..{end}]"),
            "  count: 1 [5..6]".into(),
            format!("  item: {stone} [6..7]"),
            format!("  components: 1 added, 0 removed [7..{end}]"),
        ];
        (body, lines)
    }

    fn check_with(
        range: RangeInclusive<i32>,
        name: &str,
        value: &[u8],
        expected: impl Fn(i32) -> Vec<String>,
    ) {
        for protocol in protocols(range) {
            let (body, mut lines) = stack_with(protocol, name, value);
            lines.extend(expected(protocol).iter().map(|line| format!("    {line}")));
            assert_decodes(
                protocol..=protocol,
                Game,
                Clientbound,
                "container_set_slot",
                &body,
                &lines,
            );
        }
    }

    fn check(range: RangeInclusive<i32>, name: &str, value: &[u8], expected: &[&str]) {
        check_with(range, name, value, |_| {
            expected.iter().map(|line| line.to_string()).collect()
        });
    }

    #[test]
    fn every_component_has_a_decoder() {
        for protocol in protocols(V1_20_5..=NEWEST) {
            let table = RegistryTable::for_protocol(protocol).unwrap();
            for kind in table.names(ClientRegistry::DataComponentType) {
                let mut d = Decoder::new(Reader::new(&[]), protocol);
                if let Err(err) = value(&mut d, kind, "value".into()) {
                    assert!(!err.unsupported, "{kind} in {protocol}: {}", err.message);
                }
            }
        }
    }

    #[test]
    fn tooltip_flags_until_1_21_5() {
        check(
            V1_20_5..=V1_21_5 - 1,
            "enchantments",
            &[1, 7, 3, 1],
            &[
                "enchantments:  [9..14]",
                "  value: 1 item [10..13]",
                "    0: #7 level 3 [11..13]",
                "  show_in_tooltip: true [13..14]",
            ],
        );
        check(
            V1_21_5..=NEWEST,
            "enchantments",
            &[1, 7, 3],
            &["enchantments: 1 item [9..13]", "  0: #7 level 3 [11..13]"],
        );
        check(
            V1_20_5..=V1_21_5 - 1,
            "can_break",
            &[1, 0, 0, 0, 1],
            &[
                "can_break:  [9..15]",
                "  value: 1 item [10..14]",
                "    0:  [11..14]",
                "      blocks: absent [11..12]",
                "      properties: absent [12..13]",
                "      nbt: absent [13..14]",
                "  show_in_tooltip: true [14..15]",
            ],
        );
        check(
            V1_21_5..=NEWEST,
            "can_break",
            &[1, 0, 0, 0, 0, 0],
            &[
                "can_break: 1 item [9..16]",
                "  0:  [11..16]",
                "    blocks: absent [11..12]",
                "    properties: absent [12..13]",
                "    nbt: absent [13..14]",
                "    components: 0 items [14..15]",
                "    predicates: 0 items [15..16]",
            ],
        );
        check(
            V1_20_5..=V1_21_5 - 1,
            "trim",
            &[1, 2, 1],
            &[
                "trim:  [9..13]",
                "  value:  [10..12]",
                "    material: #0 [10..11]",
                "    pattern: #1 [11..12]",
                "  show_in_tooltip: true [12..13]",
            ],
        );
        check(
            V1_21_5..=NEWEST,
            "trim",
            &[1, 2],
            &[
                "trim:  [9..12]",
                "  material: #0 [10..11]",
                "  pattern: #1 [11..12]",
            ],
        );
    }

    #[test]
    fn attribute_modifiers_by_version() {
        let modifier = |protocol: i32, depth: &str, at: usize| {
            let attribute = entry(protocol, ClientRegistry::Attribute, 0);
            [
                format!("attribute: {attribute} [11..12]"),
                format!("amount: 1.0 [{at}..{}]", at + 8),
                format!("operation: add_value (0) [{}..{}]", at + 8, at + 9),
                format!("slot: mainhand (1) [{}..{}]", at + 9, at + 10),
            ]
            .map(|line| format!("{depth}{line}"))
        };
        let uuid = [0x12; 16];
        check_with(
            V1_20_5..=V1_20_5,
            "attribute_modifiers",
            &[&[1, 0][..], &uuid, &[1, b'n'], &f64s(&[1.0]), &[0, 1, 1]].concat(),
            |protocol| {
                let [attribute, amount, operation, slot] = modifier(protocol, "      ", 30);
                vec![
                    "attribute_modifiers:  [9..41]".to_string(),
                    "  value: 1 item [10..40]".into(),
                    "    0:  [11..40]".into(),
                    attribute,
                    "      uuid: 12121212-1212-1212-1212-121212121212 [12..28]".into(),
                    "      name: \"n\" [28..30]".into(),
                    amount,
                    operation,
                    slot,
                    "  show_in_tooltip: true [40..41]".into(),
                ]
            },
        );
        let identified = [&[1, 0, 3][..], b"a:b", &f64s(&[1.0]), &[0, 1]].concat();
        let entry_lines = |protocol: i32, depth: &str| {
            let [attribute, amount, operation, slot] = modifier(protocol, depth, 16);
            vec![
                attribute,
                format!("{depth}id: \"a:b\" [12..16]"),
                amount,
                operation,
                slot,
            ]
        };
        check_with(
            V1_21..=V1_21_5 - 1,
            "attribute_modifiers",
            &[&identified[..], &[1]].concat(),
            |protocol| {
                let mut lines = vec![
                    "attribute_modifiers:  [9..27]".to_string(),
                    "  value: 1 item [10..26]".into(),
                    "    0:  [11..26]".into(),
                ];
                lines.extend(entry_lines(protocol, "      "));
                lines.push("  show_in_tooltip: true [26..27]".into());
                lines
            },
        );
        check_with(
            V1_21_5..=V1_21_5,
            "attribute_modifiers",
            &identified,
            |protocol| {
                let mut lines = vec![
                    "attribute_modifiers: 1 item [9..26]".to_string(),
                    "  0:  [11..26]".into(),
                ];
                lines.extend(entry_lines(protocol, "    "));
                lines
            },
        );
        check_with(
            V1_21_6..=NEWEST,
            "attribute_modifiers",
            &[&identified[..], &[0]].concat(),
            |protocol| {
                let mut lines = vec![
                    "attribute_modifiers: 1 item [9..27]".to_string(),
                    "  0:  [11..27]".into(),
                ];
                lines.extend(entry_lines(protocol, "    "));
                lines.push("    display: default (0) [26..27]".into());
                lines
            },
        );
    }

    #[test]
    fn food_by_version() {
        let basics = [&[4][..], &f32s(&[0.5]), &[0]].concat();
        let basic_lines = [
            "  nutrition: 4 [10..11]",
            "  saturation: 0.5 [11..15]",
            "  can_always_eat: false [15..16]",
        ];
        let eat = [&basics[..], &f32s(&[1.6])].concat();
        check(
            V1_20_5..=V1_20_5,
            "food",
            &[&eat[..], &[0]].concat(),
            &[
                &["food:  [9..21]"][..],
                &basic_lines,
                &["  eat_seconds: 1.6 [16..20]", "  effects: 0 items [20..21]"],
            ]
            .concat(),
        );
        check(
            V1_21..=V1_21,
            "food",
            &[&eat[..], &[0, 0]].concat(),
            &[
                &["food:  [9..22]"][..],
                &basic_lines,
                &[
                    "  eat_seconds: 1.6 [16..20]",
                    "  using_converts_to: absent [20..21]",
                    "  effects: 0 items [21..22]",
                ],
            ]
            .concat(),
        );
        check(
            V1_21_2..=NEWEST,
            "food",
            &basics,
            &[&["food:  [9..16]"][..], &basic_lines].concat(),
        );
    }

    #[test]
    fn consume_effects_stop_decoding() {
        let consumable = [&f32s(&[1.6])[..], &[1, 1, 1]].concat();
        let lines = |protocol: i32, depth: &str| {
            let sound = entry(protocol, ClientRegistry::SoundEvent, 0);
            [
                "consume_seconds: 1.6 [10..14]".to_string(),
                "animation: eat (1) [14..15]".into(),
                format!("sound: {sound} [15..16]"),
                "has_consume_particles: true [16..17]".into(),
            ]
            .map(|line| format!("{depth}{line}"))
        };
        check_with(
            V1_21_2..=NEWEST,
            "consumable",
            &[&consumable[..], &[0]].concat(),
            |protocol| {
                let mut expected = vec!["consumable:  [9..18]".to_string()];
                expected.extend(lines(protocol, "  "));
                expected.push("  on_consume_effects: 0 items [17..18]".into());
                expected
            },
        );
        for protocol in protocols(V1_21_2..=NEWEST) {
            let value = [&consumable[..], &[1, 3]].concat();
            let (body, _) = stack_with(protocol, "consumable", &value);
            let stone = entry(protocol, ClientRegistry::Item, 1);
            let mut expected = vec![
                "container_id: 0 [1..2]".to_string(),
                "state_id: 0 [2..3]".into(),
                "slot: 0 [3..5]".into(),
                "item:  [5..19]".into(),
                "  count: 1 [5..6]".into(),
                format!("  item: {stone} [6..7]"),
                "  components:  [7..19]".into(),
                "    consumable:  [9..19]".into(),
            ];
            expected.extend(lines(protocol, "      "));
            expected.extend([
                "      on_consume_effects:  [17..19]".into(),
                "        0: #3 [18..19]".into(),
            ]);
            assert_partial(
                protocol..=protocol,
                Game,
                Clientbound,
                "container_set_slot",
                &body,
                &expected,
                (19, "consume effect #3 isn't decoded yet"),
            );
        }
    }

    #[test]
    fn profile_by_version() {
        let partial = [&[1, 3][..], b"Bob", &[0, 0]].concat();
        check(
            V1_20_5..=V1_21_9 - 1,
            "profile",
            &partial,
            &[
                "profile:  [9..17]",
                "  name: \"Bob\" [10..15]",
                "  id: absent [15..16]",
                "  properties: 0 items [16..17]",
            ],
        );
        check(
            V1_21_9..=NEWEST,
            "profile",
            &[&[0][..], &partial, &[0, 0, 0, 1, 1]].concat(),
            &[
                "profile:  [9..23]",
                "  partial_profile:  [10..18]",
                "    name: \"Bob\" [11..16]",
                "    id: absent [16..17]",
                "    properties: 0 items [17..18]",
                "  skin:  [18..23]",
                "    body: absent [18..19]",
                "    cape: absent [19..20]",
                "    elytra: absent [20..21]",
                "    model: slim [21..23]",
            ],
        );
    }

    #[test]
    fn stacks_inside_components_became_templates_in_26_1() {
        let lines = |protocol: i32, first: &str, second: &str| {
            let stone = entry(protocol, ClientRegistry::Item, 1);
            vec![
                "bundle_contents: 1 item [9..15]".to_string(),
                format!("  0: 3 × {stone} [11..15]"),
                format!("    {first} [11..12]").replace("STONE", &stone),
                format!("    {second} [12..13]").replace("STONE", &stone),
                "    components: none [13..15]".into(),
            ]
        };
        check_with(
            V1_20_5..=V26_1 - 1,
            "bundle_contents",
            &[1, 3, 1, 0, 0],
            |protocol| lines(protocol, "count: 3", "item: STONE"),
        );
        check_with(
            V26_1..=NEWEST,
            "bundle_contents",
            &[1, 1, 3, 0, 0],
            |protocol| lines(protocol, "item: STONE", "count: 3"),
        );
    }

    #[test]
    fn tropical_fish_patterns_pack_size_and_index() {
        check(
            V1_21_5..=NEWEST,
            "tropical_fish/pattern",
            &[0x81, 0x02],
            &["tropical_fish/pattern: stripey (257) [9..12]"],
        );
    }
}
