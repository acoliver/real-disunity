//! Masked coverage decides what the depth prepass, the metadata pass and the forward pass keep,
//! and all three must keep the same pixels at a nonzero material mip bias or the prepass would
//! occlude texels the forward pass shades. The albedo has alpha 0 at level 0 and alpha 1 at every
//! other level, and the card spans 32 of 64 pixels at exactly level 1, so bias 0 covers the card
//! and bias -1 reads the transparent level 0 and covers nothing.
use super::foliage::{geometry_sources, pass_depth};
use super::mip_bias::{distance_for, masked_scene, mipped_texture};
use super::*;

fn level_zero_transparent() -> SceneTexture {
    let SceneTexture::Texels {
        format,
        color_space,
        width,
        height,
        mips,
        bytes,
    } = mipped_texture()
    else {
        unreachable!("the mip fixture is made of texels");
    };
    let mut bytes = bytes.to_vec();
    for texel in bytes[..64 * 64 * 4].chunks_exact_mut(4) {
        texel[3] = 0;
    }
    SceneTexture::Texels {
        format,
        color_space,
        width,
        height,
        mips,
        bytes: bytes.into(),
    }
}

fn covered(bytes: &[u8]) -> bool {
    bytes.chunks_exact(4).any(|v| v != 1.0_f32.to_le_bytes())
}

#[test]
fn forward_prepass_and_metadata_keep_the_same_masked_pixels_at_a_nonzero_bias() {
    let context = context();
    let device = context.device();
    let scene = masked_scene(level_zero_transparent());
    let pipelines = ScenePipelines::new(device, TextureFormat::Rgba8Unorm);
    let gpu = SceneGpu::upload(device, context.submission(), &scene, &pipelines).unwrap();
    let [forward, prepass, metadata, _shadow] = geometry_sources();
    for (bias, expect_covered) in [(0.0_f32, true), (-1.0, false)] {
        let mut frame = frame_from(CameraPose {
            eye_millimeters: [0, -distance_for(1), 1000],
            target_millimeters: [0, 0, 1000],
        });
        frame.unlit = true;
        frame.material_mip_bias = bias;
        gpu.set_frame(context.queue(), frame);
        let [reference, prepass, metadata] = [&forward, &prepass, &metadata]
            .map(|source| pass_depth(&context, &gpu, &pipelines, source, "fs_masked", true));
        assert_eq!(
            covered(&reference),
            expect_covered,
            "forward at bias {bias}"
        );
        assert_eq!(prepass, reference, "prepass at bias {bias}");
        assert_eq!(metadata, reference, "metadata at bias {bias}");
    }
}
