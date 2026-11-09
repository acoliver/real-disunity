//! A common producer isolates the forwarding depth/stencil policy from receiver derivatives.
use super::*;
use crate::compositor::FrameRenderer;
use crate::scene_metadata::receiver::{ReceiverLayout, ReceiverReference};
use worldrunner_render_plan::{SceneImpostor, SceneImpostorLayout, TexelFormat};
use worldrunner_render_post::ToneMap;

const OFFSET: [i64; 2] = [973_000, 887_000];
const TERRAIN_SIDE: u32 = 129;
#[path = "receiver_gradient_backing.rs"]
mod backing;
#[path = "receiver_gradient_control.rs"]
mod streamed;

fn receiver_pose() -> CameraPose {
    let mut pose = CameraPose {
        eye_millimeters: [0, -8_000, 2_000],
        target_millimeters: [0, 50_000, 500],
    };
    for (axis, offset) in OFFSET.into_iter().enumerate() {
        pose.eye_millimeters[axis] += offset;
        pose.target_millimeters[axis] += offset;
    }
    pose
}

fn sloped(cards: bool) -> RenderScene {
    let source = super::source();
    let ground = source.terrain().unwrap();
    let terrain = SceneTerrain::new(SceneTerrainParts {
        columns: TERRAIN_SIDE,
        rows: TERRAIN_SIDE,
        origin_east_millimeters: OFFSET[0] - i64::from(TERRAIN_SIDE - 1) * 1_000,
        origin_north_millimeters: OFFSET[1] - i64::from(TERRAIN_SIDE - 1) * 1_000,
        sample_spacing_millimeters: 2_000,
        heights_millimeters: (0_u32..TERRAIN_SIDE * TERRAIN_SIDE)
            .map(|i| {
                let x = i % TERRAIN_SIDE;
                let y = i / TERRAIN_SIDE;
                i32::try_from((x * 173 + y * 257 + x * y * 83) % 900).unwrap() - 350
            })
            .collect(),
        surface_indices: vec![0; (TERRAIN_SIDE * TERRAIN_SIDE) as usize],
        surfaces: ground.surfaces().to_vec(),
        lod_policy: ground.lod_policy(),
    })
    .unwrap();
    let source = source
        .with_terrain(Some(std::sync::Arc::new(terrain)))
        .unwrap();
    let mut parts = effect_parts();
    parts.sun.direction = SceneDirection::new([480_000, -640_000, 600_000]).unwrap();
    parts.anti_alias = SceneAntiAlias::taa(900_000);
    parts.ssao = Some(crate::ssr::neutral_ao());
    parts.shadows =
        SceneShadowCascades::camera_splits(vec![12_000, 40_000, 150_000], 300_000).unwrap();
    let effects = SceneEffects::new(parts)
        .unwrap()
        .with_ssr(*source.effects().unwrap().ssr().unwrap())
        .unwrap();
    let source = source
        .with_objects_and_effects(Vec::new(), Some(effects))
        .unwrap();
    if cards {
        overlapping_cards(&source)
    } else {
        source
    }
}

fn relief_texture() -> SceneTexture {
    let bytes: Vec<_> = (0_u32..32 * 8)
        .flat_map(|i| {
            let value = u8::try_from(((i % 8) * 29 + (i / 32) * 41) % 192 + 32).unwrap();
            [value; 4]
        })
        .collect();
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 32,
        height: 8,
        mips: 1,
        bytes: bytes.into(),
    }
}

fn overlapping_cards(source: &RenderScene) -> RenderScene {
    let card = impostor::relief_scene();
    let mut textures = source.resources().textures().to_vec();
    let texture = u32::try_from(textures.len()).unwrap();
    textures.extend_from_slice(card.resources().textures());
    textures[usize::try_from(texture + 2).unwrap()] = relief_texture();
    let mut materials = source.resources().materials().to_vec();
    let material = u32::try_from(materials.len()).unwrap();
    let atlas = SceneImpostor::new(SceneImpostorLayout::AzimuthElevation, [4, 1], 0, true)
        .unwrap()
        .with_relief(TextureHandle::from_raw(texture + 2), 100)
        .unwrap();
    materials.push(
        SceneMaterial::new(SceneMaterialParts {
            albedo: TextureHandle::from_raw(texture),
            normal: Some(TextureHandle::from_raw(texture + 1)),
            roughness_metallic: None,
            ambient_occlusion: None,
            self_illumination: None,
            roughness_micro: 700_000,
            metalness_micro: 0,
            alpha_mode: SceneAlphaMode::Mask {
                cutoff_micro: 500_000,
            },
            double_sided: true,
        })
        .unwrap()
        .with_impostor(atlas)
        .with_translucency(700_000)
        .unwrap(),
    );
    let mut meshes = source.geometry().meshes().to_vec();
    let mesh = u32::try_from(meshes.len()).unwrap();
    meshes.push(card.geometry().meshes()[0].clone());
    let mut objects = source.geometry().objects().to_vec();
    for mut position in (0..17 * 17).map(|index| {
        [
            i64::from(index % 17) * 500 - 4_000,
            i64::from(index / 17) * 500 - 4_000,
            350 + i64::from((index * 13) % 7) * 50,
        ]
    }) {
        position[0] += OFFSET[0];
        position[1] += OFFSET[1];
        objects.push(SceneObject::new(
            SceneTransform {
                position_millimeters: position,
                ..identity()
            },
            MeshHandle::from_raw(mesh),
            MaterialHandle::from_raw(material),
        ));
    }
    RenderScene::new(SceneParts {
        resources: SceneResources::new(textures, materials).unwrap(),
        geometry: SceneGeometry::new(meshes, objects).unwrap(),
        terrain: source.terrain().cloned(),
        effects: source.effects().cloned(),
    })
    .unwrap()
}

fn frame(
    context: &GpuContext,
    source: &RenderScene,
    reference: Option<ReceiverReference>,
    bias: f32,
) -> Vec<Vec<u8>> {
    frame_at_pose(context, source, reference, bias, receiver_pose())
}

fn frame_at_pose(
    context: &GpuContext,
    source: &RenderScene,
    reference: Option<ReceiverReference>,
    bias: f32,
    pose: CameraPose,
) -> Vec<Vec<u8>> {
    let device = context.device();
    let reflection = source.effects().unwrap().ssr().is_some();
    let size = if !reflection || reference.is_some_and(ReceiverReference::probe) {
        [1453, 921]
    } else {
        [257, 241]
    };
    let mut renderer = FrameRenderer::hdr(
        context,
        size,
        COLOR_FORMAT,
        1,
        ToneMap::Reinhard,
        source.effects(),
    )
    .unwrap();
    renderer.receiver_reference = reference;
    let build: fn(&GpuContext, &RenderScene, _, _, _) -> _ = if reflection {
        crate::shadows::ShadowedScene::with_taa_velocity
    } else {
        crate::shadows::ShadowedScene::with_target
    };
    let mut shadowed = build(
        context,
        source,
        crate::shadows::DirectionalShadowSettings {
            resolution: 256,
            normal_bias_meters: bias,
            ..super::shadow_settings()
        },
        TextureFormat::Rgba16Float,
        1,
    )
    .unwrap();
    let pipelines = if reflection {
        ScenePipelines::with_ssao_taa_velocity(device, TextureFormat::Rgba16Float, 1)
    } else {
        ScenePipelines::with_ssao(device, TextureFormat::Rgba16Float, 1)
    }
    .unwrap();
    let mut gpu = SceneGpu::upload(device, context.submission(), source, &pipelines).unwrap();
    let mut products = draw_receiver_frame(
        context,
        &mut renderer,
        (&mut gpu, &pipelines),
        &mut shadowed,
        source,
        pose,
    );
    assert_streamed_coverage(source, &mut shadowed);
    assert_runtime_receiver(&renderer, &mut shadowed);
    let payload = renderer
        .ssao
        .as_ref()
        .unwrap()
        .receiver
        .as_ref()
        .expect("paired shadows require stored displacements");
    assert_eq!(
        payload.layout,
        if bias != 0.0 && gpu.has_transmission() {
            ReceiverLayout::Opposing
        } else {
            ReceiverLayout::Front
        }
    );
    products.extend(
        payload
            .textures
            .iter()
            .map(|texture| read_payload(context, texture)),
    );
    products
}

fn draw_receiver_frame(
    context: &GpuContext,
    renderer: &mut FrameRenderer,
    gpu: (&mut SceneGpu, &ScenePipelines),
    shadowed: &mut crate::shadows::ShadowedScene,
    source: &RenderScene,
    pose: CameraPose,
) -> Vec<Vec<u8>> {
    let terrain = source.terrain().unwrap();
    let levels = terrain.streaming_policy().map_or(0, |policy| {
        (terrain.columns() - 1).ilog2() - policy.chunk_intervals().ilog2()
    });
    let (gpu, pipelines) = gpu;
    // Streaming refines one quadtree level per frame; compare settled geometry.
    let mut products = Vec::new();
    for _ in 0..=levels {
        products = super::draw_frame_at(
            context,
            renderer,
            (&mut *gpu, pipelines),
            Some(&mut *shadowed),
            (source, 0, 0.125),
            pose,
            worldrunner_camera::ViewConvention {
                handedness: worldrunner_camera::ViewHandedness::DirectXLeftHanded,
            },
        );
    }
    products
}

fn assert_streamed_coverage(source: &RenderScene, shadowed: &mut crate::shadows::ShadowedScene) {
    if source.terrain().unwrap().streaming_policy().is_some() {
        let statistics = shadowed
            .scene_and_pipelines_mut()
            .0
            .statistics()
            .terrain_geometry;
        let Some(crate::draw::TerrainGeometryStatistics::Streaming(stats)) = statistics else {
            panic!("the receiver fixture requires streaming geometry");
        };
        println!("STREAMED_RECEIVERS {stats:?}");
        assert!(stats.complete, "all selected chunk planes must be resident");
        assert!(stats.drawn.count > 1, "multiple chunk planes must be drawn");
    }
}

fn assert_runtime_receiver(renderer: &FrameRenderer, shadowed: &mut crate::shadows::ShadowedScene) {
    let (_, pipelines) = shadowed.scene_and_pipelines_mut();
    let selected = renderer.receiver_pipelines(pipelines);
    let stats = selected.terrain_runtime_stats();
    assert!(
        stats.pipelines_built > 0,
        "receiver terrain must draw through the runtime"
    );
    assert!(
        stats.sets_prepared > 0,
        "receiver terrain must prepare its material set"
    );
}

fn read_payload(context: &GpuContext, texture: &Texture) -> Vec<u8> {
    let row = texture.width() * texture.format().block_copy_size(None).unwrap();
    let stride = row.div_ceil(256) * 256;
    let size = u64::from(stride)
        * u64::from(texture.height())
        * u64::from(texture.depth_or_array_layers());
    let buffer = context.device().create_buffer(&BufferDescriptor {
        label: Some("receiver payload bits"),
        size,
        usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = context.submission().begin(context.device()).unwrap();
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        TexelCopyBufferInfo {
            buffer: &buffer,
            layout: TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(texture.height()),
            },
        },
        texture.size(),
    );
    encoder.finish().submit().unwrap();
    map_pixels(context.device(), &buffer)
        .chunks_exact(stride as usize)
        .flat_map(|row_bytes| row_bytes[..row as usize].iter().copied())
        .collect()
}

#[test]
fn gpu_receiver_payload_preserves_all_attachments_across_forward_depth_policies() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut terrain_depth = Vec::new();
    for cards in [false, true] {
        let source = sloped(cards);
        for bias in [0.0, 0.012] {
            let shared = frame(
                &context,
                &source,
                Some(ReceiverReference::StoredShared),
                bias,
            );
            let reuse = frame(&context, &source, None, bias);
            let independent = frame(
                &context,
                &source,
                Some(ReceiverReference::StoredIndependent),
                bias,
            );
            assert!(
                independent == reuse,
                "independent stored receivers must preserve every attachment: cards={cards} bias={bias}"
            );
            if bias == 0.0 {
                if cards {
                    assert!(
                        shared[1].iter().zip(&terrain_depth).any(|(a, b)| a != b),
                        "overlapping relief cards must change visible depth"
                    );
                } else {
                    terrain_depth.clone_from(&shared[1]);
                }
            }
            assert_eq!(shared.len(), reuse.len());
            for (attachment, (a, b)) in shared.iter().zip(&reuse).enumerate() {
                assert_eq!(a.len(), b.len(), "attachment={attachment}");
                let differences = a.iter().zip(b).filter(|(x, y)| x != y).count();
                assert_eq!(
                    differences, 0,
                    "cards={cards} bias={bias} attachment={attachment}"
                );
            }
        }
    }
}

#[test]
fn gpu_receiver_payload_fixture_detects_local_derivative_depth_state_changes() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let source = sloped(true);
    let shared = frame(&context, &source, Some(ReceiverReference::ProbeShared), 0.0);
    let reuse = frame(&context, &source, Some(ReceiverReference::ProbeReuse), 0.0);
    let black = frame(&context, &source, Some(ReceiverReference::BlackShared), 0.0);
    assert!(
        shared[10] != black[10] || shared[11] != black[11],
        "the displacement probe must affect the environment/factor MRTs"
    );
    let stored = frame(
        &context,
        &source,
        Some(ReceiverReference::StoredProbeReuse),
        0.0,
    );
    for reference in [
        ReceiverReference::StoredProbeShared,
        ReceiverReference::StoredProbeIndependent,
    ] {
        assert!(
            frame(&context, &source, Some(reference), 0.0) == stored,
            "stored displacements must remove the probe's depth-policy dependence: {reference:?}"
        );
    }
    let independent = frame(
        &context,
        &source,
        Some(ReceiverReference::ProbeIndependent),
        0.0,
    );
    let one_time = stored
        .iter()
        .zip(&reuse)
        .map(|(a, b)| a.iter().zip(b).filter(|(x, y)| x != y).count())
        .sum::<usize>();
    println!("LOCAL_VS_STORED differing_attachment_bytes={one_time}");
    let payload_start = shared.len() - 2;
    assert_eq!(
        shared[payload_start..],
        reuse[payload_start..],
        "the producer must not change in the local-derivative control"
    );
    let different_bytes = [&shared, &independent]
        .into_iter()
        .map(|reference| {
            reference[..payload_start]
                .iter()
                .zip(&reuse[..payload_start])
                .map(|(a, b)| a.iter().zip(b).filter(|(x, y)| x != y).count())
                .sum::<usize>()
        })
        .sum::<usize>();
    println!("LOCAL_PROBE_CONTROL differing_attachment_bytes={different_bytes}");
    let different_bytes = different_bytes + streamed::control(&context);
    assert!(
        different_bytes > 0,
        "the displacement probe must detect local derivative depth-policy changes"
    );
}

#[test]
fn gpu_receiver_payload_allocation_tracks_live_receivers_samples_and_reservation() {
    use crate::compositor::ReceiverGradientReservation;
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let device = context.device();
    let source = sloped(true);
    let pipelines =
        ScenePipelines::with_ssao_taa_velocity(device, TextureFormat::Rgba16Float, 1).unwrap();
    let gpu = SceneGpu::upload(device, context.submission(), &source, &pipelines).unwrap();
    assert!(gpu.has_transmission());
    let mut plain = FrameRenderer::new(device);
    plain
        .prepare_receiver_gradients(device, &gpu, 0.012)
        .unwrap();
    assert!(plain.ssao.is_none());
    let mut parts = effect_parts();
    parts.ssao = Some(crate::ssr::neutral_ao());
    let effects = SceneEffects::new(parts).unwrap();
    for samples in [1, 4] {
        let mut renderer = FrameRenderer::hdr(
            &context,
            SIZE,
            COLOR_FORMAT,
            samples,
            ToneMap::Reinhard,
            Some(&effects),
        )
        .unwrap();
        assert!(renderer.ssao.is_some());
        if samples == 4 {
            renderer
                .prepare_receiver_gradients(device, &gpu, 0.012)
                .unwrap();
            assert!(renderer.ssao.as_ref().unwrap().receiver.is_none());
            continue;
        }
        renderer.receiver_gradients = ReceiverGradientReservation::None;
        assert!(
            renderer
                .prepare_receiver_gradients(device, &gpu, 0.0)
                .is_err()
        );
        assert!(renderer.ssao.as_ref().unwrap().receiver.is_none());
        renderer.receiver_gradients = ReceiverGradientReservation::Front;
        assert!(
            renderer
                .prepare_receiver_gradients(device, &gpu, 0.012)
                .is_err()
        );
        renderer.receiver_gradients = ReceiverGradientReservation::Opposing;
        renderer
            .prepare_receiver_gradients(device, &gpu, 0.0)
            .unwrap();
        assert_eq!(
            renderer
                .ssao
                .as_ref()
                .unwrap()
                .receiver
                .as_ref()
                .unwrap()
                .layout,
            ReceiverLayout::Front
        );
        renderer.receiver_gradients = ReceiverGradientReservation::Front;
        assert!(
            renderer
                .prepare_receiver_gradients(device, &gpu, 0.012)
                .is_err()
        );
        renderer.receiver_gradients = ReceiverGradientReservation::Opposing;
        renderer
            .prepare_receiver_gradients(device, &gpu, 0.012)
            .unwrap();
        assert_eq!(
            renderer
                .ssao
                .as_ref()
                .unwrap()
                .receiver
                .as_ref()
                .unwrap()
                .layout,
            ReceiverLayout::Opposing
        );
        let empty = source
            .with_terrain(None)
            .unwrap()
            .with_objects_and_effects(Vec::new(), source.effects().cloned())
            .unwrap();
        let empty = SceneGpu::upload(device, context.submission(), &empty, &pipelines).unwrap();
        assert!(!empty.has_shadow_receivers());
        renderer
            .prepare_receiver_gradients(device, &empty, 0.012)
            .unwrap();
        assert!(renderer.ssao.as_ref().unwrap().receiver.is_none());
    }
}
