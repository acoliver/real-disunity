//! Planning one draw: choosing the program, packing its constants, binding its textures and
//! samplers, and arranging the vertex streams the program reads. Nothing here touches the
//! constants buffer, whose size depends on every draw of the frame; a [`DrawPlan`] holds the
//! offsets its constants will have in it.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use worldrunner_render_post::contracts::{
    BufferUse, BufferUseKind, BufferView, ResourceLifetime, ResourceUse, SamplerBinding, Semantic,
    TextureBinding, TextureBindingKind, UseKind,
};

use crate::budget::Reservation;
use crate::constants::pack_layout;
use crate::draw::VertexInput;
use crate::ids::{MeshId, ShaderId, TextureId};
use crate::layers::Layers;
use crate::layout::Slot;
use crate::packet::TargetSignature;
use crate::program::GraphicsProgram;
use crate::runtime::ProgramRuntime;
use crate::sampler::{Filter, SamplerCache, SamplerState, Wrap};
use crate::shader::{PassSelector, ProgramShader, Variant};
use crate::state::{RenderState, resolve_state};
use crate::targets::{ConstantBufferTarget, TextureTarget};
use worldrunner_unity_serialized::shader_parameters::ResourceKind;

use super::error::OffscreenError;
use super::meshes::MeshTable;
use super::textures::TextureTable;
use super::tracker::Tracker;

/// A graphics program with the parts of its parameters every draw of it needs.
pub(super) struct ProgramEntry {
    pub(super) program: GraphicsProgram,
    pub(super) variant: Arc<Variant>,
    constants: Vec<ConstantBufferTarget>,
    textures: Vec<TextureTarget>,
    /// The sampler binding split from the texture at a set and binding.
    samplers: HashMap<(u32, u32), u32>,
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

/// The vertex streams a program reads from a mesh.
pub(super) struct VertexPlan {
    pub(super) inputs: Vec<VertexInput>,
    pub(super) views: Vec<BufferView>,
    pub(super) uses: Vec<BufferUse>,
}

/// What chooses a draw's program and feeds its parameters.
pub(super) struct DrawSpec<'a> {
    pub(super) shader: ShaderId,
    pub(super) keywords: Vec<&'a str>,
    pub(super) selector: PassSelector,
    pub(super) layers: Layers<'a>,
}

/// The parts of the state a draw is planned from, borrowed from it together.
pub(super) struct Services<'a> {
    pub(super) runtime: &'a mut ProgramRuntime,
    pub(super) shaders: &'a mut HashMap<ShaderId, ProgramShader>,
    pub(super) programs: &'a mut HashMap<(ShaderId, usize, u32), Arc<ProgramEntry>>,
    pub(super) samplers: &'a mut SamplerCache,
    pub(super) textures: &'a TextureTable,
    pub(super) meshes: &'a MeshTable,
    pub(super) tracker: &'a mut Tracker,
    pub(super) constants: &'a mut ConstantsPlan,
    pub(super) missing: &'a mut BTreeSet<String>,
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

    fn choose(
        &mut self,
        spec: &DrawSpec<'_>,
    ) -> Result<(Arc<ProgramEntry>, RenderState), OffscreenError> {
        let shader = self
            .shaders
            .get_mut(&spec.shader)
            .ok_or(OffscreenError::UnknownShader(spec.shader))?;
        let resolved = shader.resolve(&spec.selector, &spec.keywords)?;
        let key = (spec.shader, resolved.pass, resolved.blob);
        let entry = if let Some(entry) = self.programs.get(&key) {
            entry.clone()
        } else {
            let entry = Arc::new(self.runtime_program(&resolved.variant)?);
            self.programs.insert(key, entry.clone());
            entry
        };
        let state = resolve_state(&resolved.state, &spec.layers.values())?;
        self.missing.extend(state.defaulted);
        Ok((entry, state.state))
    }

    fn runtime_program(&self, variant: &Arc<Variant>) -> Result<ProgramEntry, OffscreenError> {
        let program = self
            .runtime
            .graphics_program(&variant.vertex, &variant.fragment)?;
        let mut constants = variant.vertex.constant_buffer_targets()?;
        constants.extend(variant.fragment.constant_buffer_targets()?);
        let mut textures = variant.vertex.texture_targets()?;
        textures.extend(variant.fragment.texture_targets()?);
        let samplers = sampler_pairs(variant);
        Ok(ProgramEntry {
            program,
            variant: variant.clone(),
            constants,
            textures,
            samplers,
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
        let version = self.tracker.uniform_version(input.texture, imported)?;
        let view = self.tracker.whole_view(imported, view_dimension, version)?;
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
        let color = self.runtime.gpu_texture(info.color)?;
        let depth = info
            .depth
            .map(|handle| self.runtime.gpu_texture(handle))
            .transpose()?;
        let imported = self.tracker.import(id, &color, depth.as_ref())?;
        Ok((imported, info.sampler))
    }

    /// The vertex streams `variant` reads from `mesh`, by its table of source channels and the
    /// shader inputs they feed.
    pub(super) fn plan_vertices(
        &mut self,
        mesh: MeshId,
        variant: &Variant,
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
        };
        for &[source, target] in &variant.channels.channels {
            let channel = info.channel(source).ok_or(OffscreenError::MissingChannel {
                mesh,
                channel: source,
                input: target,
            })?;
            let location = u32::try_from(target).map_err(|_| OffscreenError::BadMesh {
                mesh,
                reason: format!("the shader input {target} is negative"),
            })?;
            let view = self.tracker.buffer_view(key, channel.range.clone(), 0)?;
            plan.uses
                .push(BufferUse::read(view.clone(), BufferUseKind::Vertex));
            plan.views.push(view);
            plan.inputs.push(VertexInput {
                buffer: info.buffer,
                stride: channel.format.stride(),
                step: wgpu::VertexStepMode::Vertex,
                attributes: vec![wgpu::VertexAttribute {
                    format: channel.format.vertex_format(),
                    offset: 0,
                    shader_location: location,
                }],
            });
        }
        Ok(plan)
    }

    /// The target signature of a pass over a texture's color and depth.
    pub(super) fn signature(&self, id: TextureId) -> Result<TargetSignature, OffscreenError> {
        let info = self.textures.get(id)?;
        Ok(TargetSignature::new(&[info.format], info.depth_format, 1)?)
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
