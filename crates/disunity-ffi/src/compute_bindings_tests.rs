//! The flat layout of a dispatch record refuses what it has no kind for.
use super::*;
use worldrunner_world_simulation::graphics::{PropertyValue, ShaderProperties};

fn bindings(value: PropertyValue) -> ComputeBindings {
    let mut properties = ShaderProperties::default();
    properties.set(value).unwrap();
    ComputeBindings {
        properties,
        buffers: Vec::new(),
    }
}

fn refusal(value: PropertyValue) -> String {
    match Flat::of(&bindings(value)) {
        Err(error) => error.to_string(),
        Ok(flat) => panic!("{} entries were laid out", flat.entries.len()),
    }
}

#[test]
fn a_vector_or_matrix_array_is_refused_by_name_and_not_dropped() {
    let vector = refusal(PropertyValue::VectorArray("_Vs".into(), vec![[1.0; 4]]));
    assert!(
        vector.contains("_Vs") && vector.contains("array"),
        "{vector}"
    );
    let matrix = refusal(PropertyValue::MatrixArray("_Ms".into(), vec![[1.0; 16]]));
    assert!(
        matrix.contains("_Ms") && matrix.contains("array"),
        "{matrix}"
    );
}

#[test]
fn a_vector_binding_still_lays_out_as_four_floats() {
    let vector = PropertyValue::Vector("_V".into(), [1.0, 2.0, 3.0, 4.0]);
    let flat = Flat::of(&bindings(vector)).unwrap();
    assert_eq!(flat.entries.len(), 1);
    assert_eq!(flat.floats, [1.0, 2.0, 3.0, 4.0]);
}
