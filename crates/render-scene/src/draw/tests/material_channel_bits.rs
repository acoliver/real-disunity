use super::*;
use std::fmt::Write;

// Inject the sampler result so these assertions measure selection, independently of
// backend filtering and texture-format handling of non-finite values and signed zero.
fn probe(packed: bool, negative_zero: bool) -> String {
    let (surface, auxiliary, emission, expected) = if negative_zero && packed {
        (
            [-0.0, -0.0, 0.0, 0.0],
            [-0.0, -0.0, 0.0, 0.0],
            [-0.0, 0.0, 0.0, 0.0],
            [-0.0_f32; 4],
        )
    } else if negative_zero {
        (
            [0.0, -0.0, -0.0, 0.0],
            [-0.0, 0.0, 0.0, 0.0],
            [-0.0, 0.0, 0.0, 0.0],
            [-0.0_f32; 4],
        )
    } else if packed {
        (
            [0.5, 0.25, f32::INFINITY, f32::NAN],
            [0.375, 0.75, f32::INFINITY, f32::NAN],
            [0.375, f32::INFINITY, f32::NAN, 1.0],
            [0.5, 0.25, 0.75, 0.375],
        )
    } else {
        (
            [f32::INFINITY, 0.5, 0.25, f32::NAN],
            [0.75, f32::INFINITY, f32::NAN, 1.0],
            [0.375, f32::INFINITY, f32::NAN, 1.0],
            [0.8 * 0.5, 0.6 * 0.25, 0.75, 0.375],
        )
    };
    // Runtime uniform bits prevent the compiler from folding the injected samples.
    let flags = crate::pipeline::MaterialUniform::from_material(&auxiliary_material(packed, false))
        .texture_flags;
    let vector = |values: [f32; 4]| {
        let [r, g, b, a] = values.map(|value| value.to_bits() ^ flags);
        format!(
            "bitcast<vec4<f32>>(vec4<u32>({r}u, {g}u, {b}u, {a}u) ^ vec4<u32>(material.texture_flags))"
        )
    };
    let mut shader = crate::shader::ENVIRONMENT_WGSL.to_owned();
    for (map, values) in [
        ("rough_metal_texture", surface),
        ("ambient_occlusion_texture", auxiliary),
        ("self_illumination_texture", emission),
    ] {
        assert!(shader.contains(&format!("scene_atlas_sample({map}, input, footprint)")));
        for call in [
            format!("scene_atlas_first({map}, footprint)"),
            format!("scene_atlas_sample({map}, input, footprint)"),
        ] {
            shader = shader.replace(&call, &vector(values));
        }
    }
    let [r, g, b, a] = expected.map(f32::to_bits);
    // Preserve the original factor operations too: Metal multiplication can change
    // a zero's sign, so compare those two outputs against the original swizzles.
    let expected = if negative_zero && !packed {
        format!(
            "vec4<u32>(bitcast<u32>(material.roughness * {}.g), bitcast<u32>(material.metalness * {}.b), {b}u, {a}u)",
            vector(surface),
            vector(surface)
        )
    } else {
        format!("vec4<u32>({r}u, {g}u, {b}u, {a}u)")
    };
    write!(
        shader,
        "\n@fragment fn fs_bits(input: VertexOutput) -> @location(0) vec4<f32> {{
            let s = scene_surface_channels(input);
            let a = scene_auxiliary_samples(input);
            let exact = bitcast<vec4<u32>>(vec4<f32>(s, a)) == {expected};
            return select(vec4<f32>(0.0), vec4<f32>(1.0), exact);
        }}"
    )
    .unwrap();
    shader
}

fn assert_bits(negative_zero: bool) {
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    for packed in [false, true] {
        let scene = auxiliary_scene(packed, false);
        let gpu =
            SceneGpu::upload(context.device(), context.submission(), &scene, &layouts).unwrap();
        let camera = if packed {
            CameraPose {
                eye_millimeters: [0, -4000, 2000],
                target_millimeters: [0, 0, 1000],
            }
        } else {
            pose()
        };
        gpu.set_frame(context.queue(), frame_from(camera));
        let pixels = super::super::foliage::pass_color(
            &context,
            &gpu,
            &layouts,
            &probe(packed, negative_zero),
            "fs_bits",
        );
        assert!(
            pixels.chunks_exact(4).any(|pixel| pixel == [255; 4]),
            "packed={packed}, negative_zero={negative_zero}, pixels={:?}",
            pixels
                .chunks_exact(4)
                .collect::<std::collections::BTreeSet<_>>()
        );
        assert!(
            pixels
                .chunks_exact(4)
                .all(|pixel| pixel == [255; 4] || pixel == [0, 0, 0, 255]),
            "default selection must preserve every component's bits: packed={packed}, negative_zero={negative_zero}"
        );
    }
}

#[test]
fn unselected_non_finite_components_do_not_contaminate_default_scalars() {
    let _session = crate::test_support::SceneTestSession::acquire();
    assert_bits(false);
}

#[test]
fn selected_negative_zero_keeps_its_sign_in_default_scalar_paths() {
    let _session = crate::test_support::SceneTestSession::acquire();
    assert_bits(true);
}
