use super::*;
use crate::{
    device::GpuContext,
    draw::SceneGpu,
    frame::FrameUniform,
    renderer::ScenePipelines,
    view::{CameraPose, OrthographicLens, SceneView, Viewport},
};
use worldrunner_render_plan::*;

pub(super) fn material(emission: u32) -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(emission),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: Some(TextureHandle::from_raw(0)),
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: true,
    })
    .unwrap()
}
pub(super) fn quad() -> SceneMesh {
    let vertices = [
        [-707, 0, -707],
        [707, 0, -707],
        [707, 0, 707],
        [-707, 0, 707],
    ]
    .map(|position_millimeters| SceneVertex {
        tangent_micro: None,
        position_millimeters,
        normal_micro: [0, -1_000_000, 0],
        texcoord_micro: [0, 0],
    });
    SceneMesh::new(vec![
        SceneMeshLod::new(0, vertices.to_vec(), vec![0, 1, 2, 0, 2, 3], None).unwrap(),
    ])
    .unwrap()
}
pub(super) fn textures() -> [SceneTexture; 4] {
    [
        [255, 255, 255, 255],
        [255, 0, 0, 255],
        [0, 255, 0, 255],
        [64, 128, 192, 255],
    ]
    .map(|color| SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: color.to_vec().into(),
    })
}
pub(super) fn base_effects(dome: bool) -> SceneEffects {
    let camera = SceneCamera {
        vertical_fov_millidegrees: 60_000,
        near_plane_millimeters: 100,
        far_plane_millimeters: 100_000,
    };
    let grey = SceneColor {
        red: 0,
        green: 0,
        blue: 0,
    };
    SceneEffects::new(SceneEffectsParts {
        water: None,
        sky: if dome {
            SceneSky::DomeTexture {
                texture: TextureHandle::from_raw(3),
            }
        } else {
            SceneSky::ProceduralHemisphere {
                zenith: grey,
                horizon: grey,
            }
        },
        sun: SceneSun {
            direction: SceneDirection::new([0, 0, 1_000_000]).unwrap(),
            irradiance_micro: 0,
        },
        ambient: SceneAmbient {
            sky_irradiance_micro: 0,
            ground_irradiance_micro: 0,
        },
        shadows: SceneShadowCascades::new(vec![10_000], 0).unwrap(),
        ssao: None,
        camera,
        grading: SceneColorGrading {
            exposure_micro: 1_000_000,
            contrast_micro: 1_000_000,
            saturation_micro: 1_000_000,
        },
        post: vec![],
        anti_alias: SceneAntiAlias::Disabled,
    })
    .unwrap()
}
pub(super) fn effects(density: u32, dome: bool) -> SceneEffects {
    base_effects(dome)
        .with_atmosphere(atmosphere(density))
        .unwrap()
}
pub(super) fn atmosphere(density: u32) -> SceneAtmosphere {
    SceneAtmosphere {
        horizon_radiance_micro: [0, 0, 2_000_000],
        zenith_radiance_micro: [0, 0, 2_000_000],
        gradient_exponent_micro: 1_000_000,
        sun_angular_radius_millidegrees: 0,
        fog: Some(SceneHeightFog {
            density_micro_per_meter: density,
            falloff_micro_per_meter: 0,
            base_height_millimeters: 0,
            start_distance_millimeters: 0,
            radiance_micro: [0, 0, 1_000_000],
        }),
        discs: vec![],
        meshes: vec![SceneCelestialMesh {
            direction: SceneDirection::new([0, 1_000_000, 0]).unwrap(),
            angular_radius_millidegrees: 30_000,
            distance_millimeters: 10_000,
            model_radius_millimeters: 1000,
            rotation: SceneRotation::IDENTITY,
            mesh: MeshHandle::from_raw(0),
            material: MaterialHandle::from_raw(1),
        }],
        background_distance_millimeters: 20_000,
    }
}
pub(super) fn objects(enabled: bool) -> Vec<SceneObject> {
    if enabled {
        [[-1333, 2000, 0], [1333, 6000, 0]]
            .map(|position_millimeters| {
                SceneObject::new(
                    SceneTransform {
                        position_millimeters,
                        rotation: SceneRotation::IDENTITY,
                        scale_millionths: 700_000,
                    },
                    MeshHandle::from_raw(0),
                    MaterialHandle::from_raw(0),
                )
            })
            .to_vec()
    } else {
        vec![]
    }
}
pub(super) fn scene(density: u32, enabled: bool, dome: bool) -> RenderScene {
    RenderScene::new(SceneParts {
        resources: SceneResources::new(textures().to_vec(), vec![material(1), material(2)])
            .unwrap(),
        geometry: SceneGeometry::new(vec![quad()], objects(enabled)).unwrap(),
        terrain: None,
        effects: Some(effects(density, dome)),
    })
    .unwrap()
}
pub(super) struct Capture {
    pub(super) color: Vec<u8>,
    pub(super) depth: Vec<u8>,
}
impl Capture {
    pub(super) fn rgb(&self, x: usize) -> [f32; 3] {
        self.rgb_at(x, 1)
    }
    pub(super) fn rgb_at(&self, x: usize, y: usize) -> [f32; 3] {
        std::array::from_fn(|c| {
            let offset = y * 256 + x * 8 + c * 2;
            let bits = u16::from_le_bytes(self.color[offset..offset + 2].try_into().unwrap());
            let exponent = i32::from((bits >> 10) & 31);
            let fraction = f32::from(bits & 1023) / 1024.0;
            assert_eq!(bits & 0x8000, 0);
            if exponent == 0 {
                fraction * 2.0f32.powi(-14)
            } else {
                (1.0 + fraction) * 2.0f32.powi(exponent - 15)
            }
        })
    }
    pub(super) fn depth(&self, x: usize) -> f32 {
        f32::from_le_bytes(self.depth[256 + x * 4..260 + x * 4].try_into().unwrap())
    }
}
pub(super) fn render(context: &GpuContext, source: &RenderScene, exposure: f32) -> Capture {
    render_at(context, source, exposure, &view())
}
pub(super) fn render_at(
    context: &GpuContext,
    source: &RenderScene,
    exposure: f32,
    view: &SceneView,
) -> Capture {
    let device = context.device();
    let pipelines = ScenePipelines::new(device, wgpu::TextureFormat::Rgba16Float);
    let mut gpu = SceneGpu::upload(device, context.submission(), source, &pipelines).unwrap();
    let mut frame = FrameUniform::new(view, source.effects()).unwrap();
    frame.exposure = exposure;
    capture(context, |encoder, color, depth| {
        use worldrunner_render_post::{AttachmentView, ResourceRef};
        let color =
            AttachmentView::new(ResourceRef::new("acceptance color"), color.texture()).unwrap();
        let depth =
            AttachmentView::new(ResourceRef::new("acceptance depth"), depth.texture()).unwrap();
        let mut renderer = crate::compositor::FrameRenderer::new(device);
        crate::compositor::encode_scene(
            &mut renderer,
            device,
            encoder,
            (&mut gpu, &pipelines),
            (&color, &depth),
            frame,
        )
        .unwrap();
    })
}
pub(super) fn render_resident(
    context: &GpuContext,
    gpu: &SceneGpu,
    pipelines: &ScenePipelines,
    frame: &FrameUniform,
) -> Capture {
    gpu.set_frame(context.queue(), *frame);
    capture(context, |encoder, color, depth| {
        draw(context.device(), encoder, gpu, pipelines, color, depth);
    })
}
pub(super) fn capture(
    context: &GpuContext,
    mut encode: impl FnMut(&mut wgpu::CommandEncoder, &wgpu::TextureView, &wgpu::TextureView),
) -> Capture {
    let device = context.device();
    let color = target(device, wgpu::TextureFormat::Rgba16Float);
    let depth = target(device, wgpu::TextureFormat::Depth32Float);
    let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
    let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
    let buffers = std::array::from_fn::<_, 2, _>(|_| {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene acceptance readback"),
            size: 768,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        })
    });
    let prepared = context
        .submission()
        .prepare(device, |encoder, _| {
            encode(encoder, &color_view, &depth_view);
            for (index, texture) in [&color, &depth].into_iter().enumerate() {
                encoder.copy_texture_to_buffer(
                    wgpu::TexelCopyTextureInfo {
                        texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: if index == 0 {
                            wgpu::TextureAspect::All
                        } else {
                            wgpu::TextureAspect::DepthOnly
                        },
                    },
                    wgpu::TexelCopyBufferInfo {
                        buffer: &buffers[index],
                        layout: wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(256),
                            rows_per_image: Some(3),
                        },
                    },
                    texture.size(),
                );
            }
            Ok::<_, std::convert::Infallible>(())
        })
        .unwrap();
    prepared.submit().unwrap();
    Capture {
        color: read(device, &buffers[0]),
        depth: read(device, &buffers[1]),
    }
}
#[test]
fn full_scene_mesh_occlusion_fog_distance_and_single_exposure() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let source = scene(100_000, true, false);
    let image = render(&context, &source, 2.0);
    let unobstructed = render(&context, &scene(100_000, false, false), 2.0);
    for (pixel, distance) in [(3, 2.0f32), (5, 6.0)] {
        let rgb = image.rgb(pixel);
        let transmittance = (-0.1 * distance).exp();
        assert!(
            (rgb[0] - 2.0 * transmittance).abs() < 0.003,
            "object {pixel}: {rgb:?}"
        );
        assert_eq!(
            rgb[1].to_bits(),
            0.0f32.to_bits(),
            "object occludes celestial green emission"
        );
        assert!((rgb[2] - 2.0 * (1.0 - transmittance)).abs() < 0.003);
        assert!(image.depth(pixel) < 1.0);
        assert!(
            unobstructed.rgb(pixel)[1] > 0.6,
            "celestial mesh covers the object pixel"
        );
        assert_eq!(unobstructed.depth(pixel).to_bits(), 1.0f32.to_bits());
    }
    assert!(
        image.rgb(3)[0] > image.rgb(5)[0],
        "fog increases over distance"
    );
    assert_eq!(image.depth(0).to_bits(), 1.0f32.to_bits());
    assert!(
        (image.rgb(0)[2] - 2.0 * (1.0 + (-2.0f32).exp())).abs() < 0.003,
        "fogged sky behind objects"
    );
    assert!(
        (image.rgb(4)[1] - 2.0 * (-1.0f32).exp()).abs() < 0.003,
        "celestial exposure and fog"
    );
    let unit = render(&context, &source, 1.0);
    for pixel in [0, 3, 4, 5, 8] {
        for (twice, once) in image.rgb(pixel).into_iter().zip(unit.rgb(pixel)) {
            assert!(
                (twice - 2.0 * once).abs() < 0.003,
                "one exposure at pixel {pixel}"
            );
        }
    }
}
#[test]
fn dome_texture_is_linear_and_uses_the_existing_scene_upload() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let image = render(&context, &scene(0, false, true), 2.0);
    for (actual, channel) in image.rgb(0).into_iter().zip([64u16, 128, 192]) {
        assert!((actual - 2.0 * f32::from(channel) / 255.0).abs() < 0.002);
    }
    assert_eq!(image.depth(0).to_bits(), 1.0f32.to_bits());
}

pub(super) fn view() -> SceneView {
    SceneView::orthographic_with_up(
        CameraPose {
            eye_millimeters: [0, 0, 0],
            target_millimeters: [0, 1000, 0],
        },
        OrthographicLens {
            vertical_half_extent_meters: 2.0,
            near_meters: 0.1,
            far_meters: 100.0,
        },
        [0.0, 0.0, 1.0],
        Viewport {
            width: 9,
            height: 3,
        },
    )
    .unwrap()
}

pub(super) fn target(device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("atmosphere scene acceptance"),
        size: wgpu::Extent3d {
            width: 9,
            height: 3,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

pub(super) fn read(device: &wgpu::Device, buffer: &wgpu::Buffer) -> Vec<u8> {
    let (send, recv) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            send.send(result).unwrap();
        });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    recv.recv().unwrap().unwrap();
    let data = buffer.slice(..).get_mapped_range().unwrap().to_vec();
    buffer.unmap();
    data
}

pub(super) fn draw(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    gpu: &SceneGpu,
    pipelines: &ScenePipelines,
    color_view: &wgpu::TextureView,
    depth_view: &wgpu::TextureView,
) {
    use worldrunner_render_post::{AttachmentView, ResourceRef};
    let color = AttachmentView::new(ResourceRef::new("acceptance color"), color_view.texture());
    let depth = AttachmentView::new(ResourceRef::new("acceptance depth"), depth_view.texture());
    let mut renderer = crate::compositor::FrameRenderer::new(device);
    crate::compositor::encode_resident(
        &mut renderer,
        device,
        encoder,
        (gpu, pipelines),
        (&color.unwrap(), &depth.unwrap()),
    )
    .unwrap();
}

#[test]
fn dome_without_an_atmosphere_is_rejected_on_upload() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let source = RenderScene::new(SceneParts {
        resources: SceneResources::new(textures().to_vec(), vec![]).unwrap(),
        geometry: SceneGeometry::new(vec![], vec![]).unwrap(),
        terrain: None,
        effects: Some(base_effects(true)),
    })
    .unwrap();
    let pipelines = ScenePipelines::new(context.device(), wgpu::TextureFormat::Rgba16Float);
    let error =
        SceneGpu::upload(context.device(), context.submission(), &source, &pipelines).unwrap_err();
    assert!(matches!(
        error,
        crate::draw::DrawError::Frame(crate::frame::FrameError::UnsupportedEffect(
            "dome sky requires atmosphere"
        ))
    ));
}

#[test]
fn dome_longitudes_poles_and_camera_translation_use_scene_z_up() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let source = directional_dome();
    for (direction, up, expected) in [
        ([1000, 0, 0], [0.0, 0.0, 1.0], [255u16, 0, 0]),
        ([-1000, 0, 0], [0.0, 0.0, 1.0], [0, 0, 255]),
        ([0, 1000, 0], [0.0, 0.0, 1.0], [64, 128, 192]),
        ([0, -1000, 0], [0.0, 0.0, 1.0], [255, 255, 255]),
        ([0, 0, 1000], [0.0, 1.0, 0.0], [255, 0, 0]),
        ([0, 0, -1000], [0.0, 1.0, 0.0], [0, 0, 255]),
    ] {
        for eye in [[0, 0, 0], [3000, 4000, 5000]] {
            let camera = SceneView::orthographic_with_up(
                CameraPose {
                    eye_millimeters: eye,
                    target_millimeters: std::array::from_fn(|i| eye[i] + direction[i]),
                },
                OrthographicLens {
                    vertical_half_extent_meters: 2.0,
                    near_meters: 0.1,
                    far_meters: 100.0,
                },
                up,
                Viewport {
                    width: 9,
                    height: 3,
                },
            )
            .unwrap();
            let image = render_at(&context, &source, 2.0, &camera);
            for (actual, channel) in image.rgb(4).into_iter().zip(expected) {
                assert!(
                    (actual - 2.0 * f32::from(channel) / 255.0).abs() < 0.002,
                    "dome direction {direction:?}, eye {eye:?}: {:?}",
                    image.rgb(4)
                );
            }
        }
    }
}
pub(super) fn directional_dome() -> RenderScene {
    let mut textures = textures();
    let row = [
        [0, 0, 255, 255],
        [255; 4],
        [255; 4],
        [255, 0, 0, 255],
        [255, 0, 0, 255],
        [64, 128, 192, 255],
        [64, 128, 192, 255],
        [0, 0, 255, 255],
    ];
    let bytes = [[255, 0, 0, 255]; 8]
        .into_iter()
        .chain(row)
        .chain(row)
        .chain([[0, 0, 255, 255]; 8])
        .flatten()
        .collect::<Vec<_>>();
    textures[3] = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 8,
        height: 4,
        mips: 1,
        bytes: bytes.into(),
    };
    let mut sky = atmosphere(0);
    sky.meshes.clear();
    RenderScene::new(SceneParts {
        resources: SceneResources::new(textures.to_vec(), vec![]).unwrap(),
        geometry: SceneGeometry::new(vec![], vec![]).unwrap(),
        terrain: None,
        effects: Some(base_effects(true).with_atmosphere(sky).unwrap()),
    })
    .unwrap()
}
