use super::specular_tests::{capture_outputs, setup_scene, source};
use crate::{CaptureOutput, RetainedCaptureProducts};
use worldrunner_render_plan::*;

#[test]
fn environment_terms_reconstruct_mesh_hdr_with_constant_uploaded_cube() {
    let scene = environment_scene();
    let (mut host, mut renderer) = setup_scene(scene, 1);
    let products = capture_outputs(
        &mut host,
        &mut renderer,
        vec![
            CaptureOutput::LinearHdr,
            CaptureOutput::DirectDiffuse,
            CaptureOutput::AmbientDiffuse,
            CaptureOutput::EnvironmentSpecular,
            CaptureOutput::SunSpecular,
        ],
    );
    assert_reconstruction(&products);
}

fn environment_scene() -> RenderScene {
    let base = source(0, false);
    let mut textures = base.resources().textures().to_vec();
    let cube = TextureHandle::from_raw(u32::try_from(textures.len()).unwrap());
    textures.push(SceneTexture::SpecularCube(
        SceneSpecularCube::rgba16_float(
            1,
            1,
            [0x4000u16, 0x3c00, 0x3800, 0x3c00]
                .repeat(6)
                .into_iter()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>()
                .into(),
        )
        .unwrap(),
    ));
    let brdf = TextureHandle::from_raw(u32::try_from(textures.len()).unwrap());
    textures.push(SceneTexture::Texels {
        format: TexelFormat::Rg16Float,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: [0x3800u16, 0x3400]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>()
            .into(),
    });
    let effects = base
        .effects()
        .unwrap()
        .clone()
        .with_ssr(SceneSsr {
            max_steps: 1,
            origin_skip_pixels: 4096,
            ..Default::default()
        })
        .unwrap()
        .with_environment(Some(SceneEnvironment {
            cube,
            brdf_lut: brdf,
            convention: SceneSpecularConvention::default(),
            consumers: SceneEnvironmentConsumers::Reflections,
            specular_intensity_micro: 1_000_000,
        }));
    base.with_resources(
        SceneResources::new(textures, base.resources().materials().to_vec()).unwrap(),
        base.geometry().clone(),
        Some(effects),
    )
    .unwrap()
}

fn assert_reconstruction(products: &RetainedCaptureProducts) {
    let terms = [
        products.direct_diffuse().unwrap(),
        products.ambient_diffuse().unwrap(),
        products.environment_specular().unwrap(),
        products.sun_specular().unwrap(),
    ];
    let hdr = products.linear_hdr.as_ref().unwrap();
    let mut covered = 0;
    for (i, pixel) in terms[0].pixels.iter().enumerate() {
        if pixel[3] == 0.0 {
            continue;
        }
        assert!(terms[2].pixels[i][0] > 0.01);
        for channel in 0..3 {
            let sum: f32 = terms.iter().map(|term| term.pixels[i][channel]).sum();
            let expected = hdr.pixels[i][channel];
            // Four half-float terms and their stored sum each contribute at most one ULP.
            assert!(
                (sum - expected).abs() <= 5.0 * expected.abs().max(1.0) / 1024.0,
                "pixel {i}, channel {channel}: {sum} != {expected}"
            );
        }
        covered += 1;
    }
    assert!(covered > 20);
}
