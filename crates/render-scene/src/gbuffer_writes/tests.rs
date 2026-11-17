//! CPU checks of the G-buffer producer sources: every terrain variant and object entry compiles
//! and writes the four targets from the same material evaluation the forward path lights.

use naga::{Binding, ScalarKind, ShaderStage, TypeInner, VectorSize};

fn module(label: &str, source: &str) -> naga::Module {
    let module = naga::front::wgsl::parse_str(source)
        .unwrap_or_else(|error| panic!("{label}: {}", error.emit_to_string(source)));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap_or_else(|error| panic!("{label}: {error:?}"));
    module
}

/// The (location, scalar kind, component count) of each output of fragment entry `name`.
fn fragment_outputs(module: &naga::Module, name: &str) -> Vec<(u32, ScalarKind, u32)> {
    let entry = module
        .entry_points
        .iter()
        .find(|entry| entry.name == name && entry.stage == ShaderStage::Fragment)
        .unwrap_or_else(|| panic!("no fragment entry {name}"));
    let result = entry.function.result.as_ref().expect("fragment result");
    let TypeInner::Struct { members, .. } = &module.types[result.ty].inner else {
        panic!("{name} returns a single value, not the G-buffer targets");
    };
    members
        .iter()
        .filter_map(|member| {
            let Some(Binding::Location { location, .. }) = member.binding else {
                return None;
            };
            let (kind, count) = match module.types[member.ty].inner {
                TypeInner::Scalar(scalar) => (scalar.kind, 1),
                TypeInner::Vector { size, scalar } => (
                    scalar.kind,
                    match size {
                        VectorSize::Bi => 2,
                        VectorSize::Tri => 3,
                        VectorSize::Quad => 4,
                    },
                ),
                ref other => panic!("{name} output of type {other:?}"),
            };
            Some((location, kind, count))
        })
        .collect()
}

/// Whether fragment entry `name` writes `@builtin(frag_depth)`.
fn writes_depth(module: &naga::Module, name: &str) -> bool {
    let entry = module
        .entry_points
        .iter()
        .find(|entry| entry.name == name)
        .expect("fragment entry");
    let result = entry.function.result.as_ref().expect("fragment result");
    let TypeInner::Struct { members, .. } = &module.types[result.ty].inner else {
        return false;
    };
    members
        .iter()
        .any(|member| member.binding == Some(Binding::BuiltIn(naga::BuiltIn::FragDepth)))
}

const TARGETS: [(u32, ScalarKind, u32); 4] = [
    (0, ScalarKind::Float, 4),
    (1, ScalarKind::Float, 4),
    (2, ScalarKind::Float, 4),
    (3, ScalarKind::Uint, 1),
];

fn terrain_variants() -> [String; 4] {
    let base = super::terrain_source(crate::shader::TERRAIN_WGSL);
    crate::terrain::program::source::test_sources(&base)
}

#[test]
fn every_terrain_variant_writes_the_four_gbuffer_targets() {
    let expected = TARGETS.to_vec();
    for (index, source) in terrain_variants().iter().enumerate() {
        let module = module(&format!("terrain variant {index}"), source);
        assert_eq!(
            fragment_outputs(&module, crate::shader::TERRAIN_FRAGMENT_ENTRY_POINT),
            expected,
            "terrain variant {index}"
        );
        assert!(
            module
                .entry_points
                .iter()
                .any(|entry| entry.name == crate::shader::TERRAIN_VERTEX_ENTRY_POINT),
            "terrain variant {index} lost its vertex entry"
        );
    }
}

#[test]
fn terrain_gbuffer_entry_stores_the_forward_material_without_lighting_it() {
    let base = super::terrain_source(crate::shader::TERRAIN_WGSL);
    assert_eq!(base.matches("fn fs_terrain(").count(), 1);
    let entry = &base[base.find("fn fs_terrain(").unwrap()..];
    assert!(entry.contains("terrain_material(input)"));
    assert!(entry.contains("scene_gbuffer_encode("));
    for lighting in [
        "scene_surface_lighting(",
        "apply_exposure(",
        "apply_height_fog(",
    ] {
        assert!(
            !entry.contains(lighting),
            "the G-buffer entry calls {lighting}"
        );
    }
    let forward = crate::shader::TERRAIN_WGSL;
    let forward_entry = &forward[forward.find("fn fs_terrain(").unwrap()..];
    assert!(forward_entry.contains("terrain_material(input)"));
}

#[test]
fn page_variants_replace_surface_selection_inside_the_shared_material() {
    let [plain, pages, layered, layered_pages] = terrain_variants();
    for source in [&plain, &layered] {
        assert!(source.contains("let centre_surface = read_surface_at("));
    }
    for source in [&pages, &layered_pages] {
        assert!(source.contains("let blended = cached_surface("));
        assert!(!source.contains("read_surface_at(i32(round(grid.x))"));
    }
}

#[test]
fn every_object_entry_writes_the_four_gbuffer_targets() {
    let module = module(
        "object G-buffer",
        &super::object_source(crate::shader::ENVIRONMENT_WGSL),
    );
    for entry in super::FORWARD_OBJECTS {
        assert_eq!(
            fragment_outputs(&module, entry),
            TARGETS.to_vec(),
            "{entry}"
        );
        assert_eq!(
            writes_depth(&module, entry),
            entry.starts_with("fs_relief"),
            "{entry}"
        );
    }
    let mut found: Vec<_> = module
        .entry_points
        .iter()
        .filter(|entry| entry.stage == ShaderStage::Fragment)
        .map(|entry| entry.name.as_str())
        .collect();
    let mut expected = super::FORWARD_OBJECTS.to_vec();
    expected.push("fs_unlit");
    found.sort_unstable();
    expected.sort_unstable();
    assert_eq!(found, expected, "the module keeps no forward object entry");
}

#[test]
fn object_gbuffer_entries_store_the_forward_material_without_lighting_it() {
    let source = super::object_source(crate::shader::ENVIRONMENT_WGSL);
    let entries = &source[source.find("fn scene_gbuffer_object(").unwrap()..];
    assert_eq!(entries.matches("shade_material(").count(), 4);
    for lighting in ["scene_finish_shading(", "scene_surface_lighting("] {
        assert!(
            !entries.contains(lighting),
            "a G-buffer entry calls {lighting}"
        );
    }
    let forward = crate::shader::ENVIRONMENT_WGSL;
    let shade = &forward[forward.find("fn shade(").unwrap()..];
    let shade = &shade[..shade.find("\n}\n").unwrap()];
    assert!(shade.contains("scene_finish_shading(input, shade_material("));
}

#[test]
fn card_gbuffer_material_is_the_frozen_card_shading_without_its_lighting() {
    let source = super::object_source(crate::shader::ENVIRONMENT_WGSL);
    let card =
        crate::scene_metadata::receiver::function_source(&source, "scene_card_gbuffer_material");
    let frozen = crate::scene_metadata::receiver::function_source(
        crate::shader::ENVIRONMENT_WGSL,
        "scene_card_shade",
    );
    for removed in ["scene_prepare_card_shadow(", "scene_card_finish_shading("] {
        assert!(frozen.contains(removed));
        assert!(!card.contains(removed), "{removed}");
    }
    // The card material is the frozen body line for line, without the card shadow preparation
    // and the lighting, with its own signature and a material return: every other frozen
    // statement (the fade and visibility discards included) is kept, in order.
    let dropped = [
        "let front_shadow = scene_prepare_card_shadow(input.world_position, projected.normal);",
        "var back_shadow: SceneCardShadow;",
        "if bitcast<f32>(material.geometry_flags.y) != 0.0 && frame.grading.y == 0.0 {",
        "back_shadow = scene_prepare_card_shadow(input.world_position, -projected.normal);",
        "let shaded = scene_card_finish_shading(input, surface_material, scattering.y, \
         auxiliary.y, front_shadow, back_shadow);",
    ];
    let frozen_lines: Vec<&str> = frozen.lines().map(str::trim).collect();
    let back_close = frozen_lines
        .iter()
        .position(|line| *line == dropped[3])
        .expect("the back-face shadow preparation")
        + 1;
    assert_eq!(
        frozen_lines[back_close], "}",
        "the back-face shadow block closes"
    );
    let mut expected: Vec<&str> = frozen_lines
        .iter()
        .enumerate()
        .filter(|(index, line)| !dropped.contains(line) && *index != back_close)
        .map(|(_, line)| *line)
        .collect();
    assert_eq!(
        frozen_lines.len() - expected.len(),
        dropped.len() + 1,
        "every dropped frozen line is present once"
    );
    expected[0] = "fn scene_card_gbuffer_material(original: VertexOutput, front_facing: bool, \
                   position: vec3<f32>, masked: bool, visible: bool) -> SceneObjectMaterial {";
    let frozen_return = expected.len() - 2;
    assert_eq!(
        expected[frozen_return],
        "return vec4<f32>(shaded.rgb, select(1.0, coverage_alpha, masked));"
    );
    expected[frozen_return] = "return SceneObjectMaterial(geometric_normal, surface_material, scattering.y, auxiliary.y);";
    let lines: Vec<&str> = card.lines().map(str::trim).collect();
    assert_eq!(lines, expected);
}

#[test]
fn renderer_builds_gbuffer_targets_for_a_producer_source() {
    for velocity in [false, true] {
        let (object, terrain) = super::sources(velocity);
        let mut expected = crate::gbuffer::GBUFFER_FORMATS.to_vec();
        if velocity {
            expected.push(worldrunner_render_post::VELOCITY_FORMAT);
        }
        for source in [&object, &terrain] {
            let formats: Vec<_> = crate::renderer::base_source_targets_for_test(
                source,
                wgpu::TextureFormat::Rgba16Float,
            )
            .into_iter()
            .map(|target| target.expect("target").format)
            .collect();
            assert_eq!(formats, expected, "velocity {velocity}");
            assert_eq!(source.contains(crate::velocity::MARKER), velocity);
        }
    }
}

/// The four targets followed by the object-motion residual at location 4.
const VELOCITY_TARGETS: [(u32, ScalarKind, u32); 5] = [
    TARGETS[0],
    TARGETS[1],
    TARGETS[2],
    TARGETS[3],
    (4, ScalarKind::Float, 2),
];

#[test]
fn velocity_producers_write_the_four_targets_and_the_residual() {
    let (object, terrain) = super::sources(true);
    let objects = module("object G-buffer with velocity", &object);
    for entry in super::FORWARD_OBJECTS {
        assert_eq!(
            fragment_outputs(&objects, entry),
            VELOCITY_TARGETS.to_vec(),
            "{entry}"
        );
        assert_eq!(
            writes_depth(&objects, entry),
            entry.starts_with("fs_relief"),
            "{entry}"
        );
    }
    // The residual is the forward velocity entries' own, over the previous-transform vertex
    // stage, and the stored targets are the plain producer's.
    assert!(object.contains("fn scene_object_velocity(input: VertexOutput)"));
    assert!(object.contains("previous: PreviousInstance"));
    assert_eq!(object.matches("scene_object_velocity(input)").count(), 4);
    for (index, source) in crate::terrain::program::source::test_sources(&terrain)
        .iter()
        .enumerate()
    {
        let module = module(&format!("terrain variant {index} with velocity"), source);
        assert_eq!(
            fragment_outputs(&module, crate::shader::TERRAIN_FRAGMENT_ENTRY_POINT),
            VELOCITY_TARGETS.to_vec(),
            "terrain variant {index}"
        );
    }
    assert!(terrain.contains("SceneVelocityOutput(t.g0, t.g1, t.g2, t.g3, vec2<f32>(0.0))"));
}

#[test]
fn object_gbuffer_normalizes_the_folded_normal_at_storage() {
    let source = include_str!("objects.wgsl");
    assert!(
        source.contains("select(evaluated.geometric_normal, normalize(evaluated.surface.normal),")
    );
    assert!(source.contains("length(evaluated.surface.normal) > 0.000001"));
}
