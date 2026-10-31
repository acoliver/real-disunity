use super::{LayeredGpuSource, principal_tests::fixture, source_mip_tests};
use crate::device::GpuContext;
use glam::Vec2;

#[test]
fn gpu_principal_footprints_are_finite_at_extreme_scales() {
    let context = crate::terrain::pages::residency_tests::context();
    let (terrain, resources) = fixture(16, false, 0);
    let source = LayeredGpuSource::new(&terrain, &resources).unwrap();
    let diagonal = 0.375 / 2.0_f32.sqrt();
    for (gradients, level, expected) in [
        ([[7.071_068e-14; 2], [0.0; 2]], 16, Some((1.0, 0.0))),
        ([[f32::MAX / 16.0; 2], [0.0; 2]], 16, None),
        ([[f32::MAX; 2]; 2], 16, None),
        ([[f32::from_bits(1); 2], [0.0; 2]], 16, Some((1.0, 0.0))),
        ([[1.0, 2.0], [0.0; 2]], 16, None),
        ([[0.125, 1e-30], [0.0, 0.25]], 16, Some((1.0, 0.0))),
        ([[0.0; 2]; 2], 16, Some((1.0, 0.0))),
        (
            [[diagonal, diagonal], [-diagonal, diagonal]],
            2,
            Some((2.0, 1.5_f32.log2())),
        ),
        ([[2.0, 0.25], [2.0, -0.25]], 16, Some((16.0, 0.5))),
    ] {
        let axes = probe(&context, &source, gradients, level, true);
        assert!(
            axes.into_iter().all(f32::is_finite),
            "gradients={gradients:?}, axes={axes:?}"
        );
        assert!((Vec2::new(axes[0], axes[1]).length() - 1.0).abs() < 1e-6);
        let footprint = probe(&context, &source, gradients, level, false);
        assert!(
            footprint.into_iter().all(f32::is_finite),
            "footprint={footprint:?}"
        );
        assert!((1.0..=f32::from(level)).contains(&footprint[2]));
        assert!((0.0..=3.0).contains(&footprint[3]));
        if let Some((count, lod)) = expected {
            assert!((footprint[2] - count).abs() < 1e-6);
            assert!((footprint[3] - lod).abs() < 1e-6);
        }
    }
}

fn probe(
    context: &GpuContext,
    source: &LayeredGpuSource,
    gradients: [[f32; 2]; 2],
    level: u16,
    axes: bool,
) -> [f32; 4] {
    let [[a, b], [c, d]] = gradients;
    let expression = if axes {
        "layered_principal_axes(dx, dy, size)".to_owned()
    } else {
        format!("layered_footprint(dx, dy, size, {level}.0)")
    };
    let record = if axes {
        "value"
    } else {
        "vec4<f32>(value.xyz, clamp(value.w, 0.0, f32(textureNumLevels(layered_array_0) - 1u)))"
    };
    let production = super::LAYERED_WGSL
        .split("@compute @workgroup_size(64)")
        .next()
        .unwrap();
    let shader = format!(
        "{production}\n@compute @workgroup_size(64)\n\
         fn bake_layered(@builtin(global_invocation_id) id: vec3<u32>) {{\n\
         if id.x > 0u {{ return; }}\n\
         let dx = vec2<f32>({a:e}, {b:e});\n\
         let dy = vec2<f32>({c:e}, {d:e});\n\
         let size = vec2<f32>(8.0, 4.0);\n\
         let value = {expression};\n\
         layered_products[0] = bitcast<vec4<u32>>({record});\n}}"
    );
    let bytes =
        source_mip_tests::execute_shader(context, source, &[[0.0; 2]], [0.0; 2], Some(&shader));
    std::array::from_fn(|i| f32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap()))
}
