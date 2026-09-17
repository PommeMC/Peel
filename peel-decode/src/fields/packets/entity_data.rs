//! `set_entity_data`: synched entity data values, each tagged with the
//! serializer that wrote it.

use peel_trace::Direction::Clientbound;
use pomme_protocol::ClientRegistry;

use super::super::reader::{Reader, Result};
use super::super::{Decoder, NEWEST, Name, OLDEST, PacketDecoder, count};
use super::components::{dye_color, painting_variant, profile, rgb};
use super::items::{item_stack, stack_or_template};
use super::text::{component, optional_component};
use super::{
    DIRECTION, V1_20_3, V1_20_5, V1_21, V1_21_2, V1_21_4, V1_21_5, V1_21_9, V1_21_11, V26_1, V26_3,
    game, global_pos, holder, holder_set, nbt_tag, registry_name, string, variant,
};

pub static DECODERS: &[PacketDecoder] =
    &[game(Clientbound, "set_entity_data", OLDEST..=NEWEST, |d| {
        d.var_int("entity_id")?;
        let values = d.group("values", |d| {
            let mut values = 0;
            while d.reader().clone().u8()? != 0xFF {
                d.prefixed(
                    |r| Ok((r.u8()?, r.var_int()?)),
                    |d, (index, id)| {
                        let serializer = serializers(d.protocol())
                            .get(usize::try_from(id).unwrap_or(usize::MAX))
                            .ok_or_else(|| {
                                d.reader()
                                    .error(format!("entity data serializer #{id} doesn't exist"))
                            })?;
                        data_value(d, serializer, format!("{index} ({serializer})").into())
                    },
                )?;
                values += 1;
            }
            d.reader().u8()?;
            Ok(values)
        })?;
        d.show_last(count(values, "value"));
        Ok(())
    })];

/// `EntityDataSerializers` in registration order, which sets their ids.
fn serializers(protocol: i32) -> &'static [&'static str] {
    match protocol {
        V26_3.. => &[
            "byte",
            "int",
            "long",
            "float",
            "string",
            "component",
            "optional_component",
            "item_stack",
            "boolean",
            "rotations",
            "block_pos",
            "optional_block_pos",
            "direction",
            "optional_living_entity_reference",
            "block_state",
            "optional_block_state",
            "particle",
            "particles",
            "villager_data",
            "optional_unsigned_int",
            "pose",
            "cat_variant",
            "cat_sound_variant",
            "cow_variant",
            "cow_sound_variant",
            "wolf_variant",
            "wolf_sound_variant",
            "frog_variant",
            "pig_variant",
            "pig_sound_variant",
            "chicken_variant",
            "chicken_sound_variant",
            "zombie_nautilus_variant",
            "optional_global_pos",
            "painting_variant",
            "sniffer_state",
            "armadillo_state",
            "copper_golem_state",
            "weathering_copper_state",
            "vector3",
            "quaternion",
            "resolvable_profile",
            "humanoid_arm",
            "dye_color",
        ],
        V26_1.. => &serializers(V26_3)[..43],
        V1_21_11.. => &[
            "byte",
            "int",
            "long",
            "float",
            "string",
            "component",
            "optional_component",
            "item_stack",
            "boolean",
            "rotations",
            "block_pos",
            "optional_block_pos",
            "direction",
            "optional_living_entity_reference",
            "block_state",
            "optional_block_state",
            "particle",
            "particles",
            "villager_data",
            "optional_unsigned_int",
            "pose",
            "cat_variant",
            "cow_variant",
            "wolf_variant",
            "wolf_sound_variant",
            "frog_variant",
            "pig_variant",
            "chicken_variant",
            "zombie_nautilus_variant",
            "optional_global_pos",
            "painting_variant",
            "sniffer_state",
            "armadillo_state",
            "copper_golem_state",
            "weathering_copper_state",
            "vector3",
            "quaternion",
            "resolvable_profile",
            "humanoid_arm",
        ],
        V1_21_9.. => &[
            "byte",
            "int",
            "long",
            "float",
            "string",
            "component",
            "optional_component",
            "item_stack",
            "boolean",
            "rotations",
            "block_pos",
            "optional_block_pos",
            "direction",
            "optional_living_entity_reference",
            "block_state",
            "optional_block_state",
            "particle",
            "particles",
            "villager_data",
            "optional_unsigned_int",
            "pose",
            "cat_variant",
            "cow_variant",
            "wolf_variant",
            "wolf_sound_variant",
            "frog_variant",
            "pig_variant",
            "chicken_variant",
            "optional_global_pos",
            "painting_variant",
            "sniffer_state",
            "armadillo_state",
            "copper_golem_state",
            "weathering_copper_state",
            "vector3",
            "quaternion",
            "resolvable_profile",
        ],
        V1_21_5.. => &[
            "byte",
            "int",
            "long",
            "float",
            "string",
            "component",
            "optional_component",
            "item_stack",
            "boolean",
            "rotations",
            "block_pos",
            "optional_block_pos",
            "direction",
            "optional_living_entity_reference",
            "block_state",
            "optional_block_state",
            "compound_tag",
            "particle",
            "particles",
            "villager_data",
            "optional_unsigned_int",
            "pose",
            "cat_variant",
            "cow_variant",
            "wolf_variant",
            "wolf_sound_variant",
            "frog_variant",
            "pig_variant",
            "chicken_variant",
            "optional_global_pos",
            "painting_variant",
            "sniffer_state",
            "armadillo_state",
            "vector3",
            "quaternion",
        ],
        V1_20_5.. => &[
            "byte",
            "int",
            "long",
            "float",
            "string",
            "component",
            "optional_component",
            "item_stack",
            "boolean",
            "rotations",
            "block_pos",
            "optional_block_pos",
            "direction",
            "optional_uuid",
            "block_state",
            "optional_block_state",
            "compound_tag",
            "particle",
            "particles",
            "villager_data",
            "optional_unsigned_int",
            "pose",
            "cat_variant",
            "wolf_variant",
            "frog_variant",
            "optional_global_pos",
            "painting_variant",
            "sniffer_state",
            "armadillo_state",
            "vector3",
            "quaternion",
        ],
        _ => &[
            "byte",
            "int",
            "long",
            "float",
            "string",
            "component",
            "optional_component",
            "item_stack",
            "boolean",
            "rotations",
            "block_pos",
            "optional_block_pos",
            "direction",
            "optional_uuid",
            "block_state",
            "optional_block_state",
            "compound_tag",
            "particle",
            "villager_data",
            "optional_unsigned_int",
            "pose",
            "cat_variant",
            "frog_variant",
            "optional_global_pos",
            "painting_variant",
            "sniffer_state",
            "vector3",
            "quaternion",
        ],
    }
}

fn data_value(d: &mut Decoder, serializer: &str, name: Name) -> Result<()> {
    let protocol = d.protocol();
    match serializer {
        "byte" => d.i8(name).map(drop),
        "int" => d.var_int(name).map(drop),
        "long" => d.var_long(name).map(drop),
        "float" => d.f32(name).map(drop),
        "string" => string(d, name),
        "component" => component(d, name),
        "optional_component" => optional_component(d, name),
        "item_stack" => item_stack(d, name),
        "boolean" => d.bool(name).map(drop),
        "rotations" | "vector3" => d.vector3f(name).map(drop),
        "quaternion" => d
            .read(
                name,
                |r| Ok([r.f32()?, r.f32()?, r.f32()?, r.f32()?]),
                |[x, y, z, w]| format!("{x:?}, {y:?}, {z:?}, {w:?}"),
            )
            .map(drop),
        "block_pos" => d.block_pos(name).map(drop),
        "optional_block_pos" => d.optional(name, |d, name| d.block_pos(name)).map(drop),
        "direction" => d.variant(name, Reader::var_int, DIRECTION).map(drop),
        "optional_uuid" | "optional_living_entity_reference" => {
            d.optional(name, |d, name| d.uuid(name)).map(drop)
        }
        "block_state"
        | "cat_variant"
        | "cat_sound_variant"
        | "cow_variant"
        | "cow_sound_variant"
        | "wolf_sound_variant"
        | "frog_variant"
        | "pig_variant"
        | "pig_sound_variant"
        | "chicken_variant"
        | "chicken_sound_variant"
        | "zombie_nautilus_variant" => d.raw_id(name).map(drop),
        "optional_block_state" => d
            .read(name, Reader::var_int, |&id| match id {
                0 => "none".into(),
                id => format!("#{id}"),
            })
            .map(drop),
        "optional_unsigned_int" => d
            .read(name, Reader::optional_var_int, |value| {
                value.map_or_else(|| "none".into(), |value| value.to_string())
            })
            .map(drop),
        "compound_tag" => nbt_tag(d, name),
        "particle" => particle(d, name),
        "particles" => d.list(name, particle).map(drop),
        "villager_data" => d.group(name, |d| {
            d.raw_id("type")?;
            d.raw_id("profession")?;
            d.var_int("level")?;
            Ok(())
        }),
        "pose" => {
            let poses = [
                "standing",
                "fall_flying",
                "sleeping",
                "swimming",
                "spin_attack",
                "crouching",
                "long_jumping",
                "dying",
                "croaking",
                "using_tongue",
                "sitting",
                "roaring",
                "sniffing",
                "emerging",
                "digging",
                "sliding",
                "shooting",
                "inhaling",
            ];
            let known = if protocol >= V1_20_3 { 18 } else { 15 };
            d.variant(name, Reader::var_int, &poses[..known]).map(drop)
        }
        "wolf_variant" if (V1_21..V1_21_5).contains(&protocol) => holder(d, name, None, |d| {
            d.string("wild_texture")?;
            d.string("tame_texture")?;
            d.string("angry_texture")?;
            holder_set(d, "biomes", None)
        }),
        "wolf_variant" => d.raw_id(name).map(drop),
        "painting_variant" if protocol < V1_21 => d.raw_id(name).map(drop),
        "painting_variant" => painting_variant(d, name),
        "optional_global_pos" => d.optional(name, global_pos).map(drop),
        "sniffer_state" => variant(
            d,
            name,
            &[
                "idling",
                "feeling_happy",
                "scenting",
                "sniffing",
                "searching",
                "digging",
                "rising",
            ],
        ),
        "armadillo_state" => variant(d, name, &["idle", "rolling", "scared", "unrolling"]),
        "copper_golem_state" => variant(
            d,
            name,
            &[
                "idle",
                "getting_item",
                "getting_no_item",
                "dropping_item",
                "dropping_no_item",
            ],
        ),
        "weathering_copper_state" => {
            variant(d, name, &["unaffected", "exposed", "weathered", "oxidized"])
        }
        "humanoid_arm" => variant(d, name, &["left", "right"]),
        "resolvable_profile" => d.group(name, profile),
        "dye_color" => dye_color(d, name),
        _ => Err(d
            .reader()
            .unsupported(format!("entity data serializer {serializer}"))),
    }
}

/// `ParticleOptions`: the particle type, then the options its type has.
fn particle(d: &mut Decoder, name: Name) -> Result<()> {
    d.prefixed(Reader::var_int, |d, id| {
        let label = d.registry_entry(ClientRegistry::ParticleType, id);
        let kind = registry_name(d, ClientRegistry::ParticleType, id, "particle type")?;
        d.group(name, |d| particle_options(d, kind))?;
        d.show_last(label);
        Ok(())
    })
}

fn particle_options(d: &mut Decoder, kind: &str) -> Result<()> {
    let protocol = d.protocol();
    match kind {
        "block" | "block_marker" | "falling_dust" | "dust_pillar" | "block_crumble" => {
            d.raw_id("block_state")?;
        }
        "dust" => {
            if protocol >= V1_21_2 {
                rgb(d, "color".into())?;
            } else {
                d.vector3f("color")?;
            }
            d.f32("scale")?;
        }
        "dust_color_transition" => match protocol {
            V1_21_2.. => {
                rgb(d, "from_color".into())?;
                rgb(d, "to_color".into())?;
                d.f32("scale")?;
            }
            V1_20_5.. => {
                d.vector3f("from_color")?;
                d.vector3f("to_color")?;
                d.f32("scale")?;
            }
            _ => {
                d.vector3f("from_color")?;
                d.f32("scale")?;
                d.vector3f("to_color")?;
            }
        },
        "sculk_charge" => {
            d.f32("roll")?;
        }
        "shriek" => {
            d.var_int("delay")?;
        }
        "item" => stack_or_template(d, "item".into())?,
        "vibration" => {
            if protocol >= V1_20_3 {
                let source = d.raw_id("source_type")?;
                return Err(d.reader().unsupported(format!("position source #{source}")));
            }
            let source = d.string("source_type")?.to_owned();
            match source.as_str() {
                "minecraft:block" => {
                    d.block_pos("block_pos")?;
                }
                "minecraft:entity" => {
                    d.var_int("entity_id")?;
                    d.f32("y_offset")?;
                }
                other => {
                    return Err(d
                        .reader()
                        .error(format!("position source {other} doesn't exist")));
                }
            }
            d.var_int("arrival_in_ticks")?;
        }
        "entity_effect" if protocol >= V1_20_5 => rgb(d, "color".into())?,
        "tinted_leaves" => rgb(d, "color".into())?,
        "flash" if protocol >= V1_21_9 => rgb(d, "color".into())?,
        "trail" => {
            d.vec3("target")?;
            rgb(d, "color".into())?;
            if protocol >= V1_21_4 {
                d.var_int("duration")?;
            }
        }
        "dragon_breath" if protocol >= V1_21_9 => {
            d.f32("power")?;
        }
        "effect" | "instant_effect" if protocol >= V1_21_9 => {
            rgb(d, "color".into())?;
            d.f32("power")?;
        }
        "geyser" | "geyser_plume" => {
            d.i32("water_blocks")?;
        }
        "geyser_base" | "geyser_poof" => {
            d.i32("water_blocks")?;
            d.f32("burst_impulse_base")?;
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use peel_trace::Phase::Game;

    use super::super::super::testing::{
        assert_decodes, assert_partial, block_pos, entry, f32s, protocols, registry_id,
    };
    use super::*;

    fn serializer_id(protocol: i32, name: &str) -> u8 {
        serializers(protocol)
            .iter()
            .position(|s| *s == name)
            .unwrap() as u8
    }

    #[test]
    fn serializer_ids_by_version() {
        for (protocol, len, particle, optional_unsigned_int) in [
            (OLDEST, 28, 17, 19),
            (V1_20_5, 31, 17, 20),
            (V1_21_5, 35, 17, 20),
            (V1_21_9, 37, 16, 19),
            (V1_21_11, 39, 16, 19),
            (V26_1, 43, 16, 19),
            (V26_3, 44, 16, 19),
        ] {
            assert_eq!(serializers(protocol).len(), len, "protocol {protocol}");
            assert_eq!(serializer_id(protocol, "particle"), particle);
            assert_eq!(
                serializer_id(protocol, "optional_unsigned_int"),
                optional_unsigned_int
            );
        }
    }

    #[test]
    fn values_until_the_end_marker() {
        assert_decodes(
            OLDEST..=NEWEST,
            Game,
            Clientbound,
            "set_entity_data",
            &[3, 0, 0, 5, 1, 15, 0, 2, 15, 9, 0xff],
            &[
                "entity_id: 3 [1..2]",
                "values: 3 values [2..12]",
                "  0 (byte): 5 [2..5]",
                "  1 (optional_block_state): none [5..8]",
                "  2 (optional_block_state): #9 [8..11]",
            ],
        );
    }

    #[test]
    fn particle_options_by_version() {
        for protocol in protocols(OLDEST..=NEWEST) {
            let dust = registry_id(protocol, ClientRegistry::ParticleType, "dust");
            let head = [3, 4, serializer_id(protocol, "particle"), dust];
            let label = entry(protocol, ClientRegistry::ParticleType, u32::from(dust));
            let (options, lines): (Vec<u8>, _) = if protocol >= V1_21_2 {
                (
                    [&0xff0000i32.to_be_bytes()[..], &f32s(&[2.0])].concat(),
                    ["    color: ff0000 [5..9]", "    scale: 2.0 [9..13]"],
                )
            } else {
                (
                    f32s(&[1.0, 0.0, 0.0, 2.0]),
                    [
                        "    color: 1.0, 0.0, 0.0 [5..17]",
                        "    scale: 2.0 [17..21]",
                    ],
                )
            };
            let end = 5 + options.len();
            let expected = [
                "entity_id: 3 [1..2]".to_string(),
                format!("values: 1 value [2..{}]", end + 1),
                format!("  4 (particle): {label} [2..{end}]"),
                lines[0].into(),
                lines[1].into(),
            ];
            assert_decodes(
                protocol..=protocol,
                Game,
                Clientbound,
                "set_entity_data",
                &[&head[..], &options, &[0xff]].concat(),
                &expected,
            );
        }
    }

    #[test]
    fn vibrations_stop_at_position_source_ids() {
        for protocol in protocols(OLDEST..=NEWEST) {
            let vibration = registry_id(protocol, ClientRegistry::ParticleType, "vibration");
            let head = [3, 4, serializer_id(protocol, "particle"), vibration];
            let label = entry(protocol, ClientRegistry::ParticleType, u32::from(vibration));
            if protocol < V1_20_3 {
                let body = [
                    &head[..],
                    &[15],
                    b"minecraft:block",
                    &block_pos(1, 2, 3),
                    &[7, 0xff],
                ]
                .concat();
                assert_decodes(
                    protocol..=protocol,
                    Game,
                    Clientbound,
                    "set_entity_data",
                    &body,
                    &[
                        "entity_id: 3 [1..2]".to_string(),
                        "values: 1 value [2..31]".into(),
                        format!("  4 (particle): {label} [2..30]"),
                        "    source_type: \"minecraft:block\" [5..21]".into(),
                        "    block_pos: 1, 2, 3 [21..29]".into(),
                        "    arrival_in_ticks: 7 [29..30]".into(),
                    ],
                );
            } else {
                assert_partial(
                    protocol..=protocol,
                    Game,
                    Clientbound,
                    "set_entity_data",
                    &[&head[..], &[0]].concat(),
                    &[
                        "entity_id: 3 [1..2]",
                        "values:  [2..6]",
                        "  4 (particle):  [2..6]",
                        "    source_type: #0 [5..6]",
                    ],
                    (6, "position source #0 isn't decoded yet"),
                );
            }
        }
    }
}
