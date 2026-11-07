//! Planning one draw: choosing the program, packing its constants, binding its textures and
//! samplers, and arranging the vertex streams the program reads. Nothing here touches the
//! constants buffer, whose size depends on every draw of the frame; a [`DrawPlan`] holds the
//! offsets its constants will have in it.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use worldrunner_render_post::contracts::{
    AllocationKey, BufferUse, BufferUseKind, BufferView, ResourceLifetime, ResourceUse,
    SamplerBinding, Semantic, TextureBinding, TextureBindingKind, UseKind,
};

use crate::budget::Reservation;
use crate::draw::VertexInput;
use crate::error::ProgramError;
use crate::ids::{MeshId, ShaderId, TextureId};
use crate::layers::Layers;
use crate::layout::Slot;
use crate::packet::TargetSignature;
use crate::program::GraphicsProgram;
use crate::resources::BufferHandle;
use crate::runtime::ProgramRuntime;
use crate::sampler::{Filter, SamplerCache, SamplerState, Wrap};
use crate::unity::constants::pack_layout;
use crate::unity::shader::{PassSelector, ProgramShader, Variant};
use crate::unity::state::{RenderState, resolve_state};
use crate::unity::targets::{ConstantBufferTarget, TextureTarget};
use worldrunner_unity_serialized::shader_parameters::ResourceKind;

use super::defaults::{DeclaredInputs, DefaultBuffers, Fill, declared_inputs, default_for};
use super::error::OffscreenError;
use super::meshes::MeshTable;
use super::textures::TextureTable;
use super::tracker::Tracker;

/// A graphics program with the parts of its parameters every draw of it needs.
pub(super) struct ProgramEntry {
    pub(super) program: GraphicsProgram,
    pub(super) variant: Arc<Variant>,
    /// The pass of the first sub-shader the program is.
    pass: usize,
    constants: Vec<ConstantBufferTarget>,
    textures: Vec<TextureTarget>,
    /// The sampler binding split from the texture at a set and binding.
    samplers: HashMap<(u32, u32), u32>,
    /// The vertex inputs the vertex stage declares, by location.
    inputs: DeclaredInputs,
}

/// The constants of a frame's draws, one slice for each constant buffer binding.
pub(super) struct ConstantsPlan {
    pub(super) bytes: Vec<u8>,
    alignment: usize,
}

impl ConstantsPlan {
    pub(super) fn new(alignment: u32) -> Self {
        Self {
            bytes: Vec::new(),
            alignment: alignment as usize,
        }
    }

    /// Where the next draw's constants start, to give back to [`rollback`](Self::rollback).
    pub(super) fn mark(&self) -> usize {
        self.bytes.len()
    }

    /// Drops the constants pushed since `mark`, for a draw that was left out after packing them.
    pub(super) fn rollback(&mut self, mark: usize) {
        self.bytes.truncate(mark);
    }

    fn push(&mut self, slice: &[u8]) -> u64 {
        let start = self.bytes.len().next_multiple_of(self.alignment);
        self.bytes.resize(start, 0);
        self.bytes.extend(slice);
        start as u64
    }
}

/// A constant buffer binding whose bytes are in the constants plan at `offset`.
pub(super) struct PendingConstant {
    pub(super) set: u32,
    pub(super) binding: u32,
    pub(super) visibility: wgpu::ShaderStages,
    pub(super) offset: u64,
    pub(super) len: u64,
}

/// Everything a draw needs but the constants buffer.
pub(super) struct DrawPlan {
    pub(super) entry: Arc<ProgramEntry>,
    pub(super) state: RenderState,
    pub(super) constants: Vec<PendingConstant>,
    pub(super) textures: Vec<(u32, TextureBinding)>,
    pub(super) samplers: Vec<(u32, SamplerBinding)>,
    pub(super) texture_uses: Vec<ResourceUse>,
}

/// The first shader input number of the tables that number their inputs as the Unity 6 player's
/// do for most variants (10,002 of 10,773 in the Old World player): the module's location is the
/// number minus 13. A table with every number below 13 gives the locations themselves.
const PROGRAM_INPUT_BASE: i8 = 13;

/// The number to subtract from a table's shader inputs to get the module's locations.
fn input_base(table: &[[i8; 2]]) -> Result<i8, OffscreenError> {
    let mut inputs = table.iter().map(|pair| pair[1]);
    let Some(first) = inputs.next() else {
        return Ok(0);
    };
    let (low, high) = inputs.fold((first, first), |(low, high), input| {
        (low.min(input), high.max(input))
    });
    match (low >= PROGRAM_INPUT_BASE, high >= PROGRAM_INPUT_BASE) {
        (true, true) => Ok(PROGRAM_INPUT_BASE),
        (false, false) => Ok(0),
        _ => Err(OffscreenError::MixedInputNumbering { low, high }),
    }
}

/// The vertex streams a program reads from a mesh.
pub(super) struct VertexPlan {
    pub(super) inputs: Vec<VertexInput>,
    pub(super) views: Vec<BufferView>,
    pub(super) uses: Vec<BufferUse>,
    /// The (source channel, shader input, format) of each input that reads a constant because the
    /// mesh lacks the channel.
    pub(super) defaulted: Vec<(i8, i8, wgpu::VertexFormat)>,
}

/// What chooses a draw's program and feeds its parameters.
pub(super) struct DrawSpec<'a> {
    pub(super) shader: ShaderId,
    pub(super) keywords: Vec<&'a str>,
    pub(super) selector: PassSelector,
    pub(super) layers: Layers<'a>,
}

/// What planning a frame's draws left out or narrowed, for the host to report.
#[derive(Default)]
pub(super) struct DrawCounts {
    /// Camera draws left out because the shader has no pass the camera selects.
    pub(super) unmatched: usize,
    /// Camera draws left out because no layer names a texture the pass samples, in all and by the
    /// texture property.
    pub(super) missing_texture: usize,
    pub(super) missing_textures: std::collections::BTreeMap<String, usize>,
    /// Camera draws left out because the mesh lacks a vertex channel the pass reads, in all and
    /// by (source channel, shader input).
    pub(super) missing_channel: usize,
    pub(super) missing_channels: std::collections::BTreeMap<(i8, i8), usize>,
    /// Draws that read a constant for a channel their mesh lacks, by channel, input and format.
    pub(super) defaulted: Vec<super::DefaultedChannelDraws>,
    /// Texture reads that bound fewer levels than the texture has.
    pub(super) partial_mips: usize,
    /// What each camera render recorded, in call order.
    pub(super) cameras: Vec<super::CameraDraws>,
    /// The draws left out, by what the shader's passes have against what the camera selected.
    pub(super) unmatched_passes: Vec<super::UnmatchedPasses>,
    /// The partial texture reads, by what read them.
    pub(super) partial_reads: Vec<super::PartialMipReads>,
    /// Why each camera render left draws out, in the order of `cameras`, and the camera now being
    /// planned.
    pub(super) camera_causes: Vec<super::CameraCauses>,
    pub(super) current: super::CameraCauses,
    /// Programs built because the state had none, and the time draws spent choosing programs.
    pub(super) programs_built: usize,
    pub(super) program_time: std::time::Duration,
}

impl DrawCounts {
    /// Counts one draw that read a constant for each of `defaulted`.
    pub(super) fn note_defaulted(&mut self, defaulted: &[(i8, i8, wgpu::VertexFormat)]) {
        for &(channel, input, format) in defaulted {
            let found = self.defaulted.iter_mut().find(|entry| {
                (entry.channel, entry.input, entry.format) == (channel, input, format)
            });
            if let Some(entry) = found {
                entry.draws += 1;
            } else {
                self.defaulted.push(super::DefaultedChannelDraws {
                    channel,
                    input,
                    format,
                    draws: 1,
                });
            }
        }
    }
}

/// The parts of the state a draw is planned from, borrowed from it together.
pub(super) struct Services<'a> {
    pub(super) runtime: &'a mut ProgramRuntime,
    pub(super) shaders: &'a mut HashMap<ShaderId, ProgramShader>,
    pub(super) programs: &'a mut HashMap<(ShaderId, usize, u32), Arc<ProgramEntry>>,
    pub(super) samplers: &'a mut SamplerCache,
    pub(super) textures: &'a TextureTable,
    pub(super) meshes: &'a MeshTable,
    pub(super) defaults: DefaultBuffers,
    pub(super) tracker: &'a mut Tracker,
    pub(super) constants: &'a mut ConstantsPlan,
    pub(super) missing: &'a mut BTreeSet<String>,
    pub(super) counts: &'a mut DrawCounts,
    /// The reservations of the storage the frame uses, which the frame holds.
    pub(super) shares: &'a mut Vec<Arc<Reservation>>,
}

impl Services<'_> {
    /// Chooses the program of `spec`, packs its constants, binds its textures and samplers.
    pub(super) fn plan_program(&mut self, spec: &DrawSpec<'_>) -> Result<DrawPlan, OffscreenError> {
        let (entry, render_state) = self.choose(spec)?;
        let slots = entry.program.layout().slots().to_vec();
        let mut plan = DrawPlan {
            entry: entry.clone(),
            state: render_state,
            constants: Vec::new(),
            textures: Vec::new(),
            samplers: Vec::new(),
            texture_uses: Vec::new(),
        };
        // Every texture is looked for before anything is bound, so a draw that lacks one changes
        // nothing in the frame.
        Self::require_textures(spec, &entry, &slots)?;
        let mut sampler_states = HashMap::new();
        for slot in &slots {
            match slot.ty {
                wgpu::BindingType::Buffer { .. } => {
                    self.bind_constants(spec, &entry, slot, &mut plan)?;
                }
                wgpu::BindingType::Texture { .. } => {
                    let state = self.bind_texture(spec, &entry, slot, &mut plan)?;
                    if let Some(sampler) = entry.samplers.get(&(slot.set, slot.binding)) {
                        sampler_states.insert((slot.set, *sampler), state);
                    }
                }
                _ => {}
            }
        }
        for slot in &slots {
            if let wgpu::BindingType::Sampler(kind) = slot.ty {
                self.bind_sampler(slot, kind, &sampler_states, &mut plan)?;
            }
        }
        Ok(plan)
    }

    /// Checks that a layer names the texture of every texture slot.
    fn require_textures(
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
            let name = target.name.resolve(&[])?;
            if spec.layers.texture(name).is_none() {
                return Err(OffscreenError::MissingTexture {
                    name: name.to_owned(),
                });
            }
        }
        Ok(())
    }

    fn choose(
        &mut self,
        spec: &DrawSpec<'_>,
    ) -> Result<(Arc<ProgramEntry>, RenderState), OffscreenError> {
        let started = std::time::Instant::now();
        let shader = self
            .shaders
            .get_mut(&spec.shader)
            .ok_or(OffscreenError::UnknownShader(spec.shader))?;
        let resolved = shader.resolve(&spec.selector, &spec.keywords)?;
        let key = (spec.shader, resolved.pass, resolved.blob);
        let entry = if let Some(entry) = self.programs.get(&key) {
            entry.clone()
        } else {
            let entry = Arc::new(self.runtime_program(&resolved.variant, resolved.pass)?);
            self.programs.insert(key, entry.clone());
            self.counts.programs_built += 1;
            entry
        };
        self.counts.program_time += started.elapsed();
        let state = resolve_state(&resolved.state, &spec.layers.values())?;
        self.missing.extend(state.defaulted);
        Ok((entry, state.state))
    }

    fn runtime_program(
        &self,
        variant: &Arc<Variant>,
        pass: usize,
    ) -> Result<ProgramEntry, OffscreenError> {
        let stage = |stage: &crate::unity::module::StageProgram| {
            stage.neutral().map_err(ProgramError::from)
        };
        let program = self
            .runtime
            .graphics_program(&stage(&variant.vertex)?, &stage(&variant.fragment)?)?;
        let mut constants = variant.vertex.constant_buffer_targets()?;
        constants.extend(variant.fragment.constant_buffer_targets()?);
        let mut textures = variant.vertex.texture_targets()?;
        textures.extend(variant.fragment.texture_targets()?);
        let samplers = sampler_pairs(variant);
        Ok(ProgramEntry {
            program,
            variant: variant.clone(),
            pass,
            constants,
            textures,
            samplers,
            inputs: declared_inputs(&variant.vertex),
        })
    }

    fn bind_constants(
        &mut self,
        spec: &DrawSpec<'_>,
        entry: &ProgramEntry,
        slot: &Slot,
        plan: &mut DrawPlan,
    ) -> Result<(), OffscreenError> {
        let wgpu::BindingType::Buffer { ty, .. } = slot.ty else {
            return Ok(());
        };
        let unsupported = |what| OffscreenError::UnsupportedBinding {
            set: slot.set,
            binding: slot.binding,
            what,
        };
        if ty != wgpu::BufferBindingType::Uniform {
            return Err(unsupported("a storage buffer"));
        }
        let target = entry
            .constants
            .iter()
            .find(|t| (t.set, t.binding) == (slot.set, slot.binding))
            .ok_or_else(|| unsupported("a uniform buffer no parameter describes"))?;
        let min_len = usize::try_from(slot.min_size).unwrap_or(usize::MAX);
        let packed = pack_layout(&target.buffer, &[], &spec.layers.values(), min_len)?;
        self.missing.extend(packed.missing);
        let offset = self.constants.push(&packed.bytes);
        plan.constants.push(PendingConstant {
            set: slot.set,
            binding: slot.binding,
            visibility: slot.visibility,
            offset,
            len: packed.bytes.len() as u64,
        });
        Ok(())
    }

    /// Binds the texture a layer names for the slot's parameter and returns the sampler state it
    /// is read with.
    fn bind_texture(
        &mut self,
        spec: &DrawSpec<'_>,
        entry: &ProgramEntry,
        slot: &Slot,
        plan: &mut DrawPlan,
    ) -> Result<Option<SamplerState>, OffscreenError> {
        let wgpu::BindingType::Texture {
            sample_type,
            view_dimension,
            ..
        } = slot.ty
        else {
            return Ok(None);
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
        let input = spec
            .layers
            .texture(name)
            .ok_or_else(|| OffscreenError::MissingTexture {
                name: name.to_owned(),
            })?;
        let (imported, own) = self.import(input.texture)?;
        let (version, mips) = self.tracker.sampled_levels(input.texture, imported)?;
        if mips < imported.mips {
            self.counts.partial_mips += 1;
            self.note_partial_read(spec, entry, (name, mips, imported.mips));
        }
        let view = self
            .tracker
            .whole_view(imported, mips, view_dimension, version)?;
        let features = self.runtime.device().features();
        let (sample_type, state) = read_unfilterable(
            sample_type,
            imported.format,
            features,
            input.sampler.or(own),
        );
        plan.texture_uses
            .push(ResourceUse::read(view.clone(), UseKind::Sampled));
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
        Ok(state)
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

    fn bind_sampler(
        &mut self,
        slot: &Slot,
        kind: wgpu::SamplerBindingType,
        states: &HashMap<(u32, u32), Option<SamplerState>>,
        plan: &mut DrawPlan,
    ) -> Result<(), OffscreenError> {
        if kind == wgpu::SamplerBindingType::Comparison {
            return Err(OffscreenError::UnsupportedBinding {
                set: slot.set,
                binding: slot.binding,
                what: "a comparison sampler",
            });
        }
        let state = states.get(&(slot.set, slot.binding)).copied().flatten();
        plan.samplers.push((
            slot.set,
            SamplerBinding {
                binding: slot.binding,
                visibility: slot.visibility,
                sampler: self.samplers.get(state),
            },
        ));
        Ok(())
    }

    /// Imports a texture of the frame and returns it with its own sampler state.
    pub(super) fn import(
        &mut self,
        id: TextureId,
    ) -> Result<(super::tracker::Imported, Option<SamplerState>), OffscreenError> {
        let info = self.textures.get(id)?;
        for handle in info.handles() {
            self.shares
                .push(self.runtime.store.texture_reservation(handle)?);
        }
        let color = info
            .color
            .map(|handle| self.runtime.gpu_texture(handle))
            .transpose()?;
        let depth = info
            .depth
            .map(|handle| self.runtime.gpu_texture(handle))
            .transpose()?;
        let imported = self.tracker.import(id, color.as_ref(), depth.as_ref())?;
        Ok((imported, info.sampler))
    }

    /// The vertex streams `entry`'s program reads from `mesh`, by its table of source channels and
    /// the shader inputs they feed. An optional channel the mesh lacks reads a constant.
    pub(super) fn plan_vertices(
        &mut self,
        mesh: MeshId,
        entry: &ProgramEntry,
    ) -> Result<VertexPlan, OffscreenError> {
        let info = self.meshes.get(mesh)?;
        self.shares
            .push(self.runtime.store.buffer_reservation(info.buffer)?);
        let buffer = self.runtime.gpu_buffer(info.buffer)?;
        let key =
            self.tracker
                .import_buffer(&buffer, ResourceLifetime::ImportedReadOnly, Some(0))?;
        let mut plan = VertexPlan {
            inputs: Vec::new(),
            views: Vec::new(),
            uses: Vec::new(),
            defaulted: Vec::new(),
        };
        let base = input_base(&entry.variant.channels.channels)?;
        for &[source, target] in &entry.variant.channels.channels {
            let location = u32::try_from(target - base).map_err(|_| OffscreenError::BadMesh {
                mesh,
                reason: format!("the shader input {target} is negative"),
            })?;
            let missing = OffscreenError::MissingChannel {
                mesh,
                channel: source,
                input: target,
            };
            let (handle, key, range, stride, format) = if let Some(channel) = info.channel(source) {
                (
                    info.buffer,
                    key,
                    channel.range.clone(),
                    channel.format.stride(),
                    channel.format.vertex_format(),
                )
            } else {
                let constant = default_for(source, location, &entry.inputs).ok_or(missing)?;
                plan.defaulted.push((source, target, constant.format));
                let (handle, key) = self.import_default(constant.fill)?;
                (handle, key, 0..DefaultBuffers::LEN, 0, constant.format)
            };
            let view = self.tracker.buffer_view(key, range, 0)?;
            plan.uses
                .push(BufferUse::read(view.clone(), BufferUseKind::Vertex));
            plan.views.push(view);
            plan.inputs.push(VertexInput {
                buffer: handle,
                stride,
                step: wgpu::VertexStepMode::Vertex,
                attributes: vec![wgpu::VertexAttribute {
                    format,
                    offset: 0,
                    shader_location: location,
                }],
            });
        }
        Ok(plan)
    }

    /// Imports the buffer of a default's `fill` for reading and has the frame hold it.
    fn import_default(
        &mut self,
        fill: Fill,
    ) -> Result<(BufferHandle, AllocationKey), OffscreenError> {
        let handle = self.defaults.buffer(fill);
        self.shares
            .push(self.runtime.store.buffer_reservation(handle)?);
        let buffer = self.runtime.gpu_buffer(handle)?;
        let key =
            self.tracker
                .import_buffer(&buffer, ResourceLifetime::ImportedReadOnly, Some(0))?;
        Ok((handle, key))
    }

    /// The target signature of a pass over a texture's color and depth.
    pub(super) fn signature(&self, id: TextureId) -> Result<TargetSignature, OffscreenError> {
        let info = self.textures.get(id)?;
        let Some(depth) = info.depth_format else {
            return Ok(TargetSignature::new(&[info.format], None, 1)?);
        };
        if info.color.is_none() {
            return Ok(TargetSignature::depth_only(depth)?);
        }
        Ok(TargetSignature::new(&[info.format], Some(depth), 1)?)
    }
}

/// How a texture the device cannot filter is read by a pass written for a filtered one.
///
/// A pass declares its textures as filterable floats. A format the device cannot filter (32-bit
/// floats without `FLOAT32_FILTERABLE`, and depth) is bound as a non-filterable float instead and
/// its sampler reads the nearest texel, since a filtering sampler cannot read it. The wrap modes
/// are kept; a texture with no sampler state gets the fixed sampler's clamp to the edge.
fn read_unfilterable(
    declared: wgpu::TextureSampleType,
    format: wgpu::TextureFormat,
    features: wgpu::Features,
    state: Option<SamplerState>,
) -> (wgpu::TextureSampleType, Option<SamplerState>) {
    use wgpu::TextureSampleType as Type;
    let filterable = Type::Float { filterable: true };
    let actual = format.sample_type(Some(wgpu::TextureAspect::All), Some(features));
    if declared != filterable || actual.is_none() || actual == Some(filterable) {
        return (declared, state);
    }
    let point = SamplerState {
        filter: Filter::Point,
        ..state.unwrap_or(SamplerState {
            wrap_u: Wrap::Clamp,
            wrap_v: Wrap::Clamp,
            filter: Filter::Point,
        })
    };
    (Type::Float { filterable: false }, Some(point))
}

/// The prepared sampler binding each prepared texture binding is read with: the binding the
/// preparation split from a combined sampler, or the separate sampler the texture's parameter
/// names.
fn sampler_pairs(variant: &Variant) -> HashMap<(u32, u32), u32> {
    let tables = [&variant.vertex, &variant.fragment].map(|stage| &stage.module().bindings);
    let prepared = |set: u32, unity: u32| {
        tables
            .iter()
            .flat_map(|table| table.iter())
            .find(|b| b.set == set && b.unity_binding == unity)
    };
    let mut pairs: HashMap<(u32, u32), u32> = tables
        .iter()
        .flat_map(|table| table.iter())
        .filter_map(|b| b.sampler_binding.map(|s| ((b.set, b.binding), s)))
        .collect();
    for parameter in &variant.parameters.bindings {
        let ResourceKind::Texture {
            sampler: Some(sampler),
            ..
        } = parameter.kind
        else {
            continue;
        };
        let texture = prepared(
            u32::from(parameter.slot.set),
            u32::from(parameter.slot.binding),
        );
        let sampler = prepared(u32::from(sampler.set), u32::from(sampler.binding));
        if let (Some(texture), Some(sampler)) = (texture, sampler) {
            pairs
                .entry((texture.set, texture.binding))
                .or_insert(sampler.binding);
        }
    }
    pairs
}
