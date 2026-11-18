//! CPU checks of the lighting program: it validates over every forward object source a frame
//! can light with, and its entry points never touch the material's set 1.
use super::*;
use crate::scene_metadata::receiver::{self as metadata, ReceiverLayout};
use crate::shader::{ENVIRONMENT_WGSL, TERRAIN_WGSL};
use worldrunner_render_plan::SceneSsaoReceiver;

#[path = "front_tests.rs"]
mod front;

#[path = "lighting/tests/reciprocal_w.rs"]
mod reciprocal_w;

/// The receiver siblings `ScenePipelines` derives from `source`: the gathering SSAO receiver
/// when the source loads nearest AO, and for shadowed sources the receiver-payload consumer and
/// the card-reject consumer of each payload layout.
fn siblings(name: &str, source: String, shadowed: bool) -> Vec<(String, String)> {
    let mut variants = Vec::new();
    if source.contains("fn screen_ao(") && !source.contains("fn receiver_gather(") {
        variants.push((
            format!("{name} gather"),
            crate::ssao::receiver::source(&source, SceneSsaoReceiver::DepthAwareGather),
        ));
    }
    if shadowed {
        for layout in [ReceiverLayout::Front, ReceiverLayout::Opposing] {
            variants.push((
                format!("{name} payload {layout:?}"),
                metadata::consumer_source(&source, layout, false),
            ));
            variants.push((
                format!("{name} card reject {layout:?}"),
                metadata::card_reject_source(&source, layout),
            ));
        }
    }
    variants.push((name.to_owned(), source));
    variants
}

/// Every forward object source a frame's colour pipelines compose, with and without velocity:
/// plain, with SSAO, with forward reflections with and without application AO, the shadowed
/// receivers of each screen-space combination, and their receiver siblings.
pub(crate) fn object_variants() -> Vec<(String, String)> {
    let mut variants = Vec::new();
    for velocity in [false, true] {
        let plain = if velocity {
            crate::velocity::object_source(ENVIRONMENT_WGSL)
        } else {
            ENVIRONMENT_WGSL.to_owned()
        };
        let tag = |name: &str| format!("{name}{}", if velocity { " velocity" } else { "" });
        variants.extend(siblings(&tag("plain"), plain.clone(), false));
        variants.extend(siblings(
            &tag("ssao"),
            crate::ssao::ambient_source(&plain),
            false,
        ));
        for application in [false, true] {
            let (object, _) = crate::ssr::forward::velocity_sources(
                (ENVIRONMENT_WGSL, TERRAIN_WGSL),
                velocity,
                application,
            );
            let name = tag(&format!("reflections application {application}"));
            variants.extend(siblings(&name, object, false));
        }
        for (ssao, reflection) in [(false, false), (true, false), (false, true), (true, true)] {
            let (object, _) = crate::shadows::receiver_sources(ssao, reflection, velocity);
            let name = tag(&format!("shadowed ssao {ssao} reflections {reflection}"));
            variants.extend(siblings(&name, object, true));
        }
    }
    variants
}

fn validated(name: &str, source: &str) -> (naga::Module, naga::valid::ModuleInfo) {
    let module = naga::front::wgsl::parse_str(source)
        .unwrap_or_else(|error| panic!("{name}: {}", error.emit_to_string(source)));
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap_or_else(|error| panic!("{name}: {}", error.emit_to_string(source)));
    (module, info)
}

/// The set and binding of every resource the entry point `entry` uses.
fn used_bindings(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    entry: &str,
) -> Vec<(u32, u32)> {
    let index = module
        .entry_points
        .iter()
        .position(|point| point.name == entry)
        .unwrap_or_else(|| panic!("{entry} is missing"));
    let usage = info.get_entry_point(index);
    module
        .global_variables
        .iter()
        .filter(|(handle, _)| !usage[*handle].is_empty())
        .filter_map(|(_, variable)| {
            variable
                .binding
                .as_ref()
                .map(|binding| (binding.group, binding.binding))
        })
        .collect()
}

#[test]
fn lighting_program_validates_for_every_forward_variant_and_never_reads_the_material() {
    let variants = object_variants();
    assert!(variants.len() >= 40, "{} variants", variants.len());
    let mut reflecting = 0;
    let mut payload = 0;
    for (name, object) in &variants {
        let source = source(object);
        let (module, info) = validated(name, &source);
        let bindings = used_bindings(&module, &info, FRAGMENT_ENTRY);
        for (group, binding) in &bindings {
            assert!(
                *group != 1 || *binding >= 16,
                "{name}: the lighting entry uses the material binding ({group}, {binding})"
            );
        }
        for binding in 16..=22 {
            assert!(
                bindings.contains(&(1, binding)),
                "{name}: input (1, {binding}) is unused"
            );
        }
        assert!(bindings.contains(&(0, 0)), "{name}: the frame block");
        assert!(used_bindings(&module, &info, VERTEX_ENTRY).is_empty());
        let entry = module
            .entry_points
            .iter()
            .find(|point| point.name == FRAGMENT_ENTRY)
            .unwrap();
        let outputs = match &entry.function.result {
            Some(result) => match &module.types[result.ty].inner {
                naga::TypeInner::Struct { members, .. } => members.len(),
                _ => 1,
            },
            None => 0,
        };
        let reflections = object.contains("fn reflection_output(");
        assert_eq!(outputs, if reflections { 3 } else { 1 }, "{name}");
        reflecting += usize::from(reflections);
        payload += usize::from(source.contains("scene_receiver_pixel = vec2<i32>(pixel.xy);"));
        if object.contains("fn scene_receiver_back_visibility(") {
            assert!(
                source.contains("return scene_receiver_back_visibility(position, normal);"),
                "{name}: the payload's far-side visibility"
            );
        }
    }
    assert!(reflecting > 0 && payload > 0, "{reflecting} {payload}");
}

#[test]
fn lighting_source_replaces_only_the_material_reflection_normal() {
    let (object, _) =
        crate::ssr::forward::velocity_sources((ENVIRONMENT_WGSL, TERRAIN_WGSL), false, false);
    assert!(
        object
            .contains("if material.contact_surface.z > 0.0 { return reflection_normal(normal); }")
    );
    let lit = source(&object);
    assert!(
        !lit.contains("if material.contact_surface.z > 0.0 { return reflection_normal(normal); }")
    );
    assert!(lit.starts_with(&object.replace(
        "fn scene_environment_normal(normal: vec3<f32>) -> vec3<f32> { if material.contact_surface.z > 0.0 { return reflection_normal(normal); } return normal; }",
        "fn scene_environment_normal(normal: vec3<f32>) -> vec3<f32> { return normal; }",
    )));
    // Without reflections the forward source is kept as it is.
    assert!(source(ENVIRONMENT_WGSL).starts_with(ENVIRONMENT_WGSL));
}

#[test]
fn camera_block_inverts_the_projection_and_the_view_rotation() {
    let rotation = glam::Quat::from_euler(glam::EulerRot::ZXY, 0.7, -1.1, 0.3);
    let view = glam::Mat4::from_rotation_translation(rotation, glam::Vec3::new(1.0, -2.0, 3.0));
    let projection = glam::Mat4::from_cols_array(&[
        1.2, 0.0, 0.0, 0.0, //
        0.0, 1.8, 0.0, 0.0, //
        0.0, 0.0, -1.001, -1.0, //
        0.0, 0.0, -0.1, 0.0,
    ]);
    let view_projection = projection * view;
    let floats: Vec<f32> = camera_bytes(view_projection, view)
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
        .collect();
    let scene_from_clip = glam::Mat4::from_cols_slice(&floats[..16]);
    let point = glam::Vec3::new(0.5, -0.25, -4.0);
    let mut clip = view_projection * point.extend(1.0);
    clip.z -= clip.w;
    let back = scene_from_clip * clip;
    assert!((back.truncate() / back.w - point).length() < 1e-4);
    let scene_from_view = glam::Mat3::from_cols(
        glam::Vec3::from_slice(&floats[16..19]),
        glam::Vec3::from_slice(&floats[20..23]),
        glam::Vec3::from_slice(&floats[24..27]),
    );
    let direction = glam::Vec3::new(0.0, 0.6, 0.8);
    let view_direction = glam::Mat3::from_mat4(view) * direction;
    assert!((scene_from_view * view_direction - direction).length() < 1e-6);
    assert_eq!(
        [floats[19], floats[23], floats[27]].map(f32::to_bits),
        [0; 3]
    );
}

#[test]
fn camera_reconstruction_preserves_the_stored_depth_ray_near_the_far_plane() {
    let position = metadata::function_source(LIGHTING_WGSL, "scene_deferred_homogeneous_position");
    assert!(position.contains("fma(inverse[0], vec4<f32>(ndc.x)"));
    assert!(position.contains("fma(inverse[1], vec4<f32>(ndc.y)"));
    assert!(position.contains("fma(inverse[2], vec4<f32>(depth - 1.0), inverse[3])"));
    let view = glam::camera::rh::view::look_at_mat4(
        glam::Vec3::new(0.0, -4.0, 6.5),
        glam::Vec3::ZERO,
        glam::Vec3::Z,
    );
    let projection = glam::camera::rh::proj::directx::perspective(1.0, 257.0 / 241.0, 0.1, 20.0);
    let view_projection = projection * view;
    let values: Vec<f32> = camera_bytes(view_projection, view)[..64]
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
        .collect();
    let inverse = glam::Mat4::from_cols_slice(&values);
    let reference = view_projection.as_dmat4().inverse();
    for row in [78.5_f32, 79.5] {
        for code in 0x3f7e_52a7..=0x3f7e_52a9 {
            let clip = glam::Vec4::new(
                179.5 / 257.0 * 2.0 - 1.0,
                1.0 - row / 241.0 * 2.0,
                f32::from_bits(code),
                1.0,
            );
            let expected = reference * clip.as_dvec4();
            let expected = (expected.truncate() / expected.w).as_vec3();
            let shifted = clip - glam::Vec4::Z * clip.w;
            let columns = inverse.to_cols_array_2d();
            let actual = glam::Vec4::from_array(std::array::from_fn(|row| {
                columns[0][row].mul_add(
                    shifted.x,
                    columns[1][row].mul_add(
                        shifted.y,
                        columns[2][row].mul_add(shifted.z, columns[3][row]),
                    ),
                )
            }));
            let actual = actual.truncate() / actual.w;
            assert!(
                (actual - expected).abs().max_element() < 5e-7,
                "row {row}, depth {code:#x}: {actual:?} != {expected:?}"
            );
        }
    }
}
