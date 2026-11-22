//! Front receiver normal selection and separate shader/binding contract checks.
use super::*;

#[test]
fn front_receiver_uses_decoded_g3_and_observation_keeps_the_shading_normal() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = crate::device::GpuContext::shared_for_test().unwrap();
    let select = metadata::function_source(LIGHTING_WGSL, "scene_deferred_receiver_normal");
    let shader = format!("{}\n{select}", crate::gbuffer::GBUFFER_WGSL);
    for renders in [false, true] {
        let entry = format!(
            "@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {{
    let geometric = scene_gbuffer_g3_normal(scene_gbuffer_g3_encode(vec3<f32>(0.0, 0.0, 1.0), 0u));
    let normal = scene_deferred_receiver_normal(vec3<f32>(1.0, 0.0, 0.0), geometric, {renders});
    result[0] = vec4<f32>(normal, 1.0);
}}"
        );
        let actual =
            crate::shader::gpu_math_tests::evaluate_source(&context, &shader, &entry, None, None);
        let expected = if renders {
            crate::gbuffer::reference::g3_normal(crate::gbuffer::reference::g3_encode(
                glam::Vec3::Z,
                0,
            ))
        } else {
            glam::Vec3::X
        };
        for (actual, expected) in actual[..3].iter().zip(expected.to_array()) {
            assert!((actual - expected).abs() < 1e-6);
        }
        assert_eq!(actual[3].to_bits(), 1.0_f32.to_bits());
    }
}

#[test]
fn lighting_camera_matches_wgsl_size_and_defaults_to_observation() {
    let shader = source(ENVIRONMENT_WGSL);
    let (module, _) = validated("camera contract", &shader);
    let camera = module
        .types
        .iter()
        .find(|(_, ty)| ty.name.as_deref() == Some("SceneDeferredCamera"))
        .unwrap();
    let naga::TypeInner::Struct { span, .. } = &camera.1.inner else {
        panic!("camera must be a uniform block");
    };
    let bytes = receiver_camera_bytes(glam::Mat4::IDENTITY, glam::Mat4::IDENTITY, false);
    assert_eq!(*span, 128);
    assert_eq!(bytes.len(), 128);
    assert_eq!(&bytes[112..128], &[0; 16]);
    let render = receiver_camera_bytes(glam::Mat4::IDENTITY, glam::Mat4::IDENTITY, true);
    assert_eq!(&render[..112], &bytes[..112]);
    assert_eq!(&render[112..116], &1_u32.to_le_bytes());
    assert_eq!(&render[116..128], &[0; 12]);
}

#[test]
fn front_lighting_needs_no_payload_bindings_and_payload_siblings_keep_them() {
    for (name, object) in object_variants() {
        let payload = object.contains("fn scene_receiver_load(");
        let shader = source(&object);
        let (module, info) = validated(&name, &shader);
        let bindings = used_bindings(&module, &info, FRAGMENT_ENTRY);
        for binding in [3, 4] {
            assert_eq!(bindings.contains(&(2, binding)), payload, "{name}");
        }
        let shadowed = object.contains("fn cascaded_sunlight_visibility(");
        if shadowed && !payload {
            assert!(bindings.contains(&(1, 19)), "{name}: G3");
            assert!(bindings.contains(&(1, 20)), "{name}: depth");
            assert!(bindings.contains(&(1, 22)), "{name}: camera policy");
            let composed = metadata::function_source(&shader, "cascaded_sunlight_visibility");
            assert!(composed.contains("scene_front_receiver_delta(dpdx(position)"));
            assert!(!composed.contains("(dpdx(position) +"));
        }
        if payload || !shadowed {
            let original = metadata::function_source(&object, "raw_sunlight_visibility");
            let composed = metadata::function_source(&shader, "raw_sunlight_visibility");
            assert_eq!(
                composed, original,
                "{name}: unchanged payload or unshadowed consumer"
            );
        }
    }
}

#[test]
fn front_surface_carries_the_decoded_geometric_normal_before_lighting() {
    let surface = metadata::function_source(LIGHTING_WGSL, "scene_deferred_surface");
    assert!(surface.contains("stored.geometric_normal"));
    let shade = metadata::function_source(LIGHTING_WGSL, "scene_deferred_shade");
    let prepare = shade
        .find("scene_deferred_geometric_normal = deferred.geometric_normal;")
        .unwrap();
    let lighting = shade.find("sunlight_visibility(position, normal)").unwrap();
    assert!(prepare < lighting);
}

/// The scene position where the ray through `pixel` meets the plane (point, normal).
fn plane_position(
    matrix: glam::Mat4,
    size: glam::Vec2,
    pixel: glam::Vec2,
    plane: (glam::Vec3, glam::Vec3),
) -> glam::Vec3 {
    let ndc = glam::Vec2::new(pixel.x / size.x * 2.0 - 1.0, 1.0 - pixel.y / size.y * 2.0);
    let inverse = matrix.inverse();
    let near = inverse.project_point3(ndc.extend(0.0));
    let ray = inverse.project_point3(ndc.extend(1.0)) - near;
    near + ray * (plane.1.dot(plane.0 - near) / plane.1.dot(ray))
}

fn displacement_frame() -> crate::frame::FrameUniform {
    let mut frame = crate::shader::gpu_math_tests::configured_frame(
        worldrunner_render_lighting::ExposureOwnership::SceneLinear(1.0),
    );
    frame.raster_viewport = [257, 241];
    frame.view_projection =
        glam::camera::rh::proj::directx::perspective(1.0, 257.0 / 241.0, 0.1, 20.0)
            * glam::camera::rh::view::look_at_mat4(
                glam::Vec3::new(0.0, -4.0, 6.5),
                glam::Vec3::ZERO,
                glam::Vec3::Z,
            );
    frame
}

#[test]
fn front_displacement_matches_the_plane_difference_of_the_quad_pair() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = crate::device::GpuContext::shared_for_test().unwrap();
    let (_, object) = object_variants()
        .into_iter()
        .find(|(_, object)| {
            object.contains("fn cascaded_sunlight_visibility(")
                && !object.contains("fn scene_receiver_load(")
        })
        .expect("a shadowed frame without a receiver payload");
    let solve = metadata::function_source(&source(&object), "scene_front_displacement").to_owned();
    let perspective = displacement_frame();
    let mut orthographic = perspective;
    orthographic.view_projection =
        glam::camera::rh::proj::directx::orthographic(-4.0, 4.0, -4.0, 4.0, 0.1, 20.0)
            * glam::camera::rh::view::look_at_mat4(
                glam::Vec3::new(0.0, -4.0, 6.5),
                glam::Vec3::ZERO,
                glam::Vec3::Z,
            );
    for frame in [perspective, orthographic] {
        for normal in [
            glam::Vec3::Z,
            glam::Vec3::new(0.0, -0.9998, 0.0156).normalize(),
        ] {
            for pixel in [glam::Vec2::new(178.5, 79.5), glam::Vec2::new(179.5, 78.5)] {
                assert_quad_displacement(&context, &solve, &frame, pixel, normal);
            }
        }
    }
}

fn displacement_entry(
    pixel: glam::Vec2,
    reciprocal_w: f32,
    normal: glam::Vec3,
    axis: glam::Vec2,
) -> String {
    format!(
        "@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {{
    let fragment = vec4<f32>({}, {}, 0.0, {reciprocal_w});
    let normal = vec3<f32>({}, {}, {});
    let along = scene_front_displacement(fragment, vec2<f32>({}, {}), normal);
    result[0] = vec4<f32>(along, length(scene_front_displacement(fragment, vec2<f32>(0.0, 1.0), normal)));
}}",
        pixel.x, pixel.y, normal.x, normal.y, normal.z, axis.x, axis.y
    )
}

fn assert_quad_displacement(
    context: &crate::device::GpuContext,
    solve: &str,
    frame: &crate::frame::FrameUniform,
    pixel: glam::Vec2,
    normal: glam::Vec3,
) {
    let size = glam::Vec2::new(257.0, 241.0);
    let plane = (glam::Vec3::ZERO, normal);
    let position = plane_position(frame.view_projection, size, pixel, plane);
    let clip = frame.view_projection * position.extend(1.0);
    let shader = format!("{ENVIRONMENT_WGSL}\n{solve}");
    let left = glam::Vec2::new((pixel.x / 2.0).floor() * 2.0 + 0.5, pixel.y);
    let top = glam::Vec2::new(pixel.x, (pixel.y / 2.0).floor() * 2.0 + 0.5);
    let across = plane_position(frame.view_projection, size, left + glam::Vec2::X, plane)
        - plane_position(frame.view_projection, size, left, plane);
    let down = plane_position(frame.view_projection, size, top + glam::Vec2::Y, plane)
        - plane_position(frame.view_projection, size, top, plane);
    for (axis, expected) in [(glam::Vec2::X, across), (glam::Vec2::Y, down)] {
        let entry = displacement_entry(pixel, 1.0 / clip.w, normal, axis);
        let actual = crate::shader::gpu_math_tests::evaluate_source(
            context,
            &shader,
            &entry,
            Some(frame),
            None,
        );
        for (actual, expected) in actual[..3].iter().zip(expected.to_array()) {
            assert!(
                (actual - expected).abs() < 2e-4,
                "{pixel:?} {normal:?} {axis:?}: {actual} != {expected}"
            );
        }
        assert!(
            (actual[3] - down.length()).abs() < 2e-4,
            "{pixel:?} {normal:?}"
        );
    }
}

#[test]
fn front_quad_compatibility_uses_the_oct_normal_uncertainty() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = crate::device::GpuContext::shared_for_test().unwrap();
    let (_, object) = object_variants()
        .into_iter()
        .find(|(_, object)| {
            object.contains("fn cascaded_sunlight_visibility(")
                && !object.contains("fn scene_receiver_load(")
        })
        .unwrap();
    let shader = source(&object);
    let predicate = metadata::function_source(&shader, "scene_front_on_plane");
    let shader = format!("{}\n{predicate}", crate::gbuffer::GBUFFER_WGSL);
    let entry = "@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {
    let normal = vec3<f32>(0.0, 0.0, 1.0);
    let decoded = scene_gbuffer_g3_normal(scene_gbuffer_g3_encode(normal, 0u));
    let tangent = normalize(cross(decoded, vec3<f32>(1.0, 0.0, 0.0)));
    let outside = normalize(tangent + decoded * 0.05);
    result[0] = vec4<f32>(
        select(0.0, 1.0, scene_front_on_plane(outside, decoded)),
        select(0.0, 1.0, scene_front_on_plane(vec3<f32>(1.0, 0.0, 0.0), decoded)),
        select(0.0, 1.0, scene_front_on_plane(vec3<f32>(0.0, 1.0, 0.0), decoded)),
        select(0.0, 1.0, scene_front_on_plane(normal, decoded))
    );
}";
    let actual =
        crate::shader::gpu_math_tests::evaluate_source(&context, &shader, entry, None, None);
    assert_eq!(actual, [0.0, 1.0, 1.0, 0.0]);
}

#[test]
fn oct_normal_bound_contains_reference_normals_across_folds_and_axes() {
    let bound = 18.0_f32.sqrt() / 127.0;
    for x in -8_i16..=8 {
        for y in -8_i16..=8 {
            for z in -8_i16..=8 {
                if x == 0 && y == 0 && z == 0 {
                    continue;
                }
                let normal = glam::Vec3::new(f32::from(x), f32::from(y), f32::from(z)).normalize();
                let code = crate::gbuffer::reference::g3_encode(normal, 0);
                let decoded = crate::gbuffer::reference::g3_normal(code);
                let residual = normal.cross(decoded).length();
                assert!(residual <= bound, "{normal:?}: {residual} > {bound}");
            }
        }
    }
}

#[test]
fn front_pair_origin_preserves_the_other_pixel_coordinate() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = crate::device::GpuContext::shared_for_test().unwrap();
    let (_, object) = object_variants()
        .into_iter()
        .find(|(_, object)| {
            object.contains("fn cascaded_sunlight_visibility(")
                && !object.contains("fn scene_receiver_load(")
        })
        .unwrap();
    let shader = source(&object);
    let origin = metadata::function_source(&shader, "scene_front_pair_origin");
    let entry = "@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {
    let pixel = vec2<i32>(179, 79);
    let across = scene_front_pair_origin(pixel, vec2<i32>(1, 0));
    let down = scene_front_pair_origin(pixel, vec2<i32>(0, 1));
    result[0] = vec4<f32>(vec2<f32>(across), vec2<f32>(down));
}";
    let actual =
        crate::shader::gpu_math_tests::evaluate_source(&context, origin, entry, None, None);
    assert_eq!(actual, [178.0, 79.0, 179.0, 78.0]);
}

#[test]
fn front_pair_provenance_checks_coverage_and_geometric_normal_codes() {
    let (_, object) = object_variants()
        .into_iter()
        .find(|(_, object)| {
            object.contains("fn cascaded_sunlight_visibility(")
                && !object.contains("fn scene_receiver_load(")
        })
        .expect("a shadowed frame without a receiver payload");
    let shader = source(&object);
    let provenance = metadata::function_source(&shader, "scene_front_pair_provenance");
    assert!(provenance.contains("textureDimensions(scene_deferred_g3)"));
    assert!(provenance.contains("textureLoad(scene_deferred_g3, first, 0).r"));
    assert!(provenance.contains("textureLoad(scene_deferred_g3, second, 0).r"));
    assert!(provenance.contains("first_g3 != 0u && second_g3 != 0u"));
    assert!(provenance.contains("0xfffffffcu"));
    assert!(provenance.contains("second >= size"));
    let delta = metadata::function_source(&shader, "scene_front_receiver_delta");
    assert!(delta.contains("!provenance || !on_plane"));
}

#[test]
fn front_rejects_coplanar_quad_from_uncovered_or_normal_discontinuous_neighbor() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = crate::device::GpuContext::shared_for_test().unwrap();
    let (_, object) = object_variants()
        .into_iter()
        .find(|(_, object)| {
            object.contains("fn cascaded_sunlight_visibility(")
                && !object.contains("fn scene_receiver_load(")
        })
        .expect("a shadowed frame without a receiver payload");
    let shader = source(&object);
    let front = [
        "scene_front_displacement",
        "scene_front_on_plane",
        "scene_front_receiver_delta",
    ]
    .map(|name| metadata::function_source(&shader, name))
    .join("\n");
    let shader = format!(
        "{ENVIRONMENT_WGSL}\nstruct FrontTestCamera {{ receiver_policy: vec4<u32>, }};\nvar<private> scene_deferred_camera: FrontTestCamera;\nvar<private> scene_deferred_geometric_normal: vec3<f32>;\nvar<private> scene_deferred_fragment: vec4<f32>;\nvar<private> scene_deferred_covered: bool;\nfn scene_front_pair_provenance(axis: vec2<f32>) -> bool {{ return false; }}\n{front}"
    );
    let frame = displacement_frame();
    let pixel = glam::Vec2::new(128.5, 120.5);
    let position = plane_position(
        frame.view_projection,
        glam::Vec2::new(257.0, 241.0),
        pixel,
        (glam::Vec3::ZERO, glam::Vec3::Z),
    );
    let reciprocal_w = 1.0 / (frame.view_projection * position.extend(1.0)).w;
    let entry = format!(
        "@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;\n@compute @workgroup_size(1) fn evaluate_math() {{\n    scene_deferred_camera.receiver_policy = vec4<u32>(1u, 0u, 0u, 0u);\n    scene_deferred_geometric_normal = vec3<f32>(0.0, 0.0, 1.0);\n    scene_deferred_fragment = vec4<f32>({}, {}, 0.0, {});\n    scene_deferred_covered = true;\n    let quad = vec3<f32>(1.0, 0.0, 0.0);\n    let selected = scene_front_receiver_delta(quad, vec2<f32>(1.0, 0.0));\n    let expected = scene_front_displacement(scene_deferred_fragment, vec2<f32>(1.0, 0.0), scene_deferred_geometric_normal);\n    result[0] = vec4<f32>(selected, length(selected - expected));\n}}",
        pixel.x, pixel.y, reciprocal_w
    );
    let actual = crate::shader::gpu_math_tests::evaluate_source(
        &context,
        &shader,
        &entry,
        Some(&frame),
        None,
    );
    assert!(actual[3] < 2e-4, "coplanar quad was admitted: {actual:?}");
}
