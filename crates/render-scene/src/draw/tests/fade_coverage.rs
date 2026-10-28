#[test]
fn hashed_distance_fade_keeps_stock_rendered_bytes() {
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let base = threshold_scene(false, false, [0, 0]);
    let material = base.resources().materials()[0].with_fade_coverage(SceneFadeCoverage::Hashed);
    let scene = RenderScene::new(SceneParts {
        resources: SceneResources::new(base.resources().textures().to_vec(), vec![material])
            .unwrap(),
        geometry: base.geometry().clone(),
        terrain: None,
        effects: base.effects().cloned(),
    })
    .unwrap();
    let gpu = SceneGpu::upload(context.device(), context.submission(), &scene, &layouts).unwrap();
    gpu.set_frame(context.queue(), frame_from(pose()));
    let stock = crate::shader::ENVIRONMENT_WGSL.replace(
        "scene_fade_covered(input.clip_position.xy",
        "scene_card_covered(input.clip_position.xy",
    );
    assert_eq!(
        super::foliage::pass_color(&context, &gpu, &layouts, &stock, "fs_opaque"),
        super::foliage::pass_color(
            &context,
            &gpu,
            &layouts,
            crate::shader::ENVIRONMENT_WGSL,
            "fs_opaque"
        )
    );
}

use super::foliage::{geometry_sources, pass_depth};
use super::*;
use worldrunner_render_plan::{
    SceneFadeBand, SceneFadeCoverage, SceneFadeQuantization, SceneImpostor, SceneImpostorLayout,
    TexelFormat,
};

fn threshold_scene(impostor: bool, quantized: bool, jitter: [i32; 2]) -> RenderScene {
    let base = card_scene(Some(BillboardPolicy::YawLocked), identity().rotation);
    let mut textures = base.resources().textures().to_vec();
    textures.push(SceneTexture::Texels {
        format: TexelFormat::R8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 2,
        height: 2,
        mips: 1,
        bytes: vec![0, 119, 120, 255].into(),
    });
    let mut material =
        base.resources().materials()[0].with_fade_coverage(SceneFadeCoverage::Threshold {
            texture: TextureHandle::from_raw(1),
            jitter_pixels: jitter,
        });
    if quantized {
        material = material.with_fade_quantization(SceneFadeQuantization::Byte);
    }
    if impostor {
        material = material.with_impostor(
            SceneImpostor::new(SceneImpostorLayout::AzimuthElevation, [1, 1], 0, false).unwrap(),
        );
    }
    let object = base.geometry().objects()[0]
        .with_fade(SceneFadeBand::new(2000, 6000).unwrap().with_terminal_fade());
    RenderScene::new(SceneParts {
        resources: SceneResources::new(textures, vec![material]).unwrap(),
        geometry: SceneGeometry::new(base.geometry().meshes().to_vec(), vec![object]).unwrap(),
        terrain: None,
        effects: base.effects().cloned(),
    })
    .unwrap()
}

#[test]
fn threshold_coverage_matches_point_repeat_oracle_in_every_pass_and_impostors() {
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    for (impostor, quantized, jitter) in [
        (false, false, [0, 0]),
        (false, true, [1, -2]),
        (true, true, [-3, 4]),
    ] {
        let scene = threshold_scene(impostor, quantized, jitter);
        let gpu =
            SceneGpu::upload(context.device(), context.submission(), &scene, &layouts).unwrap();
        gpu.set_frame(
            context.queue(),
            frame_from(CameraPose {
                eye_millimeters: [0, -4000, 1000],
                target_millimeters: [0, 0, 1000],
            }),
        );
        let mut factor = (6.0_f64 - 17.0_f64.sqrt()) / 4.0;
        if quantized {
            factor = (factor * 255.0).trunc() / 255.0;
        }
        for (index, source) in geometry_sources().into_iter().enumerate() {
            let pixels = pass_depth(
                &context,
                &gpu,
                &layouts,
                &source,
                if index == 3 {
                    "fs_depth_opaque"
                } else {
                    "fs_opaque"
                },
                index != 3,
            );
            for y in 28_i32..36 {
                for x in 28_i32..36 {
                    let cell = usize::try_from(
                        (y - jitter[1]).rem_euclid(2) * 2 + (x - jitter[0]).rem_euclid(2),
                    )
                    .unwrap();
                    let expected = factor >= f64::from([0_u8, 119, 120, 255][cell]) / 255.0;
                    let offset = (usize::try_from(y).unwrap() * TARGET as usize
                        + usize::try_from(x).unwrap())
                        * 4;
                    let actual =
                        f32::from_le_bytes(pixels[offset..offset + 4].try_into().unwrap()) < 1.0;
                    assert_eq!(
                        actual, expected,
                        "pass={index}, pixel={x},{y}, impostor={impostor}"
                    );
                }
            }
        }
    }
}

#[test]
fn default_fade_policy_preserves_uniform_prefix_and_pixels() {
    let context = context();
    let base = scene(SceneAlphaMode::Opaque, true, 1);
    let stock = base.resources().materials()[0];
    let explicit = stock
        .with_fade_coverage(SceneFadeCoverage::Hashed)
        .with_fade_quantization(SceneFadeQuantization::Continuous);
    assert_eq!(
        crate::pipeline::MaterialUniform::from_material(&stock).to_bytes(),
        crate::pipeline::MaterialUniform::from_material(&explicit).to_bytes()
    );
    assert_eq!(
        render(&context, &base),
        render(&context, &build(white_texture(), &explicit, quad(), 1))
    );
}
