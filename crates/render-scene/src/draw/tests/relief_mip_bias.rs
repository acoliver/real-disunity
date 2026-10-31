//! The material mip bias reaches the mip-enabled gradient branch of `scene_relief_normal`. The
//! relief normal atlas has its own `rg` encoding per mip level, so the decoded normal tells which
//! level was sampled. The card faces the camera square-on at a texel-to-pixel ratio of exactly
//! 2^k, like the albedo atlas fixtures, so bias -1 at level k must give what bias 0 gives at k-1.
use super::foliage::pass_color;
use super::impostor_mip_bias::atlas_scene_with_normal;
use super::mip_bias::distance_for;
use super::*;
use worldrunner_render_plan::{SceneAtlasSampling, TexelFormat};

/// Encoded `rg` per mip level. `scene_relief_decode` maps them to the normals (0, 0, 1),
/// (0.73, 0, 0.68) and (0, 0.96, 0.28) for levels 0 to 2: far apart in every output channel.
const ENCODINGS: [[u8; 2]; 7] = [
    [128, 128],
    [179, 128],
    [128, 204],
    [128, 128],
    [128, 128],
    [128, 128],
    [128, 128],
];

fn relief_texture() -> SceneTexture {
    let mut bytes = Vec::new();
    for (level, [r, g]) in ENCODINGS.iter().enumerate() {
        let size = 64_usize >> level;
        for _ in 0..size * size {
            bytes.extend_from_slice(&[*r, *g, 128, 255]);
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

/// The world normal the fragment decodes, as 8 bit colour.
fn normal_source() -> String {
    format!(
        "{}\n@fragment fn fs_sample(input: VertexOutput) -> @location(0) vec4<f32> {{ return vec4<f32>(scene_relief_normal(input) * 0.5 + 0.5, 1.0); }}",
        crate::shader::ENVIRONMENT_WGSL
    )
}

fn render_center(context: &GpuContext, source: &str, level: u32, bias: f32) -> [u8; 3] {
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let scene = atlas_scene_with_normal(SceneAtlasSampling::Implicit, Some(relief_texture()));
    let gpu = SceneGpu::upload(context.device(), context.submission(), &scene, &layouts).unwrap();
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

fn largest_difference(a: [u8; 3], b: [u8; 3]) -> u8 {
    a.iter().zip(b).map(|(x, y)| x.abs_diff(y)).max().unwrap()
}

/// Same level selected, so only the hardware LOD fixed point (1/256 of a level) and the 8 bit
/// output rounding differ: at most 0.96 / 256 of normal, half of that in colour, times the sRGB
/// slope (under 0.7 over these values) times 255 is 0.3, plus 0.5 rounding and 0.5 margin: 2.
const SAME_LEVEL: u8 = 2;
/// Adjacent levels differ in the decoded normal by 0.73 (x, levels 0 and 1) and 0.73 and 0.96
/// (levels 1 and 2) along a camera-facing axis; half of that is 0.37 in linear colour, which the
/// sRGB encode (slope above 0.4 here) turns into at least 40 of 255.
const DIFFERENT_LEVEL: u8 = 40;

#[test]
fn a_negative_bias_selects_the_next_finer_relief_normal_level() {
    let context = context();
    let source = normal_source();
    for level in [1_u32, 2] {
        let at = |level, bias| render_center(&context, &source, level, bias);
        let biased = at(level, -1.0);
        let finer = at(level - 1, 0.0);
        let same = at(level, 0.0);
        assert!(
            largest_difference(biased, finer) <= SAME_LEVEL,
            "level {level}: bias -1 {biased:?} is not the unbiased level {} normal {finer:?}",
            level - 1
        );
        assert!(
            largest_difference(biased, same) >= DIFFERENT_LEVEL,
            "level {level}: bias -1 {biased:?} still equals the unbiased normal {same:?}"
        );
    }
}
