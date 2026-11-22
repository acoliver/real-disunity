//! How a draw's texture slots are bound: the texture each slot reads, the stand-in for a slot no
//! layer explains, the partially written mip chains that are noted, and the sampler of each slot.
use std::collections::HashMap;

use worldrunner_render_post::contracts::{
    ResourceUse, SamplerBinding, Semantic, TextureBinding, TextureBindingKind, UseKind,
};

use crate::layers::TextureInput;
use crate::layout::Slot;
use crate::unity::shader::PassSelector;

use super::error::OffscreenError;
use super::plan::{DrawPlan, DrawSpec, ProgramEntry, Services};
use super::sampling::{self, Sampled};
use super::slot_dimension;

/// The reason a draw reads the frame's grey for a slot no layer sets and the shader does not
/// declare, when no layer says why the slot has no texture.
const UNDECLARED_REASON: &str = "undeclared";

/// The reason a draw samples a render texture that no draw, copy or blit has written, so that it
/// holds the zeros it was made with.
const UNWRITTEN_REASON: &str = "unwritten-render-texture";

/// The texture a slot reads and why it is not a layer's own: see `Services::texture_input`.
struct ChosenTexture {
    input: TextureInput,
    /// The reason it is a stand-in, if it is one.
    stand_in: Option<String>,
}

impl Services<'_> {
    /// Checks that every texture slot has a texture to read.
    pub(super) fn require_textures(
        &self,
        spec: &DrawSpec<'_>,
        entry: &ProgramEntry,
        slots: &[Slot],
    ) -> Result<(), OffscreenError> {
        for slot in slots {
            if !matches!(slot.ty, wgpu::BindingType::Texture { .. }) {
                continue;
            }
            let Some(target) = entry
                .textures
                .iter()
                .find(|t| (t.set, t.binding) == (slot.set, slot.binding))
            else {
                continue;
            };
            self.texture_input(spec, target.name.resolve(&[])?, slot)?;
        }
        Ok(())
    }

    /// The texture a slot of property `name` reads, and the reason when it is a stand-in.
    ///
    /// A layer's texture wins. It is a stand-in when its layer says why, with the reason of the
    /// highest layer that explains the property (see [`crate::layers::Layers::stand_in`]), and
    /// when it is a render texture that nothing has written (`unwritten-render-texture`). With
    /// none, and when the shader does not declare the property, this engine reads a grey default
    /// texture of the
    /// slot's dimension, so the frame's `undeclared_textures` stand in, for a slot that samples a
    /// single-sampled float texture of a dimension with a default. The reason is the one a layer
    /// explains the missing texture with, or `undeclared`. A declared property, a slot of another
    /// kind or dimension and a frame without the texture leave the property missing.
    fn texture_input(
        &self,
        spec: &DrawSpec<'_>,
        name: &str,
        slot: &Slot,
    ) -> Result<ChosenTexture, OffscreenError> {
        if let Some(input) = spec.layers.texture(name) {
            let unwritten = self.textures.never_written(input.texture)
                && self.tracker.is_unwritten(input.texture);
            return Ok(ChosenTexture {
                input: *input,
                stand_in: spec
                    .layers
                    .stand_in(name)
                    .or_else(|| unwritten.then_some(UNWRITTEN_REASON))
                    .map(str::to_owned),
            });
        }
        let default = match slot.ty {
            wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { .. },
                view_dimension,
                multisampled: false,
            } => self.undeclared_textures.for_view(view_dimension),
            _ => None,
        };
        let declared = self
            .shaders
            .get(&spec.shader)
            .is_some_and(|shader| shader.declares_texture(name));
        match default {
            Some(texture) if !declared => Ok(ChosenTexture {
                input: TextureInput {
                    texture,
                    sampler: None,
                },
                stand_in: Some(
                    spec.layers
                        .explanation(name)
                        .unwrap_or(UNDECLARED_REASON)
                        .to_owned(),
                ),
            }),
            _ => Err(OffscreenError::MissingTexture {
                name: name.to_owned(),
            }),
        }
    }

    /// Binds the texture a layer names for the slot's parameter, or the frame's default for an
    /// undeclared one, and returns the sampler state it is read with.
    pub(super) fn bind_texture(
        &mut self,
        spec: &DrawSpec<'_>,
        entry: &ProgramEntry,
        slot: &Slot,
        plan: &mut DrawPlan,
    ) -> Result<Sampled, OffscreenError> {
        let wgpu::BindingType::Texture {
            sample_type,
            view_dimension,
            ..
        } = slot.ty
        else {
            return Ok(Sampled::default());
        };
        let target = entry
            .textures
            .iter()
            .find(|t| (t.set, t.binding) == (slot.set, slot.binding))
            .ok_or(OffscreenError::UnsupportedBinding {
                set: slot.set,
                binding: slot.binding,
                what: "a texture no parameter names",
            })?;
        let name = target.name.resolve(&[])?;
        self.counts.note_texture_lookup(name);
        let ChosenTexture { input, stand_in } = self.texture_input(spec, name, slot)?;
        if let Some(reason) = stand_in {
            plan.stand_ins.push((name.to_owned(), reason));
        }
        if let Some(facts) = plan.facts.as_mut() {
            facts.textures.push((name.to_owned(), input.texture));
        }
        let (imported, own) = self.import(input.texture)?;
        let stored = self.textures.get(input.texture)?.dimension;
        slot_dimension::refuse(name, input.texture, stored, view_dimension)?;
        // A depth slot is read through a comparison sampler, which takes the depth aspect alone:
        // the depth of a depth-only render texture, whose format has no stencil. A colour texture,
        // and the depth buffer under one, have none to give.
        let depth_only =
            imported.format.has_depth_aspect() && !imported.format.has_stencil_aspect();
        if sample_type == wgpu::TextureSampleType::Depth && !depth_only {
            return Err(OffscreenError::UnsupportedBinding {
                set: slot.set,
                binding: slot.binding,
                what: "a depth texture slot over a texture with no depth-only storage",
            });
        }
        let versions = self.tracker.sampled_levels(input.texture, imported)?;
        let mips = u32::try_from(versions.len()).unwrap_or(u32::MAX);
        if mips < imported.mips {
            self.counts.partial_mips += 1;
            self.note_partial_read(spec, entry, (name, mips, imported.mips));
        }
        let view = self
            .tracker
            .whole_view(imported, mips, view_dimension, versions[0])?;
        let features = self.runtime.device().features();
        let (sample_type, state) = sampling::read_unfilterable(
            sample_type,
            imported.format,
            features,
            input.sampler.or(own),
        );
        // One read per run of levels written at the same point; a single run is the view itself.
        let mut start = 0;
        while start < versions.len() {
            let end = versions[start..]
                .iter()
                .position(|&version| version != versions[start])
                .map_or(versions.len(), |offset| start + offset);
            let used = if start == 0 && end == versions.len() {
                view.clone()
            } else {
                let levels = u32::try_from(start).unwrap_or(u32::MAX)
                    ..u32::try_from(end).unwrap_or(u32::MAX);
                self.tracker
                    .levels_view(imported, levels, view_dimension, versions[start])?
            };
            plan.texture_uses
                .push(ResourceUse::read(used, UseKind::Sampled));
            start = end;
        }
        plan.textures.push((
            slot.set,
            TextureBinding {
                binding: slot.binding,
                visibility: slot.visibility,
                view,
                samples: 1,
                semantic: Semantic::Unspecified,
                kind: TextureBindingKind::Sampled(sample_type),
            },
        ));
        Ok(Sampled {
            state,
            unfiltered: sample_type == wgpu::TextureSampleType::Float { filterable: false },
        })
    }

    /// Counts a read of `texture` (property, levels bound, levels it has) by the pass that made it.
    fn note_partial_read(
        &mut self,
        spec: &DrawSpec<'_>,
        entry: &ProgramEntry,
        (texture, written, total): (&str, u32, u32),
    ) {
        let (key, tag) = match &spec.selector {
            PassSelector::Index(index) => ("index".to_owned(), Some(index.to_string())),
            PassSelector::Tag { key, .. } | PassSelector::TagWithUntagged { key, .. } => (
                key.clone(),
                self.shaders
                    .get(&spec.shader)
                    .and_then(|shader| shader.pass_tag(entry.pass, key)),
            ),
        };
        let found = self.counts.partial_reads.iter_mut().find(|read| {
            read.texture == texture
                && (read.written, read.total) == (written, total)
                && read.key == key
                && read.tag == tag
        });
        if let Some(read) = found {
            read.reads += 1;
        } else {
            self.counts.partial_reads.push(super::PartialMipReads {
                texture: texture.to_owned(),
                written,
                total,
                key,
                tag,
                reads: 1,
            });
        }
    }

    pub(super) fn bind_sampler(
        &mut self,
        entry: &ProgramEntry,
        slot: &Slot,
        kind: wgpu::SamplerBindingType,
        (states, any_unfiltered): (&HashMap<(u32, u32), Sampled>, bool),
        plan: &mut DrawPlan,
    ) {
        if kind == wgpu::SamplerBindingType::Comparison {
            // The runtime cannot create a comparison sampler for a program, so the draw lends
            // one: the shadow-map read of the texture the sampler is paired with.
            let at = (slot.set, slot.binding);
            let name = entry.comparison_names.get(&at).map(String::as_str);
            let state = sampling::comparison_state(name, states.get(&at));
            plan.samplers.push((
                slot.set,
                SamplerBinding {
                    binding: slot.binding,
                    visibility: slot.visibility,
                    sampler: self.samplers.comparison(state, sampling::SHADOW_COMPARE),
                },
            ));
            return;
        }
        let state =
            sampling::sampler_state(kind, states.get(&(slot.set, slot.binding)), any_unfiltered);
        plan.samplers.push((
            slot.set,
            SamplerBinding {
                binding: slot.binding,
                visibility: slot.visibility,
                sampler: self.samplers.get(state),
            },
        ));
    }
}
