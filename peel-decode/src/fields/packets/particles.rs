//! `ParticleOptions`, sent in entity data and particle packets.

use pomme_protocol::ClientRegistry;

use super::super::reader::{Reader, Result};
use super::super::{Decoder, Name};
use super::components::rgb;
use super::items::stack_or_template;
use super::{V1_20_3, V1_20_5, V1_21_2, V1_21_4, V1_21_9, registry_name};

/// `ParticleOptions`: the particle type, then the options its type has.
pub fn particle(d: &mut Decoder, name: Name) -> Result<()> {
    d.prefixed(Reader::var_int, |d, id| {
        let label = d.registry_entry(ClientRegistry::ParticleType, id);
        let kind = registry_name(d, ClientRegistry::ParticleType, id, "particle type")?;
        d.group(name, |d| particle_options(d, kind))?;
        d.show_last(label);
        Ok(())
    })
}

/// A particle type id, returning its name for [`particle_options`], for
/// 1.20.3 and older `level_particles`, which sends the options apart.
pub fn particle_type(d: &mut Decoder, name: &'static str) -> Result<&'static str> {
    let id = d.registry(name, ClientRegistry::ParticleType)?;
    registry_name(d, ClientRegistry::ParticleType, id, "particle type")
}

/// The options of a particle of type `kind`, if it has any.
pub fn particle_options(d: &mut Decoder, kind: &str) -> Result<()> {
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
