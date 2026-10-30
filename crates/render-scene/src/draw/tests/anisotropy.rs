use super::resident_sampling::{grazing_frame, plane};
use super::*;
use crate::sampling::tests::checker;
use worldrunner_render_plan::TextureAnisotropy;

fn horizontal_energy(pixels: &[u8]) -> u64 {
    (12..52)
        .flat_map(|y| (12..51).map(move |x| (y * 64 + x) * 4))
        .map(|i| u64::from(pixels[i].abs_diff(pixels[i + 4])).pow(2))
        .sum()
}

fn channel_material(channel: usize) -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: (channel == 1).then_some(TextureHandle::from_raw(1)),
        roughness_metallic: (channel == 2).then_some(TextureHandle::from_raw(1)),
        ambient_occlusion: (channel == 3).then_some(TextureHandle::from_raw(1)),
        self_illumination: (channel == 4).then_some(TextureHandle::from_raw(1)),
        roughness_micro: 1_000_000,
        metalness_micro: if channel == 2 { 1_000_000 } else { 0 },
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: true,
    })
    .unwrap()
}

#[test]
fn grazing_plane_anisotropy_preserves_minor_axis_checker_detail() {
    // Derive the screen footprint independently of the sampler implementation.
    let north_projection = 2.0_f64 / (32.0_f64.powi(2) + 2.0_f64.powi(2)).sqrt();
    let east_texels_per_pixel = 256.0 * 0.25 / 2.0 * 2.2 / 64.0;
    let north_texels_per_pixel = 256.0 * 4.0 / (32.0 * north_projection) * 2.2 / 64.0;
    assert!(north_texels_per_pixel / east_texels_per_pixel > 15.0);
    assert!(east_texels_per_pixel < 4.0 && north_texels_per_pixel > 4.0);
    let context = context();
    for channel in 0..5 {
        let mut frame = grazing_frame();
        frame.sun_direction = glam::Vec3::new(0.8, 0.0, 0.6);
        frame.sun_irradiance = glam::Vec3::splat(2.0);
        frame.ambient_sky = glam::Vec3::splat(0.1);
        frame.ambient_ground = glam::Vec3::splat(0.1);
        let textures = if channel == 0 {
            vec![checker()]
        } else {
            vec![white_texture(), checker()]
        };
        let material = channel_material(channel);
        let one = render_frame(&context, &plane(textures.clone(), &material), &frame);
        let sixteen = render_frame(
            &context,
            &plane(
                textures,
                &material.with_anisotropy(TextureAnisotropy::new(16).unwrap()),
            ),
            &frame,
        );
        let low = horizontal_energy(&one);
        let high = horizontal_energy(&sixteen);
        assert!(
            high > low + 10_000,
            "channel {channel}: 1x={low}, 16x={high}"
        );
    }
}

#[test]
fn shared_albedo_materials_cache_samplers_by_material_anisotropy() {
    let context = context();
    let one = material(SceneAlphaMode::Opaque, true);
    let sixteen = one.with_anisotropy(TextureAnisotropy::new(16).unwrap());
    assert_eq!(one.albedo(), sixteen.albedo());
    let mut cache = crate::sampling::MaterialSamplers::default();
    let first = cache.get(context.device(), &one).clone();
    let second = cache.get(context.device(), &sixteen).clone();
    assert_ne!(first, second);
    assert_eq!(first, *cache.get(context.device(), &one));
    assert_eq!(second, *cache.get(context.device(), &sixteen));
}
