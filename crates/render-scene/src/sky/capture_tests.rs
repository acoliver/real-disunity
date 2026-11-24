//! Captures of the celestial mesh draw: a whole frame is encoded through
//! `FrameEncoding::encode_scene` with the scene's celestial meshes recording live and not at all,
//! and the resolved colour of the frame has to differ.
//!
//! The celestial pass draws into its own mesh target and the composite brings its colour into
//! the frame, so a colour difference the composite passes on is seen here. The mesh target's
//! depth is neither copyable nor sampled; it decides only which of the body's own fragments
//! survive, which shows in the colour. The frame depth is read back too, but the celestial pass
//! does not write it (every case reads the same depth on lavapipe). The
//! sun and ambient are lit, so the lighting multipliers and the specular convention reach the
//! image; every case other than the base also has to differ from the base, so a case that
//! does not exercise its axis fails rather than passing twice over the same pixels.
//!
//! The body is normal mapped with a tilted normal and texture coordinates across the quad, so
//! its lighting reads the cotangent frame. That is what `SCENE_RASTER_MIRRORED` reverses, and
//! without it the left-handed case would not reach the constant. The masked case draws a
//! cutout texture whose right half is transparent, so its discard removes texels; with texels
//! that all pass the cutoff, the masked and opaque entry points draw the same image.
//!
//! Covered: one sample; neutral and non-neutral body lighting; anchored; separate surface
//! composition; independent celestial exposure; a non-default specular convention; the
//! left-handed (clockwise) winding; opaque and masked materials. Not covered: four samples,
//! physical (scattering) composition, and wind or any other vertex path celestial meshes do
//! not have.

use super::meshes::Recorder;
use super::scene_tests::{atmosphere, material, read, textures};
use crate::test_support::upload_scene;
use crate::{
    device::GpuContext,
    draw::SceneGpu,
    frame::FrameUniform,
    renderer::ScenePipelines,
    view::{CameraPose, SceneView, Viewport},
};
use worldrunner_render_plan::*;

/// The capture is `SIZE` by `SIZE` pixels.
const SIZE: u32 = 64;

pub(super) struct Case {
    name: &'static str,
    pub(super) sky: SceneAtmosphere,
    convention: SceneSpecularConvention,
    left_handed: bool,
}

impl Case {
    pub(super) fn base() -> Self {
        let mut sky = atmosphere(0);
        // Fifteen degrees off the view axis, so the left-handed mirror is not the same image.
        sky.meshes[0].direction = SceneDirection::new([258_819, 965_926, 0]).unwrap();
        Self {
            name: "base",
            sky,
            convention: SceneSpecularConvention::default(),
            left_handed: false,
        }
    }

    fn sky(name: &'static str, change: impl FnOnce(&mut SceneAtmosphere)) -> Self {
        let mut case = Self::base();
        case.name = name;
        change(&mut case.sky);
        case
    }
}

fn cases() -> Vec<Case> {
    vec![
        Case::base(),
        Case::sky("lighting", |sky| {
            sky.meshes[0].lighting = SceneCelestialLighting::new(2_000_000, 500_000, 250_000)
                .expect("valid celestial lighting");
        }),
        Case::sky("anchored", |sky| {
            sky.meshes[0].anchor_millimeters = Some([0, 9000, 500]);
        }),
        Case::sky("separate", |sky| {
            sky.celestial_composition = SceneCelestialComposition::SeparateSurface;
        }),
        Case::sky("exposure", |sky| {
            sky.celestial_exposure_micro = Some(2_000_000);
        }),
        Case::sky("masked", |sky| {
            sky.meshes[0].material = MaterialHandle::from_raw(2);
        }),
        Case {
            name: "convention",
            convention: SceneSpecularConvention {
                roughness: SceneRoughnessRemap::LinearAlpha,
                visibility: SceneSpecularVisibility::SchlickSmith,
                ..SceneSpecularConvention::default()
            },
            ..Case::base()
        },
        Case {
            name: "left-handed",
            left_handed: true,
            ..Case::base()
        },
    ]
}

/// The texture index of the tilted normal map that [`textures_with_normal`] appends.
const NORMAL_MAP: u32 = 4;
/// The texture index of the cutout albedo that [`textures_with_normal`] appends: green, opaque
/// on the left half and transparent on the right, so a masked body loses half its texels.
const CUTOUT: u32 = 5;

/// The sky tests' textures plus one tangent-space normal tilted toward +U. With it the body's
/// lighting reads the cotangent frame, which the mirrored raster reverses, so the left-handed
/// case reaches `SCENE_RASTER_MIRRORED`.
fn textures_with_normal() -> Vec<SceneTexture> {
    let mut all = textures().to_vec();
    all.push(SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![230, 128, 200, 255].into(),
    });
    all.push(SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 2,
        height: 1,
        mips: 1,
        bytes: vec![0, 255, 0, 255, 0, 255, 0, 0].into(),
    });
    all
}

/// A lit, normal-mapped body material on the albedo texture `albedo`, opaque or masked.
fn body(albedo: u32, alpha_mode: SceneAlphaMode) -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(albedo),
        normal: Some(TextureHandle::from_raw(NORMAL_MAP)),
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: Some(TextureHandle::from_raw(0)),
        roughness_micro: 400_000,
        metalness_micro: 0,
        alpha_mode,
        double_sided: true,
    })
    .unwrap()
}

/// The sky tests' quad facing the camera, with texture coordinates across it so the cotangent
/// frame has a direction.
fn quad() -> SceneMesh {
    let corners = [
        ([-707, 0, -707], [0, 1_000_000]),
        ([707, 0, -707], [1_000_000, 1_000_000]),
        ([707, 0, 707], [1_000_000, 0]),
        ([-707, 0, 707], [0, 0]),
    ];
    let vertices = corners.map(|(position_millimeters, texcoord_micro)| SceneVertex {
        tangent_micro: None,
        position_millimeters,
        normal_micro: [0, -1_000_000, 0],
        texcoord_micro,
    });
    SceneMesh::new(vec![
        SceneMeshLod::new(0, vertices.to_vec(), vec![0, 1, 2, 0, 2, 3], None).unwrap(),
    ])
    .unwrap()
}

/// The sky tests' effects with the sun and the ambient lit, so the body's direct, environment
/// and specular terms are not zero.
fn effects(sky: SceneAtmosphere) -> SceneEffects {
    let grey = SceneColor {
        red: 26,
        green: 26,
        blue: 26,
    };
    SceneEffects::new(SceneEffectsParts {
        water: None,
        sky: SceneSky::ProceduralHemisphere {
            zenith: grey,
            horizon: grey,
        },
        sun: SceneSun {
            direction: SceneDirection::new([0, -707_107, 707_107]).unwrap(),
            irradiance_micro: 3_000_000,
        },
        ambient: SceneAmbient {
            sky_irradiance_micro: 400_000,
            ground_irradiance_micro: 100_000,
        },
        shadows: SceneShadowCascades::new(vec![10_000], 0).unwrap(),
        ssao: None,
        camera: SceneCamera {
            vertical_fov_millidegrees: 60_000,
            near_plane_millimeters: 100,
            far_plane_millimeters: 100_000,
        },
        grading: SceneColorGrading {
            exposure_micro: 1_000_000,
            contrast_micro: 1_000_000,
            saturation_micro: 1_000_000,
        },
        post: vec![],
        anti_alias: SceneAntiAlias::Disabled,
    })
    .unwrap()
    .with_atmosphere(sky)
    .unwrap()
}

pub(super) fn scene(case: &Case) -> RenderScene {
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            textures_with_normal(),
            vec![
                material(1),
                body(2, SceneAlphaMode::Opaque),
                body(
                    CUTOUT,
                    SceneAlphaMode::Mask {
                        cutoff_micro: 500_000,
                    },
                ),
            ],
        )
        .unwrap(),
        geometry: SceneGeometry::new(vec![quad()], vec![]).unwrap(),
        terrain: None,
        effects: Some(effects(case.sky.clone())),
    })
    .unwrap()
}

/// A perspective camera at the origin looking along +Y at the body, Z up.
pub(super) fn view(left_handed: bool) -> SceneView {
    let handedness = if left_handed {
        worldrunner_camera::ViewHandedness::DirectXLeftHanded
    } else {
        worldrunner_camera::ViewHandedness::RightHanded
    };
    SceneView::with_convention(
        CameraPose {
            eye_millimeters: [0, 0, 0],
            target_millimeters: [0, 1000, 0],
        },
        SceneCamera {
            vertical_fov_millidegrees: 90_000,
            near_plane_millimeters: 100,
            far_plane_millimeters: 100_000,
        },
        Viewport {
            width: SIZE,
            height: SIZE,
        },
        worldrunner_camera::ViewConvention { handedness },
    )
    .unwrap()
}

/// The frame's resolved colour and depth.
#[derive(PartialEq)]
pub(super) struct Image {
    color: Vec<u8>,
    depth: Vec<u8>,
}

fn target(device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("celestial capture"),
        size: wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
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

/// Encodes one frame of `gpu` with its celestial meshes, if it has any, recording as `recorder`
/// says.
pub(super) fn render(
    context: &GpuContext,
    (gpu, pipelines): (&mut SceneGpu, &ScenePipelines),
    frame: &FrameUniform,
    recorder: Recorder,
) -> Image {
    use worldrunner_render_post::{AttachmentView, ResourceRef};
    let device = context.device();
    if let Some(meshes) = gpu.celestial_meshes() {
        meshes.recorder.set(recorder);
    }
    let color = target(device, wgpu::TextureFormat::Rgba16Float);
    let depth = target(device, wgpu::TextureFormat::Depth32Float);
    let rows = [SIZE * 8, SIZE * 4];
    let buffers = rows.map(|row| {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("celestial capture readback"),
            size: u64::from(row * SIZE),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        })
    });
    let color_view = AttachmentView::new(ResourceRef::new("celestial color"), &color).unwrap();
    let depth_view = AttachmentView::new(ResourceRef::new("celestial depth"), &depth).unwrap();
    let prepared = context
        .submission()
        .prepare(device, |encoder, _| {
            let mut renderer = crate::compositor::FrameRenderer::new(device);
            renderer
                .encoding()
                .encode_scene(
                    device,
                    encoder,
                    (gpu, pipelines),
                    crate::compositor::ColorDepth {
                        color: &color_view,
                        depth: &depth_view,
                    },
                    *frame,
                )
                .unwrap();
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
                            bytes_per_row: Some(rows[index]),
                            rows_per_image: Some(SIZE),
                        },
                    },
                    texture.size(),
                );
            }
            Ok::<_, std::convert::Infallible>(())
        })
        .unwrap();
    prepared.submit().unwrap();
    Image {
        color: read(device, &buffers[0]),
        depth: read(device, &buffers[1]),
    }
}

/// Captures `case` live and empty, checks the body changed the frame, and returns the live
/// image.
fn compare(context: &GpuContext, case: &Case) -> Image {
    let device = context.device();
    let source = scene(case);
    let pipelines = ScenePipelines::new(device, wgpu::TextureFormat::Rgba16Float, 1).unwrap();
    let mut gpu = upload_scene(device, context.submission(), &source, &pipelines).unwrap();
    assert_eq!(
        gpu.celestial_meshes()
            .map(super::meshes::Meshes::draw_count),
        Some(1),
        "{}: one body",
        case.name
    );
    let mut frame = FrameUniform::new(&view(case.left_handed), source.effects()).unwrap();
    frame.specular_convention = case.convention;
    let mut draw = |recorder| render(context, (&mut gpu, &pipelines), &frame, recorder);
    let nothing = draw(Recorder::Nothing);
    let live = draw(Recorder::Live);
    assert!(
        live.color != nothing.color,
        "{}: the body left the frame as it was",
        case.name
    );
    live
}

#[test]
fn the_celestial_draw_changes_the_frame_in_every_case() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = GpuContext::shared_for_test().unwrap();
    let adapter = context.adapter().get_info().name;
    let mut base = None;
    for case in cases() {
        let live = compare(&context, &case);
        if let Some(base) = &base {
            assert!(
                &live != base,
                "{}: the case draws the base case's image, so it does not exercise its axis",
                case.name
            );
        }
        println!(
            "CELESTIAL-CAPTURE adapter={adapter:?} case={} color={:016x} depth={:016x}",
            case.name,
            digest(&live.color),
            digest(&live.depth)
        );
        if case.name == "base" {
            base = Some(live);
        }
    }
}

fn digest(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}
