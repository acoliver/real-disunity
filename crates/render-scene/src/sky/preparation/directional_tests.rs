use super::*;
use glam::DVec3;
use worldrunner_render_plan::SceneRoughnessRemap;

#[test]
fn selected_prefilter_matches_directional_radiance_equations() {
    let context = pollster::block_on(crate::device::GpuContext::request()).unwrap();
    let source = directional_source(&context);
    for remap in [
        SceneRoughnessRemap::Perceptual04,
        SceneRoughnessRemap::LinearAlpha,
    ] {
        let mut config = settings();
        config.convention.roughness = remap;
        config.samples = 1024;
        let mut encoder = context.submission().begin(context.device()).unwrap();
        let maps = encode(
            context.device(),
            &mut encoder,
            &source,
            config,
            AllocationBudget(1_000_000),
        )
        .unwrap();
        encoder.finish().submit().unwrap();
        let bytes = read_texture(context.device(), context.submission(), &maps.cube, 1);
        let offset = 4 * 2 * 256;
        let actual = half(u16::from_le_bytes([bytes[offset], bytes[offset + 1]]));
        let roughness = if remap == SceneRoughnessRemap::LinearAlpha {
            0.5_f64.sqrt()
        } else {
            0.5
        };
        let expected =
            directional_integral(DVec3::new(-0.5, 0.5, 1.0).normalize(), roughness, 1024);
        assert!(
            (f64::from(actual) - expected).abs() < 0.003,
            "{remap:?}: {actual} != {expected}"
        );
    }
}

fn directional_integral(normal: DVec3, roughness: f64, samples: u32) -> f64 {
    let tangent = DVec3::Z.cross(normal).normalize();
    let bitangent = normal.cross(tangent);
    let mut sum = 0.0;
    let mut weight = 0.0;
    for i in 0..samples {
        let sample_x = f64::from(i) / f64::from(samples);
        let sample_y = f64::from(i.reverse_bits()) / 4_294_967_296.0;
        let cosine = ((1.0 - sample_y) / (1.0 + (roughness.powi(4) - 1.0) * sample_y)).sqrt();
        let sine = (1.0 - cosine * cosine).sqrt();
        let phi = std::f64::consts::TAU * sample_x;
        let half_vector =
            tangent * (sine * phi.cos()) + bitangent * (sine * phi.sin()) + normal * cosine;
        let light = 2.0 * normal.dot(half_vector) * half_vector - normal;
        let nl = normal.dot(light).max(0.0);
        sum += (0.5 + 0.25 * light.z * light.z) * nl;
        weight += nl;
    }
    sum / weight
}

fn directional_source(context: &crate::device::GpuContext) -> wgpu::TextureView {
    let source = context.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("directional polynomial radiance"),
        size: wgpu::Extent3d {
            width: 32,
            height: 32,
            depth_or_array_layers: 6,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut bytes = Vec::new();
    for face in 0..6 {
        for y in 0..32_u16 {
            for x in 0..32_u16 {
                let u = (f32::from(x) + 0.5) / 16.0 - 1.0;
                let v = (f32::from(y) + 0.5) / 16.0 - 1.0;
                let ray = match face {
                    0 => glam::Vec3::new(1.0, -v, -u),
                    1 => glam::Vec3::new(-1.0, -v, u),
                    2 => glam::Vec3::new(u, 1.0, v),
                    3 => glam::Vec3::new(u, -1.0, -v),
                    4 => glam::Vec3::new(u, -v, 1.0),
                    _ => glam::Vec3::new(-u, -v, -1.0),
                }
                .normalize();
                let value = 0.5 + 0.25 * ray.z * ray.z;
                let bits = value.to_bits();
                let half = u16::try_from((bits >> 13) - 0x1c000).unwrap();
                bytes.extend(
                    [half, half, half, 0x3c00]
                        .into_iter()
                        .flat_map(u16::to_le_bytes),
                );
            }
        }
    }
    context.queue().write_texture(
        source.as_image_copy(),
        &bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(256),
            rows_per_image: Some(32),
        },
        source.size(),
    );
    source.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::Cube),
        ..Default::default()
    })
}
