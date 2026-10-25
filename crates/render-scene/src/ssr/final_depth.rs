use super::{
    AttachmentView, FeaturePass, FrameError, FrameProduct, PassDeclaration, PassSlot, ResourceRef,
    gpu,
};

#[derive(Debug)]
pub(super) struct FinalDepth {
    levels: Vec<wgpu::Texture>,
    samples: u32,
    copy: wgpu::RenderPipeline,
    reduce: wgpu::RenderPipeline,
}
impl FinalDepth {
    pub fn new(device: &wgpu::Device, mut size: [u32; 2], samples: u32) -> Self {
        let mut levels = Vec::new();
        loop {
            levels.push(crate::ssao::passes::texture(
                device,
                size,
                wgpu::TextureFormat::R32Float,
            ));
            if size == [1, 1] || levels.len() == 7 {
                break;
            }
            size = size.map(|side| side.div_ceil(2));
        }
        Self {
            levels,
            samples,
            copy: crate::ssao::passes::pipeline(device, include_str!("final_depth.wgsl")),
            reduce: crate::ssao::passes::pipeline(device, include_str!("../ssao/reduce.wgsl")),
        }
    }
    pub fn textures(&self) -> &[wgpu::Texture] {
        &self.levels
    }
    pub fn trace_levels(&self) -> Vec<wgpu::Texture> {
        [0, 2, 4, 6]
            .map(|i| self.levels[i.min(self.levels.len() - 1)].clone())
            .to_vec()
    }
    pub fn reads(&self) -> Vec<ResourceRef> {
        [0, 2, 4, 6]
            .map(|i| {
                FrameProduct::reflection_depth(
                    u32::try_from(i.min(self.levels.len() - 1)).expect("depth levels"),
                    self.samples,
                )
                .resource()
            })
            .to_vec()
    }
    pub fn schedule(
        &self,
        samples: u32,
        passes: &mut Vec<PassDeclaration>,
        views: &mut Vec<AttachmentView>,
    ) -> Result<Vec<FeaturePass>, FrameError> {
        self.levels
            .iter()
            .enumerate()
            .map(|(i, texture)| {
                let index = u32::try_from(i).expect("depth levels");
                let slot = PassSlot::ReflectionDepthPyramid(index);
                let view =
                    FrameProduct::reflection_depth(index, self.samples).attachment(texture)?;
                let mut pass = PassDeclaration::new(slot);
                pass.color = Some(crate::compositor::attachment(view.resource(), &view, false));
                views.push(view);
                passes.push(pass);
                let mut feature = FeaturePass::new(slot);
                feature.color_format = Some(wgpu::TextureFormat::R32Float);
                feature.reads = vec![
                    if index == 0 {
                        FrameProduct::final_depth(samples)
                    } else {
                        FrameProduct::reflection_depth(index - 1, self.samples)
                    }
                    .resource(),
                ];
                Ok(feature)
            })
            .collect()
    }
    pub fn groups(&self, device: &wgpu::Device, input: &wgpu::TextureView) -> Vec<wgpu::BindGroup> {
        (0..self.levels.len())
            .map(|i| {
                let source = if i == 0 {
                    input.clone()
                } else {
                    self.levels[i - 1].create_view(&wgpu::TextureViewDescriptor::default())
                };
                gpu::group(device, self.pipeline(i), None, &[source])
            })
            .collect()
    }
    pub fn pipeline(&self, index: usize) -> &wgpu::RenderPipeline {
        if index == 0 { &self.copy } else { &self.reduce }
    }
    #[cfg(test)]
    pub fn encode_for_test(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        input: &wgpu::Texture,
    ) {
        let groups = self.groups(
            device,
            &input.create_view(&wgpu::TextureViewDescriptor::default()),
        );
        for (i, (texture, group)) in self.levels.iter().zip(&groups).enumerate() {
            worldrunner_render_post::fullscreen_pass(
                encoder,
                Some("final depth reference"),
                wgpu::RenderPassColorAttachment {
                    view: &texture.create_view(&wgpu::TextureViewDescriptor::default()),
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                },
                self.pipeline(i),
                &[group],
            );
        }
    }
}
