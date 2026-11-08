use super::ReceiverLayout;
use worldrunner_render_post::{AttachmentView, FrameError, PassDeclaration, ResourceRef};

#[derive(Clone, Debug)]
pub(crate) struct ReceiverPayload {
    pub(crate) layout: ReceiverLayout,
    pub(crate) bias: f32,
    pub(crate) output: super::super::MetadataOutput,
    pub(crate) textures: [wgpu::Texture; 2],
    pub(crate) pipelines: crate::renderer::ScenePipelines,
}
impl ReceiverPayload {
    pub(super) fn output_for(
        metadata: &super::super::SceneMetadata,
    ) -> super::super::MetadataOutput {
        if metadata.geometry.merged.is_some() {
            super::super::MetadataOutput::Merged
        } else {
            super::super::MetadataOutput::Normal
        }
    }
    pub(crate) fn new(
        device: &wgpu::Device,
        metadata: &super::super::SceneMetadata,
        layout: ReceiverLayout,
        bias: f32,
    ) -> Result<Self, FrameError> {
        let output = Self::output_for(metadata);
        layout
            .validate(
                &device.limits(),
                if metadata.geometry.merged.is_some() {
                    2
                } else {
                    1
                },
            )
            .map_err(FrameError::UnsupportedEffect)?;
        let sources = super::producer_sources(output, layout, bias);
        let pipelines =
            super::super::geometry::pipelines_from_sources(device, &metadata.uniform, &sources).0;
        let textures = [
            wgpu::TextureFormat::Rgba32Float,
            wgpu::TextureFormat::Rg32Float,
        ]
        .map(|format| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("receiver displacements"),
                size: wgpu::Extent3d {
                    width: metadata.size[0],
                    height: metadata.size[1],
                    depth_or_array_layers: layout.sides(),
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        });
        Ok(Self {
            layout,
            bias,
            output,
            textures,
            pipelines,
        })
    }
    pub(crate) fn resource(index: u32, side: u32) -> ResourceRef {
        ResourceRef {
            layer: side,
            ..ResourceRef::new(if index == 0 {
                "receiver displacement xyzw"
            } else {
                "receiver displacement yz"
            })
        }
    }
    pub(crate) fn resources(&self) -> Vec<ResourceRef> {
        (0..self.layout.sides())
            .flat_map(|side| (0..2).map(move |index| Self::resource(index, side)))
            .collect()
    }
    pub(crate) fn attach(
        &self,
        pass: &mut PassDeclaration,
        views: &mut Vec<AttachmentView>,
    ) -> Result<(), FrameError> {
        for side in 0..self.layout.sides() {
            for (index, texture) in self.textures.iter().enumerate() {
                let resource = Self::resource(u32::try_from(index).expect("two textures"), side);
                let view = AttachmentView::new(resource, texture)?;
                let mut attachment = crate::compositor::attachment(resource, &view, false);
                attachment.load =
                    worldrunner_render_post::AttachmentLoad::ClearColor(wgpu::Color::TRANSPARENT);
                pass.additional_colors.push(attachment);
                views.push(view);
            }
        }
        Ok(())
    }
    pub(crate) fn views(&self) -> [wgpu::TextureView; 2] {
        self.textures.each_ref().map(|texture| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            })
        })
    }
}
