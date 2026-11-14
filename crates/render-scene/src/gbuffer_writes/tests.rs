//! CPU checks of the G-buffer producer sources: every terrain variant compiles and writes the
//! four targets from the same material evaluation the forward path lights.

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
        .map(|member| {
            let Some(Binding::Location { location, .. }) = member.binding else {
                panic!("{name} output {:?} has no location", member.name);
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
            (location, kind, count)
        })
        .collect()
}

fn terrain_variants() -> [String; 4] {
    let base = super::terrain_source(crate::shader::TERRAIN_WGSL);
    crate::terrain::program::source::test_sources(&base)
}

#[test]
fn every_terrain_variant_writes_the_four_gbuffer_targets() {
    let expected = vec![
        (0, ScalarKind::Float, 4),
        (1, ScalarKind::Float, 4),
        (2, ScalarKind::Float, 4),
        (3, ScalarKind::Uint, 1),
    ];
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
    for lighting in ["scene_surface_lighting(", "apply_exposure(", "apply_height_fog("] {
        assert!(!entry.contains(lighting), "the G-buffer entry calls {lighting}");
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
