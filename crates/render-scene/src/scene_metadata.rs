//! Shared pixel-centre geometry metadata preparation for AO, depth statistics
//! and reflections. Forward MSAA coverage is deliberately a separate domain.
mod geometry;
mod output;
pub(crate) mod receiver;
use crate::{frame::FrameUniform, renderer::ScenePipelines};
pub(crate) use geometry::pipelines;
#[cfg(test)]
pub(crate) use geometry::{pipelines_from_sources, sources};
#[cfg(test)]
pub(crate) use output::single_fragment;
pub(crate) use output::{MERGED_MARKER, MetadataOutput};
use worldrunner_render_plan::SceneSsao;
use worldrunner_render_post::{FrameError, PrepareContext};

#[derive(Clone, Debug)]
pub(crate) struct Geometry {
    pub pipelines: ScenePipelines,
    pub group: wgpu::BindGroup,
    pub ambient: wgpu::BindGroup,
    /// Pipelines that write the prepass normal and the reflection material in one draw, present
    /// when reflections are installed.
    pub merged: Option<ScenePipelines>,
    pub forward: Option<(ScenePipelines, wgpu::BindGroup)>,
}

#[derive(Clone, Debug)]
pub(crate) struct SceneMetadata {
    pub settings: SceneSsao,
    pub size: [u32; 2],
    pub normals: wgpu::Texture,
    pub depth: wgpu::Texture,
    pub(crate) independent_depth: wgpu::Texture,
    pub geometry: Geometry,
    pub(crate) receiver: Option<receiver::ReceiverPayload>,
    pub passes: crate::ssao::passes::ScreenPasses,
    pub(crate) uniform: wgpu::Buffer,
}

impl Geometry {
    /// The pipelines the depth and normal prepass draws with.
    pub fn prepass_pipelines(&self) -> &ScenePipelines {
        self.merged.as_ref().unwrap_or(&self.pipelines)
    }
}

impl SceneMetadata {
    pub fn new(
        device: &wgpu::Device,
        size: [u32; 2],
        settings: SceneSsao,
    ) -> Result<Self, FrameError> {
        crate::ssao::validate_settings(&settings)?;
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("screen camera"),
            size: 224,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let (pipelines, group) = geometry::pipelines(device, &uniform, MetadataOutput::Normal);
        let (normals, depth) = source_textures(device, size)?;
        let passes = crate::ssao::passes::ScreenPasses::new(
            device, size, &uniform, &depth, &normals, settings,
        );
        let ambient =
            crate::ssao::receiver::group(device, &passes.filtered, &uniform, settings.receiver);
        Ok(Self {
            settings,
            size,
            normals,
            independent_depth: depth.clone(),
            depth,
            geometry: Geometry {
                pipelines,
                group,
                ambient,
                merged: None,
                forward: None,
            },
            passes,
            receiver: None,
            uniform,
        })
    }

    pub(crate) fn geometry_for_depth(&self, depth: crate::renderer::depth::DepthMode) -> Geometry {
        use crate::renderer::depth::DepthMode;
        let mut geometry = self.geometry.clone();
        if let Some(receiver) = &self.receiver {
            geometry.merged = Some(receiver.pipelines.clone());
        }
        let metadata = if self.depth.format().has_stencil_aspect() {
            if depth == DepthMode::Reuse || self.receiver.is_some() {
                DepthMode::Prepass
            } else {
                DepthMode::Shared
            }
        } else {
            DepthMode::Independent
        };
        geometry.pipelines = geometry.pipelines.for_depth(metadata);
        geometry.merged = geometry
            .merged
            .as_ref()
            .map(|pipelines| pipelines.for_depth(metadata));
        if let Some((pipelines, _)) = &mut geometry.forward {
            *pipelines = pipelines.for_depth(depth);
        }
        geometry
    }

    pub(crate) fn depth_resource(&self) -> worldrunner_render_post::ResourceRef {
        if self.depth.format().has_stencil_aspect() {
            worldrunner_render_post::ResourceRef::new("scene depth")
        } else {
            crate::frame::metadata::FrameProduct::prepass_depth().resource()
        }
    }

    pub(crate) fn share_depth(&mut self, device: &wgpu::Device, depth: &wgpu::Texture) {
        assert_eq!(depth.sample_count(), 1);
        assert_eq!([depth.width(), depth.height()], self.size);
        if self.depth == *depth {
            return;
        }
        self.depth = depth.clone();
        self.passes
            .rebind_depth(device, &self.uniform, depth, self.depth_resource());
    }

    pub fn upload(&self, context: &mut PrepareContext<'_>, frame: &FrameUniform) {
        let projection = frame.view_projection * frame.view.inverse();
        let mut data = Vec::new();
        for matrix in [frame.view, projection, projection.inverse()] {
            data.extend(matrix.to_cols_array());
        }
        let (turns, power) = match self.settings.kernel {
            worldrunner_render_plan::SceneSsaoKernel::Scalable {
                turns_micro,
                power_micro,
            } => (
                crate::ssao::scalar(u64::from(turns_micro)) / 1_000_000.0,
                crate::ssao::scalar(u64::from(power_micro)) / 1_000_000.0,
            ),
            _ => (0.0, 0.0),
        };
        data.extend([
            crate::ssao::scalar(self.settings.radius_millimeters) / 1000.0,
            crate::ssao::scalar(self.settings.intensity_micro) / 1_000_000.0,
            crate::view::meters_scalar(self.settings.bias_millimeters),
            (crate::view::meters_scalar(self.settings.horizon_angle_millidegrees).to_radians())
                .cos(),
            crate::ssao::scalar(u64::from(self.settings.sample_count)),
            turns,
            power,
            0.0,
        ]);
        context.upload_buffer(
            &self.uniform,
            0,
            &data
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<_>>(),
        );
    }
}

fn source_textures(
    device: &wgpu::Device,
    size: [u32; 2],
) -> Result<(wgpu::Texture, wgpu::Texture), FrameError> {
    let normals = crate::ssao::passes::texture(device, size, wgpu::TextureFormat::Rgba16Float);
    let depth = crate::ssao::passes::texture(device, size, wgpu::TextureFormat::Depth32Float);
    crate::frame::metadata::validate_inputs(
        &depth,
        &normals,
        None,
        None,
        crate::frame::metadata::MetadataSampling::PixelCenter,
    )
    .map_err(|_| FrameError::SsaoSettings("source metadata contract"))?;
    Ok((normals, depth))
}

#[cfg(test)]
thread_local! {
    pub(crate) static EXECUTED_PASSES: std::cell::RefCell<Vec<worldrunner_render_post::PassSlot>> = const { std::cell::RefCell::new(Vec::new()) };
}
#[cfg(test)]
pub(crate) fn record_pass(slot: worldrunner_render_post::PassSlot) {
    EXECUTED_PASSES.with_borrow_mut(|passes| passes.push(slot));
}
