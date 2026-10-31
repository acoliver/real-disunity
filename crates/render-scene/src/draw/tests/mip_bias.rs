//! The material mip bias in the frame block selects a finer mip level for implicit-derivative
//! atlas sampling. The texture has a constant colour per mip level and the card faces the camera
//! square-on at a texel-to-pixel ratio of exactly 2^k, so the implicit LOD is the integer k and
//! the trilinear blend has no second level in it.
use super::foliage::pass_color;
use super::*;
use worldrunner_render_plan::TexelFormat;

pub(super) const LEVELS: [[u8; 3]; 7] = [
    [230, 40, 40],
    [40, 230, 40],
    [40, 40, 230],
    [230, 230, 40],
    [230, 40, 230],
    [40, 230, 230],
    [128, 128, 128],
];
/// Output rounding to 8 bit (0.5), the residual LOD error of the hardware times the largest
/// adjacent-level step in the encoded output (1/256 LOD fixed point precision, at most 140 of 255
/// between these colours, 0.55) and 0.95 of margin for the sRGB encode: 2 of 255.
const BOUND: f64 = 2.0;

pub(super) fn mipped_texture() -> SceneTexture {
    let mut bytes = Vec::new();
    for (level, color) in LEVELS.iter().enumerate() {
        let size = 64_usize >> level;
        for _ in 0..size * size {
            bytes.extend_from_slice(&[color[0], color[1], color[2], 255]);
        }
    }
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 64,
        height: 64,
        mips: 7,
        bytes: bytes.into(),
    }
}

fn plain_scene() -> RenderScene {
    let surface = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
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
    .unwrap();
    RenderScene::new(SceneParts {
        resources: SceneResources::new(vec![mipped_texture()], vec![surface]).unwrap(),
        geometry: SceneGeometry::new(
            vec![card(None)],
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

/// Camera distance in millimeters at which the 2 m card spans `64 / 2^level` pixels of the 64
/// pixel target under the 60 degree vertical field of view: `2^level / tan(30 degrees)` meters,
/// 3464.10 and 6928.20, rounded to a millimeter. The LOD moves by under 5e-5.
pub(super) fn distance_for(level: u32) -> i64 {
    match level {
        1 => 3464,
        2 => 6928,
        _ => unreachable!("only levels 1 and 2 have a fixture distance"),
    }
}

fn render_center(context: &GpuContext, source: &str, level: u32, bias: f32) -> [u8; 3] {
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let gpu = SceneGpu::upload(
        context.device(),
        context.submission(),
        &plain_scene(),
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

fn sample_source() -> String {
    format!(
        "{}\n@fragment fn fs_sample(input: VertexOutput) -> @location(0) vec4<f32> {{ return scene_atlas_sample(albedo_texture,input); }}",
        crate::shader::ENVIRONMENT_WGSL
    )
}

fn encoded(color: [u8; 3]) -> [f64; 3] {
    color.map(|channel| {
        let linear = f64::from(channel) / 255.0;
        (1.055 * linear.powf(1.0 / 2.4) - 0.055) * 255.0
    })
}

pub(super) fn assert_level(actual: [u8; 3], level: usize, what: &str) {
    for (got, wanted) in actual.iter().zip(encoded(LEVELS[level])) {
        assert!(
            (f64::from(*got) - wanted).abs() <= BOUND,
            "{what}: {actual:?} is not level {level} {:?}",
            encoded(LEVELS[level])
        );
    }
}

#[test]
fn a_negative_bias_selects_the_next_finer_mip_level() {
    let context = context();
    let source = sample_source();
    for level in [1_u32, 2] {
        let k = level as usize;
        assert_level(render_center(&context, &source, level, 0.0), k, "bias 0");
        assert_level(
            render_center(&context, &source, level, -1.0),
            k - 1,
            "bias -1",
        );
    }
}

#[test]
fn bias_zero_matches_the_unbiased_stock_sample_on_one_device() {
    let context = context();
    let source = sample_source();
    let stock = source.replace(
        "textureSampleBias(map, material_sampler, input.texcoord, frame.lighting_options.w)",
        "textureSample(map, material_sampler, input.texcoord)",
    );
    assert_ne!(
        stock, source,
        "the reference must differ in the sampling call"
    );
    for level in [1_u32, 2] {
        assert_eq!(
            render_center(&context, &source, level, 0.0),
            render_center(&context, &stock, level, 0.0),
            "level {level}"
        );
    }
}
