//! Native screen pixels are composed over the final display output, never the HDR scene.
use crate::draw::{SceneGpu, overlay::OverlayGpu, pass_statistics::PassCounter};
use std::rc::Rc;

struct Pipelines {
    size: [u32; 2],
    format: wgpu::TextureFormat,
    always: wgpu::RenderPipeline,
    tested: Option<wgpu::RenderPipeline>,
}
#[derive(Default)]
pub(super) struct OverlayStage {
    geometry: Option<Rc<OverlayGpu>>,
    counter: Option<PassCounter>,
    tested_counter: Option<PassCounter>,
    pipeline: Option<Pipelines>,
    depth: Option<wgpu::BindGroup>,
}
impl super::FrameRenderer {
    pub(crate) fn prepare_scene_inputs(&mut self, device: &wgpu::Device, scene: &SceneGpu) {
        self.bind_water_inputs(&scene.water_inputs, scene.pass_counters.water.clone());
        self.overlay.geometry.clone_from(&scene.overlay);
        self.overlay.counter = Some(scene.pass_counters.ui.clone());
        self.overlay.tested_counter = Some(scene.pass_counters.ui_depth_tested.clone());
        let Some(geometry) = &scene.overlay else {
            return;
        };
        let size = self.targets().config().output_extent();
        let format = self.output_format;
        if self.overlay.pipeline.as_ref().is_none_or(|p| {
            p.size != size || p.format != format || p.tested.is_some() != geometry.depth_test
        }) {
            let build = |depth| {
                crate::renderer::overlay::build(device, format, size, &geometry.layout, depth)
            };
            self.overlay.pipeline = Some(Pipelines {
                size,
                format,
                always: build(false),
                tested: geometry.depth_test.then(|| build(true)),
            });
        }
    }
}
impl OverlayStage {
    pub fn needs_depth(&self) -> bool {
        self.geometry.as_ref().is_some_and(|g| g.depth_test)
    }
    pub fn prepare_depth(
        &mut self,
        device: &wgpu::Device,
        snapshot: Option<&crate::water::snapshots::Snapshots>,
        frame: &crate::frame::FrameUniform,
    ) {
        self.depth = self.needs_depth().then(|| {
            super::overlay_depth::group(
                device,
                &snapshot.expect("scheduled annotation depth").final_depth,
                frame,
                self.pipeline.as_ref().expect("prepared overlay").size,
            )
        });
    }
    pub fn encode(&self, encoder: &mut wgpu::CommandEncoder, output: &wgpu::TextureView) {
        let (mut count, mut tested) = Default::default();
        if let Some(geometry) = &self.geometry {
            let pipelines = self.pipeline.as_ref().expect("prepared overlay pipeline");
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
            if let Some(depth) = &self.depth {
                pass.set_bind_group(0, depth, &[]);
            }
            (count, tested) = geometry.geometry.draw_with_pipeline(
                &mut pass,
                None,
                &pipelines.always,
                Some(pipelines.size),
                pipelines.tested.as_ref(),
            );
        }
        if let Some(counter) = &self.counter {
            counter.record(count);
        }
        if let Some(counter) = &self.tested_counter {
            counter.record(tested);
        }
    }
}
