//! Ordered vector and matrix arrays as shader properties: what a set of properties, the global
//! state, a material, a property block and recorded offscreen work hold and refuse.

use worldrunner_world_simulation::graphics::{
    ComputeKernel, GraphicsError, GraphicsSnapshot, MAX_PROPERTIES_PER_KIND,
    MAX_PROPERTY_ARRAY_ELEMENTS, MaterialEdit, OffscreenWork, PropertyValue, RenderTextureId,
    ShaderProperties, ShaderSource, WorldComputeShader, WorldRenderTexture, WorldSampler,
    WorldShader,
};
use worldrunner_world_simulation::{World, WorldError, WorldSettings};

fn world() -> World {
    World::new(WorldSettings::default()).unwrap()
}

fn vectors(count: usize) -> Vec<[f32; 4]> {
    (0..count)
        .map(|index| {
            let base = f32::from(u16::try_from(index).unwrap());
            [base, base + 0.25, -base, 1.0]
        })
        .collect()
}

fn matrices(count: usize) -> Vec<[f32; 16]> {
    (0..count)
        .map(|index| {
            let base = f32::from(u16::try_from(index).unwrap());
            std::array::from_fn(|slot| base + 0.5 * f32::from(u8::try_from(slot).unwrap()))
        })
        .collect()
}

fn vector_array(name: &str, count: usize) -> PropertyValue {
    PropertyValue::VectorArray(name.into(), vectors(count))
}

fn matrix_array(name: &str, count: usize) -> PropertyValue {
    PropertyValue::MatrixArray(name.into(), matrices(count))
}

fn graphics_error(result: Result<(), WorldError>) -> GraphicsError {
    match result {
        Err(WorldError::Graphics(error)) => error,
        other => panic!("expected a graphics error, got {other:?}"),
    }
}

#[test]
fn global_arrays_of_one_and_of_the_most_are_stored_in_element_order() {
    let mut world = world();
    world.set_global_property(vector_array("_V1", 1)).unwrap();
    world
        .set_global_property(vector_array("_VMax", MAX_PROPERTY_ARRAY_ELEMENTS))
        .unwrap();
    world.set_global_property(matrix_array("_M1", 1)).unwrap();
    world
        .set_global_property(matrix_array("_MMax", MAX_PROPERTY_ARRAY_ELEMENTS))
        .unwrap();
    assert_eq!(MAX_PROPERTY_ARRAY_ELEMENTS, 1023);

    let properties = &world.global_shader_state().properties;
    assert_eq!(properties.vector_array("_V1"), Some(vectors(1)));
    assert_eq!(properties.vector_array("_VMax"), Some(vectors(1023)));
    assert_eq!(properties.matrix_array("_M1"), Some(matrices(1)));
    assert_eq!(properties.matrix_array("_MMax"), Some(matrices(1023)));
    assert_eq!(properties.vector_array("_Missing"), None);
    assert_eq!(world.global_shader_state().generation, 4);
}

#[test]
fn an_array_of_no_elements_or_of_too_many_is_refused_and_changes_nothing() {
    let mut world = world();
    world.set_global_property(vector_array("_A", 2)).unwrap();
    let before = world.global_shader_state().clone();
    let refused = [
        vector_array("_A", 0),
        vector_array("_A", 1024),
        matrix_array("_A", 0),
        matrix_array("_B", 1024),
    ];
    for value in refused {
        let result = world.set_global_property(value);
        assert_eq!(
            graphics_error(result),
            GraphicsError::InvalidPropertyArrayLength
        );
        assert_eq!(world.global_shader_state(), &before);
    }
}

#[test]
fn a_non_finite_component_anywhere_is_refused_and_changes_nothing() {
    let mut world = world();
    let before = world.global_shader_state().clone();
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for slot in 0..4 {
            let mut values = vectors(3);
            values[2][slot] = bad;
            let value = PropertyValue::VectorArray("_V".into(), values);
            assert_eq!(
                graphics_error(world.set_global_property(value)),
                GraphicsError::NotFinite
            );
        }
        for slot in 0..16 {
            let mut values = matrices(3);
            values[1][slot] = bad;
            let value = PropertyValue::MatrixArray("_M".into(), values);
            assert_eq!(
                graphics_error(world.set_global_property(value)),
                GraphicsError::NotFinite
            );
        }
    }
    assert_eq!(world.global_shader_state(), &before);
}

#[test]
fn an_array_name_of_no_bytes_or_of_over_256_is_refused() {
    let mut world = world();
    let long = "n".repeat(257);
    for name in [String::new(), long] {
        let vector = PropertyValue::VectorArray(name.clone(), vectors(1));
        let matrix = PropertyValue::MatrixArray(name, matrices(1));
        for value in [vector, matrix] {
            assert_eq!(
                graphics_error(world.set_global_property(value)),
                GraphicsError::InvalidName
            );
        }
    }
    assert_eq!(world.global_shader_state().generation, 0);
    let longest = "n".repeat(256);
    world
        .set_global_property(PropertyValue::VectorArray(longest, vectors(1)))
        .unwrap();
}

#[test]
fn a_shorter_replacement_leaves_no_tail_and_the_kinds_are_separate() {
    let mut world = world();
    world.set_global_property(vector_array("_Same", 5)).unwrap();
    world.set_global_property(matrix_array("_Same", 4)).unwrap();
    world
        .set_global_property(PropertyValue::Vector("_Same".into(), [9.0; 4]))
        .unwrap();
    world.set_global_property(vector_array("_Same", 2)).unwrap();

    let properties = &world.global_shader_state().properties;
    assert_eq!(properties.vector_array("_Same"), Some(vectors(2)));
    assert_eq!(properties.matrix_array("_Same"), Some(matrices(4)));
    assert_eq!(properties.vector("_Same"), Some([9.0; 4]));
    assert_eq!(world.global_shader_state().generation, 4);
}

#[test]
fn arrays_are_kept_sorted_by_name_and_getters_return_copies() {
    let mut properties = ShaderProperties::default();
    for name in ["_b", "_c", "_a"] {
        properties.set(vector_array(name, 1)).unwrap();
        properties.set(matrix_array(name, 1)).unwrap();
    }
    let names: Vec<_> = properties
        .vector_arrays
        .iter()
        .map(|(n, _)| n.as_str())
        .collect();
    assert_eq!(names, ["_a", "_b", "_c"]);
    let names: Vec<_> = properties
        .matrix_arrays
        .iter()
        .map(|(n, _)| n.as_str())
        .collect();
    assert_eq!(names, ["_a", "_b", "_c"]);

    let mut copy = properties.vector_array("_a").unwrap();
    copy[0] = [7.0; 4];
    assert_eq!(properties.vector_array("_a"), Some(vectors(1)));
}

#[test]
fn a_set_of_properties_holds_1024_array_names_per_kind_and_a_merge_may_hold_more() {
    let mut base = ShaderProperties::default();
    let mut over = ShaderProperties::default();
    for index in 0..MAX_PROPERTIES_PER_KIND {
        base.set(vector_array(&format!("_base{index:04}"), 1))
            .unwrap();
        over.set(matrix_array(&format!("_over{index:04}"), 1))
            .unwrap();
        over.set(vector_array(&format!("_over{index:04}"), 1))
            .unwrap();
    }
    let refused = base.set(vector_array("_one_more", 1));
    assert_eq!(refused, Err(GraphicsError::TooManyProperties));
    base.set(vector_array("_base0000", 3)).unwrap();
    assert_eq!(base.vector_array("_base0000"), Some(vectors(3)));
    base.set(matrix_array("_other_kind", 1)).unwrap();

    let merged = base.merged_with(&over);
    assert_eq!(merged.vector_arrays.len(), 2 * MAX_PROPERTIES_PER_KIND);
    assert_eq!(merged.matrix_arrays.len(), MAX_PROPERTIES_PER_KIND + 1);
    let sorted = merged
        .vector_arrays
        .windows(2)
        .all(|pair| pair[0].0 < pair[1].0);
    assert!(sorted);
}

#[test]
fn a_merge_lets_the_upper_set_win_by_name_within_each_array_kind() {
    let mut base = ShaderProperties::default();
    base.set(vector_array("_Both", 2)).unwrap();
    base.set(vector_array("_Low", 1)).unwrap();
    base.set(matrix_array("_Both", 2)).unwrap();
    let mut over = ShaderProperties::default();
    over.set(vector_array("_Both", 3)).unwrap();
    over.set(matrix_array("_High", 1)).unwrap();

    let merged = base.merged_with(&over);
    assert_eq!(merged.vector_array("_Both"), Some(vectors(3)));
    assert_eq!(merged.vector_array("_Low"), Some(vectors(1)));
    assert_eq!(merged.matrix_array("_Both"), Some(matrices(2)));
    assert_eq!(merged.matrix_array("_High"), Some(matrices(1)));
}

#[test]
fn a_set_holding_only_an_array_is_not_empty() {
    let mut properties = ShaderProperties::default();
    assert!(properties.is_empty());
    properties.set(matrix_array("_M", 1)).unwrap();
    assert!(!properties.is_empty());
    let mut properties = ShaderProperties::default();
    properties.set(vector_array("_V", 1)).unwrap();
    assert!(!properties.is_empty());
}

#[test]
fn clearing_global_properties_removes_arrays() {
    let mut world = world();
    world.set_global_property(vector_array("_V", 2)).unwrap();
    world.set_global_property(matrix_array("_M", 2)).unwrap();
    world.clear_global_properties();
    assert!(world.global_shader_state().properties.is_empty());
}

fn shader_material(world: &mut World) -> worldrunner_world_simulation::graphics::MaterialId {
    let shader = world
        .create_shader(WorldShader {
            name: "Arrays".into(),
            source: None,
        })
        .unwrap();
    world.create_material(shader).unwrap()
}

#[test]
fn a_material_holds_arrays_and_a_refused_array_leaves_it_and_its_generation() {
    let mut world = world();
    let material = shader_material(&mut world);
    world
        .edit_material(material, MaterialEdit::Property(vector_array("_V", 3)))
        .unwrap();
    world
        .edit_material(material, MaterialEdit::Property(matrix_array("_M", 2)))
        .unwrap();
    let held = world.material(material).unwrap().clone();
    assert_eq!(held.properties.vector_array("_V"), Some(vectors(3)));
    assert_eq!(held.properties.matrix_array("_M"), Some(matrices(2)));
    assert_eq!(held.generation, 3, "creation is generation one");

    let refused = world.edit_material(material, MaterialEdit::Property(vector_array("_V", 0)));
    assert_eq!(
        graphics_error(refused),
        GraphicsError::InvalidPropertyArrayLength
    );
    assert_eq!(world.material(material).unwrap(), &held);
    world
        .edit_material(material, MaterialEdit::ClearProperties)
        .unwrap();
    assert!(world.material(material).unwrap().properties.is_empty());
}

fn render_texture(world: &mut World) -> RenderTextureId {
    world
        .create_render_texture(WorldRenderTexture {
            width: 4,
            height: 4,
            format: 8,
            depth_bits: 0,
            mip_count: 1,
            srgb: false,
            dimension: 2,
            volume_depth: 1,
            sampler: WorldSampler::default(),
            generation: 0,
        })
        .unwrap()
}

#[test]
fn recorded_array_work_validates_keeps_order_and_leaves_the_initial_globals_alone() {
    let mut world = world();
    let target = render_texture(&mut world);
    world.set_global_property(vector_array("_Init", 2)).unwrap();
    let generation = world.global_shader_state().generation;

    let first = OffscreenWork::SetGlobal(vector_array("_Ordered", 3));
    let second = OffscreenWork::SetGlobal(matrix_array("_Ordered", 2));
    let third = OffscreenWork::SetGlobal(vector_array("_Ordered", 1));
    for work in [first.clone(), second.clone(), third.clone()] {
        world.record_offscreen(work).unwrap();
    }
    let refused = world.record_offscreen(OffscreenWork::SetGlobal(vector_array("_Ordered", 0)));
    assert_eq!(
        graphics_error(refused),
        GraphicsError::InvalidPropertyArrayLength
    );
    let refused = world.record_offscreen(OffscreenWork::SetGlobal(PropertyValue::MatrixArray(
        "_Ordered".into(),
        vec![[f32::NAN; 16]],
    )));
    assert_eq!(graphics_error(refused), GraphicsError::NotFinite);
    assert_eq!(world.offscreen_work().len(), 3);

    assert_eq!(world.global_shader_state().generation, generation);
    let global = &world.global_shader_state().properties;
    assert_eq!(global.vector_array("_Ordered"), None);
    assert_eq!(global.matrix_array("_Ordered"), None);

    world
        .record_offscreen(OffscreenWork::Blit {
            source: worldrunner_world_simulation::graphics::TextureRef::None,
            target,
            material: None,
            pass: -1,
        })
        .unwrap();
    let snapshot = GraphicsSnapshot::take(&mut world).unwrap();
    assert_eq!(&snapshot.work[..3], [first, second, third]);
    let initial = &snapshot.global.unwrap().properties;
    assert_eq!(initial.vector_array("_Init"), Some(vectors(2)));
    assert_eq!(initial.vector_array("_Ordered"), None);
}

#[test]
fn compute_bindings_merge_shader_wide_and_kernel_arrays() {
    let mut world = world();
    let kernel = |name: &str| ComputeKernel {
        name: name.into(),
        thread_group_size: Some([8, 8, 1]),
    };
    let id = world
        .create_compute_shader(WorldComputeShader {
            name: "Arrays".into(),
            source: ShaderSource {
                file: "resources.assets".into(),
                path_id: 3,
            },
            kernels: vec![kernel("Main")],
            generation: 0,
        })
        .unwrap();
    world
        .set_compute_property(id, None, vector_array("_Wide", 2))
        .unwrap();
    world
        .set_compute_property(id, Some(0), matrix_array("_Kernel", 3))
        .unwrap();
    let refused = world.set_compute_property(id, Some(0), vector_array("_Wide", 0));
    assert_eq!(
        graphics_error(refused),
        GraphicsError::InvalidPropertyArrayLength
    );

    let bindings = world.compute_bindings(id, 0).unwrap();
    assert_eq!(bindings.properties.vector_array("_Wide"), Some(vectors(2)));
    assert_eq!(
        bindings.properties.matrix_array("_Kernel"),
        Some(matrices(3))
    );
}
