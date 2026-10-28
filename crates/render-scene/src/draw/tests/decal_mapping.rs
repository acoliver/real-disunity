use super::{capture, decal, source, srgb, texel};
use crate::draw::tests::context;
use glam::Vec3;
use worldrunner_render_plan::{SceneDecalAngle, SceneResources, SceneRotation, SceneTexture};

#[test]
fn transformed_uv_selects_authored_crop_independently_of_hull_size() {
    let context = context();
    let base = source(false, false);
    let mut textures = base.resources().textures().to_vec();
    let mut texture = texel([0; 4]);
    if let SceneTexture::Texels {
        width,
        height,
        bytes,
        ..
    } = &mut texture
    {
        *width = 2;
        *height = 2;
        *bytes = vec![
            255, 0, 0, 255, 0, 0, 255, 255, 0, 255, 0, 255, 255, 255, 255, 255,
        ]
        .into();
    }
    textures[1] = texture;
    let scene = base
        .with_resources(
            SceneResources::new(textures, base.resources().materials().to_vec()).unwrap(),
            base.geometry().clone(),
            None,
        )
        .unwrap();
    let mut d = decal();
    d.edge_fade_micro = 0;
    d.mapping.uv_scale_micro = [0; 2];
    d.mapping.uv_offset_micro = [250_000, 750_000];
    for size in [2000, 4000] {
        d.dimensions_millimeters = [size; 3];
        let pixels = capture(&context, &scene.with_decals(vec![d]).unwrap(), 1);
        for (x, y) in [(32, 32), (28, 32), (36, 32)] {
            let offset = (y * 64 + x) * 4;
            assert_eq!(&pixels[offset..offset + 3], &[0, 255, 0]);
        }
    }
}

#[test]
fn power_fade_and_double_sided_projection_match_plane_and_slope_pixel_oracles() {
    let context = context();
    for slope in [false, true] {
        let scene = source(slope, false);
        let angle = if slope {
            1.0_f32 / 1.25_f32.sqrt()
        } else {
            1.0
        };
        for double_sided in [false, true] {
            let mut d = decal();
            d.dimensions_millimeters = [4000; 3];
            d.edge_fade_micro = 0;
            d.transform.rotation = SceneRotation {
                x: 1_000_000,
                y: 0,
                z: 0,
                w: 0,
            };
            d.mapping.angle = SceneDecalAngle::Power {
                threshold_micro: 1_000_000,
                exponent_micro: 2_400_000,
            };
            d.mapping.double_sided = double_sided;
            let weight = if double_sided {
                angle.powf(2.4) * 128.0 / 255.0
            } else {
                0.0
            };
            let expected =
                Vec3::new(51.0, 102.0, 153.0).lerp(Vec3::new(204.0, 51.0, 26.0), weight) / 255.0;
            for samples in [1, 4] {
                let pixels = capture(&context, &scene.with_decals(vec![d]).unwrap(), samples);
                let offset = (32 * 64 + 32) * 4;
                for (actual, expected) in pixels[offset..offset + 3].iter().zip(expected.to_array())
                {
                    assert!((f32::from(*actual) - srgb(expected) * 255.0).abs() < 1.5);
                }
            }
        }
    }
}
