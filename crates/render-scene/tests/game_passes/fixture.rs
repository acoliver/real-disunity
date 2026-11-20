//! The synthetic scenes and the rig that renders them through a configured renderer.
use std::time::Duration;
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_plan::{
    MaterialHandle, MeshHandle, ROTATION_MICRO_SCALE, RenderScene, SceneAlphaMode, SceneGeometry,
    SceneMaterial, SceneMaterialParts, SceneMesh, SceneMeshLod, SceneObject, SceneParts,
    SceneResources, SceneRotation, SceneTerrain, SceneTerrainAppearance, SceneTerrainParts,
    SceneTerrainSurface, SceneTexture, SceneTransform, SceneVertex, TerrainLodPolicy,
    TerrainStreamingPolicy, TexelFormat, TextureColorSpace, TextureHandle,
};
use worldrunner_render_scene::{
    CameraPose, SceneView, Viewport,
    compositor::{ConfiguredFrameRenderer, RendererConfiguration},
    device::GpuContext,
    draw::SceneGpu,
    executor::{AttachmentView, FrameConfig, FrameError, Readback, ResourceRef, ToneMap},
    frame::FrameUniform,
    renderer::ScenePipelines,
    view::OrthographicLens,
    wgpu,
};

pub const SIZE: [u32; 2] = [64, 48];
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
/// The texel of the stock quad's albedo.
pub const STOCK_TEXEL: [u8; 4] = [200, 120, 60, 255];

fn texture(texel: [u8; 4], color_space: TextureColorSpace) -> SceneTexture {
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space,
        width: 1,
        height: 1,
        mips: 1,
        bytes: texel.to_vec().into(),
    }
}

fn material() -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    })
    .unwrap()
}

/// One unlit quad, six metres on a side, lying in the ground plane.
pub fn quad_scene() -> RenderScene {
    let half = 3_000;
    let corners = [
        ([-half, -half, 0], [0, 0]),
        ([half, -half, 0], [1_000_000, 0]),
        ([half, half, 0], [1_000_000, 1_000_000]),
        ([-half, half, 0], [0, 1_000_000]),
    ];
    let mesh = SceneMesh::new(vec![
        SceneMeshLod::new(
            0,
            corners
                .into_iter()
                .map(|(position, uv)| SceneVertex {
                    tangent_micro: None,
                    position_millimeters: position,
                    normal_micro: [0, 0, 1_000_000],
                    texcoord_micro: uv,
                })
                .collect(),
            vec![0, 1, 2, 0, 2, 3],
            None,
        )
        .unwrap(),
    ])
    .unwrap();
    let object = SceneObject::new(
        SceneTransform {
            position_millimeters: [0, 0, 0],
            rotation: SceneRotation {
                x: 0,
                y: 0,
                z: 0,
                w: ROTATION_MICRO_SCALE,
            },
            scale_millionths: 1_000_000,
        },
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    );
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![texture(STOCK_TEXEL, TextureColorSpace::Srgb)],
            vec![material()],
        )
        .unwrap(),
        geometry: SceneGeometry::new(vec![mesh], vec![object]).unwrap(),
        terrain: None,
        effects: None,
    })
    .unwrap()
}

/// The material of a terrain palette entry, which names all three of its maps.
fn terrain_material() -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: Some(TextureHandle::from_raw(1)),
        roughness_metallic: Some(TextureHandle::from_raw(2)),
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    })
    .unwrap()
}

/// A flat white terrain four metres on a side and nothing else.
pub fn terrain_scene() -> RenderScene {
    let terrain = SceneTerrain::new(SceneTerrainParts {
        columns: 2,
        rows: 2,
        origin_east_millimeters: -2000,
        origin_north_millimeters: -2000,
        sample_spacing_millimeters: 4000,
        heights_millimeters: vec![0; 4],
        surface_indices: vec![0; 4],
        surfaces: vec![SceneTerrainSurface::new(
            MaterialHandle::from_raw(0),
            SceneTerrainAppearance::NEUTRAL,
        )],
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap();
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![
                texture([255; 4], TextureColorSpace::Linear),
                texture([128, 128, 255, 255], TextureColorSpace::Linear),
                texture([255, 0, 0, 255], TextureColorSpace::Linear),
            ],
            vec![terrain_material()],
        )
        .unwrap(),
        geometry: SceneGeometry::new(Vec::new(), Vec::new()).unwrap(),
        terrain: Some(terrain),
        effects: None,
    })
    .unwrap()
}

/// A flat white terrain four metres on a side whose stock selection streams it in chunks.
pub fn streamed_terrain_scene() -> RenderScene {
    let flat = terrain_scene();
    let source = flat.terrain().unwrap();
    let terrain = SceneTerrain::new(SceneTerrainParts {
        columns: 5,
        rows: 5,
        origin_east_millimeters: -2000,
        origin_north_millimeters: -2000,
        sample_spacing_millimeters: 1000,
        heights_millimeters: vec![0; 25],
        surface_indices: vec![0; 25],
        surfaces: source.surfaces().to_vec(),
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap()
    .with_streaming_policy(Some(TerrainStreamingPolicy::new(2, 9, 4, 1000).unwrap()));
    flat.with_terrain(Some(std::sync::Arc::new(terrain)))
        .unwrap()
}

/// Straight down from five metres, so depth is linear in height: the ground plane is at
/// `(5 - 0.1) / (10 - 0.1)`.
pub fn view(size: [u32; 2]) -> SceneView {
    view_from(5000, size)
}

/// [`view`] from `height_millimeters` above the ground plane, which stays inside the 10 m far
/// plane below 10 000: the ground plane's linear depth is the height in metres.
pub fn view_from(height_millimeters: i64, size: [u32; 2]) -> SceneView {
    SceneView::orthographic_with_up(
        CameraPose {
            eye_millimeters: [0, 0, height_millimeters],
            target_millimeters: [0, 0, 0],
        },
        OrthographicLens {
            vertical_half_extent_meters: 4.0,
            near_meters: 0.1,
            far_meters: 10.0,
        },
        [0.0, 1.0, 0.0],
        Viewport {
            width: size[0],
            height: size[1],
        },
    )
    .unwrap()
}

/// A configured legacy renderer with a resident scene, and the frames it draws.
pub struct Rig {
    pub context: GpuContext,
    pub renderer: ConfiguredFrameRenderer,
    pub gpu: SceneGpu,
    pub pipelines: ScenePipelines,
    frame: FrameUniform,
    size: [u32; 2],
}

impl Rig {
    pub fn new(
        scene: &RenderScene,
        size: [u32; 2],
        configure: impl FnOnce(&mut RendererConfiguration),
    ) -> Self {
        Self::build(scene, size, (false, wgpu::Features::empty()), configure)
    }

    /// A rig on the shared test device with exactly `features`, for a game whose targets need
    /// them, such as block-compressed ones.
    pub fn with_features(
        scene: &RenderScene,
        size: [u32; 2],
        features: wgpu::Features,
        configure: impl FnOnce(&mut RendererConfiguration),
    ) -> Self {
        Self::build(scene, size, (false, features), configure)
    }

    /// A rig whose frame is HDR: the scene colour is `Rgba16Float`, which the opaque colour
    /// snapshot needs, and the output is tone mapped into [`FORMAT`].
    pub fn hdr(
        scene: &RenderScene,
        size: [u32; 2],
        configure: impl FnOnce(&mut RendererConfiguration),
    ) -> Self {
        Self::build(scene, size, (true, wgpu::Features::empty()), configure)
    }

    fn build(
        scene: &RenderScene,
        size: [u32; 2],
        (hdr, features): (bool, wgpu::Features),
        configure: impl FnOnce(&mut RendererConfiguration),
    ) -> Self {
        let context = GpuContext::shared_for_test_with_features(features)
            .expect("game pass acceptance requires a real adapter; never skip");
        let mut settings = RendererConfiguration::legacy(size, FORMAT);
        let scene_format = if hdr {
            settings.frame.format = wgpu::TextureFormat::Rgba16Float;
            settings.hdr = Some((1.0, ToneMap::Reinhard));
            wgpu::TextureFormat::Rgba16Float
        } else {
            FORMAT
        };
        configure(&mut settings);
        let renderer =
            ConfiguredFrameRenderer::new(&context, settings, AllocationBudget(1 << 28)).unwrap();
        let pipelines = ScenePipelines::new(context.device(), scene_format);
        let gpu =
            SceneGpu::upload(context.device(), context.submission(), scene, &pipelines).unwrap();
        let view = view(size);
        let frame = FrameUniform::unlit(&view);
        gpu.set_frame(context.queue(), frame);
        Self {
            context,
            renderer,
            gpu,
            pipelines,
            frame,
            size,
        }
    }

    /// A second renderer on this rig's device, built from `settings` as a derived configuration is.
    pub fn build_renderer(&self, settings: RendererConfiguration) -> ConfiguredFrameRenderer {
        ConfiguredFrameRenderer::new(&self.context, settings, AllocationBudget(1 << 28)).unwrap()
    }

    /// The allocation a renderer reports for the frames it has resident, charged to its budget.
    pub fn resident_bytes(&self) -> u64 {
        self.renderer
            .resident_bytes(&self.context.device().limits())
            .unwrap()
    }

    /// Encodes one frame's geometry and resolve into `encoder`.
    pub fn encode(&mut self, encoder: &mut wgpu::CommandEncoder) -> Result<(), FrameError> {
        let targets = self.renderer.targets();
        let color =
            AttachmentView::new(ResourceRef::new("scene color"), targets.scene().texture())?;
        let depth =
            AttachmentView::new(ResourceRef::new("scene depth"), targets.depth().texture())?;
        self.renderer.encoding().encode_scene(
            self.context.device(),
            encoder,
            (&mut self.gpu, &self.pipelines),
            (&color, &depth),
            self.frame,
        )?;
        self.renderer.targets().encode_resolve(encoder);
        Ok(())
    }

    /// Draws one frame, submits it and returns the frame's colour.
    pub fn render(&mut self) -> Result<Vec<[u8; 4]>, FrameError> {
        let mut encoder = self
            .context
            .submission()
            .begin(self.context.device())
            .unwrap();
        self.encode(&mut encoder)?;
        encoder.finish().submit().unwrap();
        Ok(self.read(self.renderer.targets().texture()))
    }

    /// Draws one frame in the host's frame `slot`, as seen from `height_millimeters`, submits it
    /// and returns the frame's colour. Every slot has its own frame uniform on the scene.
    pub fn render_in_slot(
        &mut self,
        slot: usize,
        height_millimeters: i64,
    ) -> Result<Vec<[u8; 4]>, FrameError> {
        self.renderer.select_frame_slot(slot);
        self.gpu.select_frame_slot(slot);
        self.frame = FrameUniform::unlit(&view_from(height_millimeters, self.size));
        self.gpu.set_frame(self.context.queue(), self.frame);
        self.render()
    }

    /// Draws one frame and cancels it before submission.
    pub fn render_cancelled(&mut self) -> Result<(), FrameError> {
        let mut encoder = self
            .context
            .submission()
            .begin(self.context.device())
            .unwrap();
        self.encode(&mut encoder)?;
        encoder.finish().cancel();
        Ok(())
    }

    pub fn read(&self, texture: &wgpu::Texture) -> Vec<[u8; 4]> {
        let config = FrameConfig::new([texture.width(), texture.height()], texture.format(), 1);
        let mut readback = Readback::new(self.context.device(), config, 1 << 22).unwrap();
        readback
            .read(self.context.device(), self.context.submission(), texture)
            .unwrap()
            .chunks_exact(4)
            .map(|texel| texel.try_into().unwrap())
            .collect()
    }

    /// The bytes of a buffer that allows `COPY_SRC`, after every submitted frame completed.
    pub fn read_buffer(&self, buffer: &wgpu::Buffer) -> Vec<u8> {
        let device = self.context.device();
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("game pass readback"),
            size: buffer.size(),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.copy_buffer_to_buffer(buffer, 0, &staging, 0, buffer.size());
        self.context
            .submission()
            .submit_commands([encoder.finish()])
            .unwrap();
        staging
            .slice(..)
            .map_async(wgpu::MapMode::Read, |mapped| mapped.unwrap());
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(30)),
            })
            .unwrap();
        staging.slice(..).get_mapped_range().unwrap().to_vec()
    }

    pub fn pixel(image: &[[u8; 4]], column: usize, row: usize) -> [u8; 4] {
        image[row * SIZE[0] as usize + column]
    }
}
