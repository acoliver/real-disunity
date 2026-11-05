//! Native screen pixels are composed over the final display output, never the HDR scene.
use crate::draw::{SceneGpu, overlay::OverlayGpu, pass_statistics::PassCounter};
use std::rc::Rc;

#[derive(Default)]
pub(super) struct OverlayStage {
    geometry: Option<Rc<OverlayGpu>>,
    counter: Option<PassCounter>,
    pipeline: Option<([u32; 2], wgpu::TextureFormat, wgpu::RenderPipeline)>,
}
impl super::FrameRenderer {
    pub(crate) fn prepare_scene_inputs(&mut self, device: &wgpu::Device, scene: &SceneGpu) {
        self.bind_water_inputs(&scene.water_inputs, scene.pass_counters.water.clone());
        self.overlay.geometry.clone_from(&scene.overlay);
        self.overlay.counter = Some(scene.pass_counters.ui.clone());
        let Some(geometry) = &scene.overlay else {
            return;
        };
        let size = self.targets().config().output_extent();
        let format = self.output_format;
        if self
            .overlay
            .pipeline
            .as_ref()
            .is_none_or(|p| p.0 != size || p.1 != format)
        {
            self.overlay.pipeline = Some((
                size,
                format,
                crate::renderer::overlay::build(device, format, size, &geometry.layout),
            ));
        }
    }
}
impl OverlayStage {
    pub fn encode(&self, encoder: &mut wgpu::CommandEncoder, output: &wgpu::TextureView) {
        let mut count = crate::draw::DrawCount::default();
        if let Some(geometry) = &self.geometry {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("native UI overlay after tone mapping"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            let (viewport, _, pipeline) =
                self.pipeline.as_ref().expect("prepared overlay pipeline");
            count = geometry
                .geometry
                .draw_overlay(&mut pass, pipeline, *viewport);
        }
        if let Some(counter) = &self.counter {
            counter.record(count);
        }
    }
}
