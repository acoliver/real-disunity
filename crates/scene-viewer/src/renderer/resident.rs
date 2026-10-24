use worldrunner_render_plan::SceneEffects;
use worldrunner_render_scene::{
    SceneView, device::GpuContext, draw::SceneGpu, frame::FrameUniform, renderer::ScenePipelines,
    shadows::ShadowedScene,
};

use super::SceneSource;
use crate::{
    LiveFrame, ViewerError,
    presentation::{COLOR, Targets},
};

pub(super) enum Resident {
    Plain(Box<PlainScene>),
    Shadowed(Box<ShadowedScene>),
}

pub(super) struct PlainScene {
    gpu: SceneGpu,
    pipelines: ScenePipelines,
    effects: Option<SceneEffects>,
    clear: wgpu::Color,
}

impl Resident {
    pub fn new(context: &GpuContext, source: SceneSource<'_>) -> Result<Self, ViewerError> {
        if let SceneSource::Static {
            scene,
            shadows: Some(settings),
        } = source
        {
            return Ok(Self::Shadowed(Box::new(ShadowedScene::new(
                context, scene, settings,
            )?)));
        }
        let pipelines = ScenePipelines::new(context.device(), COLOR);
        let (gpu, effects) = match source {
            SceneSource::Static { scene, .. } => (
                SceneGpu::upload(context.device(), context.queue(), scene, &pipelines)?,
                Some(scene.effects().clone()),
            ),
            SceneSource::Live => (
                SceneGpu::empty(context.device(), context.queue(), &pipelines),
                None,
            ),
        };
        Ok(Self::Plain(Box::new(PlainScene {
            gpu,
            pipelines,
            effects,
            clear: wgpu::Color::BLACK,
        })))
    }

    pub fn is_live(&self) -> bool {
        matches!(self, Self::Plain(plain) if plain.effects.is_none())
    }

    pub fn prepare_live(
        &mut self,
        context: &GpuContext,
        frame: &LiveFrame,
    ) -> Result<(), ViewerError> {
        let Self::Plain(plain) = self else {
            return Err(ViewerError::UnsupportedScene("dynamic shadow host"));
        };
        if let Some(textures) = &frame.textures {
            plain.gpu.set_dynamic_textures(
                context.device(),
                context.queue(),
                &plain.pipelines,
                textures,
            )?;
        }
        plain.gpu.set_dynamic_quads(
            context.device(),
            context.queue(),
            &plain.pipelines,
            &frame.quads,
        )?;
        let [r, g, b, a] = frame.clear;
        plain.clear = wgpu::Color { r, g, b, a };
        Ok(())
    }

    pub fn render(
        &mut self,
        context: &GpuContext,
        view: &SceneView,
        targets: &Targets,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<(), ViewerError> {
        match self {
            Self::Shadowed(renderer) => {
                renderer.render(context, view, &targets.color, &targets.depth)?;
            }
            Self::Plain(plain) => plain.render(context, view, targets, encoder)?,
        }
        Ok(())
    }
}

impl PlainScene {
    fn render(
        &self,
        context: &GpuContext,
        view: &SceneView,
        targets: &Targets,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<(), ViewerError> {
        let uniform = match &self.effects {
            Some(effects) => FrameUniform::new(view, effects)?,
            None => FrameUniform::unlit(view),
        };
        self.gpu.set_frame(context.queue(), uniform);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("viewer scene"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &targets.color,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(self.clear),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &targets.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        self.gpu.draw(&mut pass, &self.pipelines);
        Ok(())
    }
}
