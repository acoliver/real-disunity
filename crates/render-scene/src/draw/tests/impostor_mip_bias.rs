//! The material mip bias reaches the impostor atlas sites that derive their level from the
//! footprint: `scene_atlas_sample` (mip-enabled gradient branch) and `scene_atlas_first`. The
//! atlas holds one cell with a constant colour per mip level and the card faces the camera
//! square-on at a texel-to-pixel ratio of exactly 2^k, so the footprint LOD is the integer k.
use super::foliage::pass_color;
use super::mip_bias::{assert_level, distance_for, mipped_texture};
use super::*;
use worldrunner_render_plan::{SceneAtlasSampling, SceneImpostor, SceneImpostorLayout};

fn atlas_scene(sampling: SceneAtlasSampling) -> RenderScene {
    atlas_scene_with_normal(sampling, None)
}

/// The same card with an optional relief normal atlas as texture 1.
pub(super) fn atlas_scene_with_normal(
    sampling: SceneAtlasSampling,
    normal: Option<SceneTexture>,
) -> RenderScene {
    let impostor = SceneImpostor::new(SceneImpostorLayout::AzimuthElevation, [1, 1], 0, false)
        .unwrap()
        .with_sampling(sampling);
    let surface = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: normal.as_ref().map(|_| TextureHandle::from_raw(1)),
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 500_000,
        metalness_micro: 100_000,
        alpha_mode: SceneAlphaMode::Mask {
            cutoff_micro: 500_000,
        },
        double_sided: true,
    })
    .unwrap()
    .with_impostor(impostor);
    let textures = std::iter::once(mipped_texture()).chain(normal).collect();
    RenderScene::new(SceneParts {
        resources: SceneResources::new(textures, vec![surface]).unwrap(),
        geometry: SceneGeometry::new(
            vec![card(Some(BillboardPolicy::YawLocked))],
            vec![SceneObject::new(
                identity(),
                MeshHandle::from_raw(0),
                MaterialHandle::from_raw(0),
            )],
        )
        .unwrap(),
        terrain: None,
        effects: None,
    })
    .unwrap()
}

fn render_center(
    context: &GpuContext,
    sampling: SceneAtlasSampling,
    source: &str,
    level: u32,
    bias: f32,
) -> [u8; 3] {
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let gpu = SceneGpu::upload(
        context.device(),
        context.submission(),
        &atlas_scene(sampling),
        &layouts,
    )
    .unwrap();
    let mut frame = frame_from(CameraPose {
        eye_millimeters: [0, -distance_for(level), 1000],
        target_millimeters: [0, 0, 1000],
    });
    frame.unlit = true;
    frame.material_mip_bias = bias;
    gpu.set_frame(context.queue(), frame);
    let image = pass_color(context, &gpu, &layouts, source, "fs_sample");
    let center = usize::try_from((TARGET / 2 * TARGET + TARGET / 2) * 4).unwrap();
    [image[center], image[center + 1], image[center + 2]]
}

/// Both sites, by the expression the fragment returns.
const SITES: [&str; 2] = [
    "scene_atlas_sample(albedo_texture,input)",
    "scene_atlas_first(albedo_texture,input)",
];

fn site_source(site: &str) -> String {
    format!(
        "{}\n@fragment fn fs_sample(input: VertexOutput) -> @location(0) vec4<f32> {{ return {site}; }}",
        crate::shader::ENVIRONMENT_WGSL
    )
}

#[test]
fn a_negative_bias_selects_the_next_finer_level_of_a_mip_enabled_atlas() {
    let context = context();
    for site in SITES {
        let source = site_source(site);
        for level in [1_u32, 2] {
            let k = level as usize;
            let at =
                |bias| render_center(&context, SceneAtlasSampling::Implicit, &source, level, bias);
            assert_level(at(0.0), k, &format!("{site} bias 0"));
            assert_level(at(-1.0), k - 1, &format!("{site} bias -1"));
        }
    }
}

#[test]
fn a_fixed_level_zero_atlas_ignores_the_bias() {
    let context = context();
    for site in SITES {
        let source = site_source(site);
        for bias in [0.0_f32, -1.0, -2.0] {
            assert_level(
                render_center(&context, SceneAtlasSampling::MipZero, &source, 2, bias),
                0,
                &format!("{site} bias {bias}"),
            );
        }
    }
}

#[test]
fn bias_zero_matches_the_unbiased_impostor_sampling_on_one_device() {
    let context = context();
    for site in SITES {
        let source = site_source(site);
        let stock = source
            .replace(
                "textureSampleBias(map, material_sampler, input.texcoord, frame.lighting_options.w)",
                "textureSample(map, material_sampler, input.texcoord)",
            )
            .replace(
                "textureSampleBias(map, material_sampler, coord, frame.lighting_options.w)",
                "textureSample(map, material_sampler, coord)",
            )
            .replace("exp2(frame.lighting_options.w)", "1.0");
        assert_ne!(stock, source, "the reference must differ in the sampling");
        for level in [1_u32, 2] {
            let sampling = SceneAtlasSampling::Implicit;
            assert_eq!(
                render_center(&context, sampling, &source, level, 0.0),
                render_center(&context, sampling, &stock, level, 0.0),
                "{site} level {level}"
            );
        }
    }
}
