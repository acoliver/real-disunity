//! Planning one draw: choosing the program, packing its constants, binding its textures and
//! samplers, and arranging the vertex streams the program reads. Nothing here touches the
//! constants buffers, whose sizes depend on every draw of the frame; a [`DrawPlan`] holds the
//! chunk and offset its constants will have.

use std::collections::BTreeSet;
use std::collections::HashMap;
use std::sync::Arc;

use worldrunner_render_post::contracts::AllocationKey;
use worldrunner_render_post::contracts::BufferUse;
use worldrunner_render_post::contracts::BufferUseKind;
use worldrunner_render_post::contracts::BufferView;
use worldrunner_render_post::contracts::ResourceLifetime;
use worldrunner_render_post::contracts::ResourceUse;
use worldrunner_render_post::contracts::SamplerBinding;
use worldrunner_render_post::contracts::TextureBinding;

use crate::budget::Reservation;
use crate::draw::VertexInput;
use crate::error::ProgramError;
use crate::ids::MeshId;
use crate::ids::ShaderId;
use crate::ids::TextureId;
use crate::layers::Layers;
use crate::layout::Slot;
use crate::packet::TargetSignature;
use crate::program::GraphicsProgram;
use crate::resources::BufferHandle;
use crate::runtime::ProgramRuntime;
use crate::sampler::SamplerCache;
use crate::sampler::SamplerState;
use crate::unity::constants::pack_layout;
use crate::unity::shader::PassSelector;
use crate::unity::shader::ProgramShader;
use crate::unity::shader::Variant;
use crate::unity::state::RenderState;
use crate::unity::state::resolve_state;
use crate::unity::targets::BufferTarget;
use crate::unity::targets::ConstantBufferTarget;
use crate::unity::targets::TextureTarget;
use worldrunner_unity_serialized::shader_parameters::ResourceKind;

use super::constants_plan::ConstantsPlan;
use super::counts::DrawCounts;
use super::defaults::DeclaredInputs;
use super::defaults::DefaultBuffers;
use super::defaults::Fill;
use super::defaults::declared_inputs;
use super::defaults::default_for;
use super::depth_range;
use super::dots::DotsDraw;
use super::error::OffscreenError;
use super::frame::UndeclaredTextures;
use super::meshes::MeshTable;
use super::probe::CONSTANT_ENTRY;
use super::probe::CONSTANT_FRAGMENT;
use super::probe::DrawFacts;
use super::sampling::{self};
use super::storage_buffers::PendingStorage;
use super::storage_buffers::check_stage_limits;
use super::storage_buffers::require_buffers;
use super::texel_size;
use super::textures::TextureTable;
use super::tracker::Tracker;

/// A graphics program with the parts of its parameters every draw of it needs.
pub(super) struct ProgramEntry {
    pub(super) program: GraphicsProgram,
    pub(super) variant: Arc<Variant>,
    /// The pass of the first sub-shader the program is.
    pub(super) pass: usize,
    pub(super) constants: Vec<ConstantBufferTarget>,
    pub(super) textures: Vec<TextureTarget>,
    /// The structured buffers the stages declare, by the property that names each.
    pub(super) buffers: Vec<BufferTarget>,
    /// The sampler binding split from the texture at a set and binding.
    pub(super) samplers: HashMap<(u32, u32), u32>,
    /// The names the shader gives its comparison samplers, by set and binding.
    pub(super) comparison_names: HashMap<(u32, u32), String>,
    /// The vertex inputs the vertex stage declares, by location.
    pub(super) inputs: DeclaredInputs,
    /// The color locations the fragment stage writes.
    pub(super) color_outputs: Vec<u32>,
}

impl ProgramEntry {
    /// How many vertex inputs the vertex stage declares.
    pub(super) fn vertex_input_count(&self) -> usize {
        self.inputs.count()
    }

    /// The property that names the structured buffer `slot` binds, if a parameter does.
    pub(super) fn buffer_name(&self, slot: &Slot) -> Result<Option<&str>, OffscreenError> {
        let found = self
            .buffers
            .iter()
            .find(|t| (t.set, t.binding) == (slot.set, slot.binding));
        Ok(found.map(|t| t.name.resolve(&[])).transpose()?)
    }
}

/// A constant buffer binding whose bytes are in the constants plan at `offset` of `chunk`.
pub(super) struct PendingConstant {
    pub(super) set: u32,
    pub(super) binding: u32,
    pub(super) visibility: wgpu::ShaderStages,
    pub(super) chunk: usize,
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
    /// The read-only structured buffers the draw binds.
    pub(super) storage: Vec<PendingStorage>,
    pub(super) texture_uses: Vec<ResourceUse>,
    /// What a probed camera render's look needs, noted only while the services are probing.
    pub(super) facts: Option<DrawFacts>,
    /// The texture properties the draw reads a stand-in for, with the reason the layers gave, or
    /// `texture_binding::UNDECLARED_REASON` for a slot no layer explains. A draw left out reports none.
    pub(super) stand_ins: Vec<(String, String)>,
    /// The constant buffer members and state properties no layer supplied. They count as the
    /// frame's only once the draw is kept (see [`Services::keep_unset`]).
    pub(super) unset: BTreeSet<String>,
}

/// The program and render state `choose` picked for a draw, and the state properties no layer
/// supplied.
pub(super) struct Chosen {
    pub(super) entry: Arc<ProgramEntry>,
    pub(super) state: RenderState,
    unset: BTreeSet<String>,
}

/// Set in the blob number of the key of a program whose fragment stage writes a constant colour,
/// which no shader's blob numbers reach.
const CONSTANT_FRAGMENT_KEY: u32 = 1 << 31;

/// The fragment stage of [`StateVariant::constant_fragment`](super::StateVariant): it reads
/// nothing and binds nothing.
fn constant_fragment() -> Result<crate::module::StageProgram, ProgramError> {
    let module = crate::module::StageModule {
        source: crate::module::StageSource::Wgsl(Arc::from(CONSTANT_FRAGMENT)),
        entry_point: CONSTANT_ENTRY.to_owned(),
        kind: crate::module::StageKind::Fragment,
    };
    Ok(crate::module::StageProgram::new(
        module,
        crate::module::StageDeclarations::default(),
    )?)
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
    /// The instances of a raw buffer the draw makes, when it makes them.
    pub(super) dots: Option<DotsDraw<'a>>,
}

/// The parts of the state a draw is planned from, borrowed from it together.
pub(super) struct Services<'a> {
    pub(super) runtime: &'a mut ProgramRuntime,
    pub(super) shaders: &'a mut HashMap<ShaderId, ProgramShader>,
    pub(super) programs: &'a mut HashMap<(ShaderId, usize, u32), Arc<ProgramEntry>>,
    pub(super) samplers: &'a mut SamplerCache,
    /// The program a blit without a material draws with, made on first use.
    pub(super) resample: &'a mut Option<GraphicsProgram>,
    pub(super) textures: &'a TextureTable,
    pub(super) meshes: &'a MeshTable,
    /// The frame's textures for a slot no layer sets and the shader does not declare.
    pub(super) undeclared_textures: UndeclaredTextures,
    pub(super) defaults: DefaultBuffers,
    pub(super) tracker: &'a mut Tracker,
    pub(super) constants: &'a mut ConstantsPlan,
    pub(super) missing: &'a mut BTreeSet<String>,
    pub(super) counts: &'a mut DrawCounts,
    /// The reservations of the storage the frame uses, which the frame holds.
    pub(super) shares: &'a mut Vec<Arc<Reservation>>,
    /// Whether the draws planned now are noted for a camera render's probe.
    pub(super) probing: bool,
    /// Whether the draws planned now use a fragment stage that writes a constant colour, whatever
    /// the pass's own does.
    pub(super) constant_fragment: bool,
}

impl Services<'_> {
    /// Chooses the program of `spec`, packs its constants, binds its textures and samplers.
    pub(super) fn plan_program(&mut self, spec: &DrawSpec<'_>) -> Result<DrawPlan, OffscreenError> {
        let chosen = self.choose(spec)?;
        self.plan_chosen(spec, chosen)
    }

    /// Adds the names a kept draw left unset to those the frame reports. A draw left out reports
    /// none, whichever step refused it.
    pub(super) fn keep_unset(&mut self, plan: &DrawPlan) {
        self.missing.extend(plan.unset.iter().cloned());
    }

    /// Packs the constants of the program `choose` picked for `spec` and binds its textures and
    /// samplers.
    pub(super) fn plan_chosen(
        &mut self,
        spec: &DrawSpec<'_>,
        chosen: Chosen,
    ) -> Result<DrawPlan, OffscreenError> {
        let Chosen {
            entry,
            state: render_state,
            unset,
        } = chosen;
        let slots = entry.program.layout().slots().to_vec();
        let mut plan = DrawPlan {
            entry,
            state: render_state,
            constants: Vec::new(),
            textures: Vec::new(),
            samplers: Vec::new(),
            storage: Vec::new(),
            texture_uses: Vec::new(),
            facts: self.probing.then(DrawFacts::default),
            stand_ins: Vec::new(),
            unset,
        };
        let entry = Arc::clone(&plan.entry);
        // Every texture and structured buffer is looked for before anything is bound, so a draw
        // that lacks one changes nothing in the frame.
        self.require_textures(spec, &entry, &slots)?;
        let storage = require_buffers(spec, &entry, &slots, self.defaults)?;
        let mut sampler_states = HashMap::new();
        let mut any_unfiltered = false;
        for slot in &slots {
            match slot.ty {
                wgpu::BindingType::Buffer { .. } => {
                    self.bind_constants(spec, &entry, slot, &mut plan)?;
                }
                wgpu::BindingType::Texture { .. } => {
                    let read = self.bind_texture(spec, &entry, slot, &mut plan)?;
                    any_unfiltered |= read.unfiltered;
                    if let Some(sampler) = entry.samplers.get(&(slot.set, slot.binding)) {
                        let key = (slot.set, *sampler);
                        sampler_states.insert(key, sampling::merge(sampler_states.get(&key), read));
                    }
                }
                _ => {}
            }
        }
        for slot in &slots {
            if let wgpu::BindingType::Sampler(kind) = slot.ty {
                let states = (&sampler_states, any_unfiltered);
                self.bind_sampler(&entry, slot, kind, states, &mut plan);
            }
        }
        for input in &storage {
            self.bind_storage(input, &mut plan)?;
        }
        Ok(plan)
    }

    /// The program and render state of the pass and variant `spec` selects.
    pub(super) fn choose(&mut self, spec: &DrawSpec<'_>) -> Result<Chosen, OffscreenError> {
        let started = std::time::Instant::now();
        let shader = self
            .shaders
            .get_mut(&spec.shader)
            .ok_or(OffscreenError::UnknownShader(spec.shader))?;
        let resolved = shader.resolve(&spec.selector, &spec.keywords)?;
        let blob = if self.constant_fragment {
            resolved.blob | CONSTANT_FRAGMENT_KEY
        } else {
            resolved.blob
        };
        let key = (spec.shader, resolved.pass, blob);
        let entry = if let Some(entry) = self.programs.get(&key) {
            entry.clone()
        } else {
            let entry = Arc::new(self.runtime_program(&resolved.variant, resolved.pass)?);
            self.programs.insert(key, entry.clone());
            self.counts.programs_built += 1;
            entry
        };
        self.counts.program_time += started.elapsed();
        let mut state = resolve_state(&resolved.state, &spec.layers.values())?;
        state.state.depth_compare = depth_range::depth_test(state.state.depth_compare);
        Ok(Chosen {
            entry,
            state: state.state,
            unset: state.defaulted.into_iter().collect(),
        })
    }

    fn runtime_program(
        &self,
        variant: &Arc<Variant>,
        pass: usize,
    ) -> Result<ProgramEntry, OffscreenError> {
        // The device's limit is checked before the program is made, so a refusal names the buffer.
        let vertex_buffers = variant.vertex.buffer_targets()?;
        let fragment_buffers = variant.fragment.buffer_targets()?;
        check_stage_limits(
            self.runtime
                .device()
                .limits()
                .max_storage_buffers_per_shader_stage,
            [("vertex", &vertex_buffers), ("fragment", &fragment_buffers)],
        )?;
        let stage = |stage: &crate::unity::module::StageProgram| {
            stage.neutral().map_err(ProgramError::from)
        };
        let fragment = if self.constant_fragment {
            constant_fragment()?
        } else {
            stage(&variant.fragment)?
        };
        let color_outputs = fragment.color_outputs();
        let program = self
            .runtime
            .graphics_program(&stage(&variant.vertex)?, &fragment)?;
        let mut constants = variant.vertex.constant_buffer_targets()?;
        constants.extend(variant.fragment.constant_buffer_targets()?);
        let mut textures = variant.vertex.texture_targets()?;
        textures.extend(variant.fragment.texture_targets()?);
        let samplers = sampler_pairs(variant);
        let comparison_names = comparison_names(variant);
        Ok(ProgramEntry {
            program,
            variant: variant.clone(),
            pass,
            constants,
            textures,
            buffers: [vertex_buffers, fragment_buffers].concat(),
            samplers,
            comparison_names,
            inputs: declared_inputs(&variant.vertex),
            color_outputs,
        })
    }

    fn bind_constants(
        &mut self,
        spec: &DrawSpec<'_>,
        entry: &ProgramEntry,
        slot: &Slot,
        plan: &mut DrawPlan,
    ) -> Result<(), OffscreenError> {
        // A storage buffer is not packed from the layers' values: `require_buffers` found its
        // buffer, and `bind_storage` binds it.
        let wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            ..
        } = slot.ty
        else {
            return Ok(());
        };
        let target = entry
            .constants
            .iter()
            .find(|t| (t.set, t.binding) == (slot.set, slot.binding))
            .ok_or(OffscreenError::UnsupportedBinding {
                set: slot.set,
                binding: slot.binding,
                what: "a uniform buffer no parameter describes",
            })?;
        let min_len = usize::try_from(slot.min_size).unwrap_or(usize::MAX);
        let texel_sizes = texel_size::texel_sizes(
            self.textures,
            self.undeclared_textures,
            &spec.layers,
            &target.buffer,
        );
        let mut values = spec.layers.values();
        values.push(&texel_sizes);
        let dots = spec
            .dots
            .map(|dots| dots.values(&target.buffer, &values))
            .transpose()?;
        // The batch's own values come first: they are what the draw's variant is for.
        if let Some(set) = &dots {
            values.insert(0, set);
        }
        let packed = pack_layout(&target.buffer, &[], &values, min_len)?;
        if let Some(facts) = plan.facts.as_mut() {
            facts.note_constants(&target.buffer, &packed, slot.visibility, &values);
        }
        plan.unset.extend(packed.missing);
        let placed = self.constants.push(&packed.bytes)?;
        plan.constants.push(PendingConstant {
            set: slot.set,
            binding: slot.binding,
            visibility: slot.visibility,
            chunk: placed.chunk,
            offset: placed.offset,
            len: packed.bytes.len() as u64,
        });
        Ok(())
    }

    /// Imports a texture of the frame and returns it with its own sampler state.
    pub(super) fn import(
        &mut self,
        id: TextureId,
    ) -> Result<(super::tracker::Imported, Option<SamplerState>), OffscreenError> {
        let info = self.textures.get(id)?;
        // A render volume is deep and only kernels touch it. An uploaded 3D source of any depth is
        // a single cell, which a draw samples through a 3D view.
        if info.dimension.is_volume() && info.spec.is_some() {
            return Err(OffscreenError::VolumeInGraph(id));
        }
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
            return Ok(TargetSignature::depth_attachment(depth, 1)?);
        }
        Ok(TargetSignature::attachments(
            &[info.format],
            Some(depth),
            1,
        )?)
    }
}

/// The names the variant's stages give their comparison samplers, by set and binding.
fn comparison_names(variant: &Variant) -> HashMap<(u32, u32), String> {
    [&variant.vertex, &variant.fragment]
        .into_iter()
        .flat_map(|stage| stage.resources().iter())
        .filter(|resource| {
            matches!(
                resource.ty,
                wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison)
            ) && !resource.name.is_empty()
        })
        .map(|resource| ((resource.set, resource.binding), resource.name.clone()))
        .collect()
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
