//! Turning planned steps into typed operations once the constants buffer exists: creating the
//! bind groups and the checked draws.

use std::collections::HashMap;

use worldrunner_render_post::contracts::{
    AllocationKey, BufferBinding, BufferBindingKind, BufferUse, BufferUseKind, CheckedBindings,
    CheckedDraw, ContractError, FullscreenVertices, RasterPass, TypedOperation, TypedWork,
    VertexStream,
};

use crate::draw::VertexInput;
use crate::packet::{PipelineRequest, TargetSignature, VertexLayout, with_pipeline_descriptor};
use crate::runtime::ProgramRuntime;
use crate::state::{ColorState, RenderState};

use super::error::OffscreenError;
use super::plan::DrawPlan;
use super::steps::{BlitStep, CameraDraw, Step, push_buffer_use};
use super::tracker::Tracker;

/// The state turning steps into operations needs, borrowed from the offscreen state.
pub(super) struct Finish<'a> {
    pub(super) runtime: &'a mut ProgramRuntime,
    pub(super) tracker: &'a mut Tracker,
    pub(super) draws: &'a mut HashMap<blake3::Hash, CheckedDraw>,
    /// The constants buffer in the registry, when any draw has constants.
    pub(super) constants: Option<AllocationKey>,
}

impl Finish<'_> {
    pub(super) fn operations(
        &mut self,
        steps: Vec<Step>,
    ) -> Result<Vec<TypedOperation>, OffscreenError> {
        steps
            .into_iter()
            .map(|step| match step {
                Step::Ready(operation) => Ok(operation),
                Step::Camera { id, pass, draws } => self.camera(id, pass, &draws),
                Step::Blit(blit) => self.blit(blit),
            })
            .collect()
    }

    fn camera(
        &mut self,
        id: worldrunner_render_post::contracts::RasterId,
        pass: RasterPass,
        draws: &[CameraDraw],
    ) -> Result<TypedOperation, OffscreenError> {
        let mut buffers = Vec::new();
        let mut checked = Vec::new();
        for draw in draws {
            let (bindings, uses) = self.bind(&draw.plan)?;
            checked.push(self.checked_draw(draw, bindings)?);
            for usage in uses.into_iter().chain(draw.vertices.uses.iter().cloned()) {
                push_buffer_use(&mut buffers, usage);
            }
            if let Some(usage) = &draw.index_use {
                push_buffer_use(&mut buffers, usage.clone());
            }
        }
        let mut operation = TypedOperation::new(
            id,
            TypedWork::Draws {
                pass,
                draws: checked,
            },
        );
        operation.buffers = buffers;
        Ok(operation)
    }

    /// The bind group of each descriptor set of the plan's program, and the uniform reads they
    /// make of the constants buffer.
    fn bind(
        &mut self,
        plan: &DrawPlan,
    ) -> Result<(Vec<CheckedBindings>, Vec<BufferUse>), OffscreenError> {
        let slots = plan.entry.program.layout().slots();
        let sets = slots.iter().map(|slot| slot.set + 1).max().unwrap_or(0);
        let device = self.runtime.device().clone();
        let mut groups = Vec::new();
        let mut uses = Vec::new();
        for set in 0..sets {
            let textures: Vec<_> = plan
                .textures
                .iter()
                .filter(|(s, _)| *s == set)
                .map(|(_, binding)| binding.clone())
                .collect();
            let samplers: Vec<_> = plan
                .samplers
                .iter()
                .filter(|(s, _)| *s == set)
                .map(|(_, binding)| binding.clone())
                .collect();
            let mut buffers = Vec::new();
            for constant in plan.constants.iter().filter(|c| c.set == set) {
                let key = self.constants.ok_or(ContractError::Pipeline)?;
                let view = self.tracker.buffer_view(
                    key,
                    constant.offset..constant.offset + constant.len,
                    1,
                )?;
                uses.push(BufferUse::read(view.clone(), BufferUseKind::Uniform));
                buffers.push(BufferBinding {
                    binding: constant.binding,
                    visibility: constant.visibility,
                    view,
                    kind: BufferBindingKind::Uniform,
                    dynamic_offset: None,
                });
            }
            let registry = &self.tracker.registry;
            let built = self.runtime.guarded(|| {
                CheckedBindings::build_with_resources(
                    &device, registry, &textures, &buffers, &samplers,
                )
            })?;
            groups.push(built?);
        }
        Ok((groups, uses))
    }

    /// A draw of the camera's program: a rebinding of the pipeline an earlier draw made when its
    /// layouts agree, else a new pipeline.
    fn checked_draw(
        &mut self,
        draw: &CameraDraw,
        bindings: Vec<CheckedBindings>,
    ) -> Result<CheckedDraw, OffscreenError> {
        let program = &draw.plan.entry.program;
        let state = draw.plan.state.neutral(draw.front_face);
        let layouts = vertex_layouts(&draw.vertices.inputs);
        let colors = colors_of(draw.signature, &state)?;
        let request = PipelineRequest {
            program,
            state: &state,
            layouts: &layouts,
            signature: draw.signature,
            colors: &colors,
        };
        let key = request.key();
        if let Some(prototype) = self.draws.get(&key) {
            match prototype.rebind(bindings.clone(), draw.geometry.clone()) {
                Ok(rebound) => return Ok(rebound),
                Err(ContractError::Pipeline) => {}
                Err(error) => return Err(error.into()),
            }
        }
        let modules = self.runtime.program_modules(program)?;
        let device = self.runtime.device().clone();
        let created =
            with_pipeline_descriptor(&request, &modules, &program.gpu.pipeline, |descriptor| {
                self.runtime.guarded(|| {
                    CheckedDraw::with_geometry(&device, descriptor, bindings, draw.geometry.clone())
                })
            })??;
        self.draws.insert(key, created.clone());
        Ok(created)
    }

    /// The pipeline of a blit, whose layout is made of the bind groups the blit binds.
    fn blit_pipeline(
        &mut self,
        blit: &BlitStep,
        bindings: &[CheckedBindings],
    ) -> Result<wgpu::RenderPipeline, OffscreenError> {
        let device = self.runtime.device().clone();
        let layouts: Vec<_> = bindings.iter().map(|group| Some(group.layout())).collect();
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            bind_group_layouts: &layouts,
            ..Default::default()
        });
        let program = &blit.plan.entry.program;
        let color = blit.colors[0]
            .as_ref()
            .map_or(wgpu::TextureFormat::Rgba8Unorm, |c| c.view.format());
        let signature = TargetSignature::new(&[color], None, 1)?;
        let state = blit.plan.state.neutral(wgpu::FrontFace::Ccw);
        let layouts = vertex_layouts(&blit.vertices.inputs);
        let colors = colors_of(signature, &state)?;
        let request = PipelineRequest {
            program,
            state: &state,
            layouts: &layouts,
            signature,
            colors: &colors,
        };
        let modules = self.runtime.program_modules(program)?;
        Ok(with_pipeline_descriptor(
            &request,
            &modules,
            &layout,
            |descriptor| {
                self.runtime
                    .guarded(|| device.create_render_pipeline(descriptor))
            },
        )?)
    }

    fn blit(&mut self, blit: BlitStep) -> Result<TypedOperation, OffscreenError> {
        let (bindings, mut buffers) = self.bind(&blit.plan)?;
        let pipeline = self.blit_pipeline(&blit, &bindings)?;
        let streams = blit
            .vertices
            .views
            .iter()
            .zip(&blit.vertices.inputs)
            .map(|(view, input)| VertexStream {
                view: view.clone(),
                stride: input.stride,
                extent: input.attributes[0].format.size(),
            })
            .collect();
        for usage in &blit.vertices.uses {
            push_buffer_use(&mut buffers, usage.clone());
        }
        let pass = RasterPass {
            id: blit.id,
            colors: blit.colors,
            depth: None,
            uses: blit.plan.texture_uses.clone(),
            bindings,
        };
        let mut operation = TypedOperation::new(
            blit.id,
            TypedWork::Fullscreen {
                pass,
                pipeline,
                vertices: Some(FullscreenVertices {
                    streams,
                    count: blit.count,
                }),
            },
        );
        operation.buffers = buffers;
        Ok(operation)
    }
}

/// How the pipeline reads each vertex buffer of a draw.
fn vertex_layouts(inputs: &[VertexInput]) -> Vec<VertexLayout<'_>> {
    inputs
        .iter()
        .map(|input| VertexLayout {
            stride: input.stride,
            step: input.step,
            attributes: &input.attributes,
        })
        .collect()
}

/// The colour state of each colour attachment of `signature`: one for a colour pass, none for a
/// depth-only pass.
fn colors_of(
    signature: TargetSignature,
    state: &RenderState,
) -> Result<Vec<ColorState>, OffscreenError> {
    if signature.color().is_none() {
        return Ok(Vec::new());
    }
    Ok(vec![state.check_drawable()?])
}
