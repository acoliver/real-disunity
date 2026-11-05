//! Render state resolved from a pass's `ShaderState` and material properties.

use wgpu::{BlendFactor, BlendOperation, ColorWrites, CompareFunction, Face};
use worldrunner_render_program::properties::{PropertySet, PropertyValue};
use worldrunner_render_program::state::{StateError, resolve_state};
use worldrunner_unity_serialized::shader_state::ShaderState;

use crate::common::{fixed, named, opaque_state};

type Change = fn(&mut ShaderState);

fn floats(values: &[(&str, f32)]) -> PropertySet {
    let mut set = PropertySet::new();
    for (name, value) in values {
        set.set(name, PropertyValue::Float(*value));
    }
    set
}

#[test]
fn fixed_values_give_the_opaque_defaults() {
    let resolved = resolve_state(&opaque_state(), &[]).unwrap();
    assert_eq!(resolved.state.cull, Some(Face::Back));
    assert_eq!(resolved.state.depth_compare, CompareFunction::LessEqual);
    assert!(resolved.state.depth_write);
    assert_eq!(resolved.state.color_write, ColorWrites::ALL);
    assert_eq!(
        resolved.state.blend, None,
        "One/Zero with Add is blending off"
    );
    assert!(resolved.defaulted.is_empty());
}

#[test]
fn a_property_overrides_the_default_value() {
    let mut state = opaque_state();
    state.depth.cull = named(2.0, "_Cull");
    state.depth.test = named(4.0, "_ZTest");
    state.depth.write = named(1.0, "_ZWrite");
    let material = floats(&[("_Cull", 0.0), ("_ZTest", 8.0), ("_ZWrite", 0.0)]);
    let resolved = resolve_state(&state, &[&material]).unwrap();
    assert_eq!(resolved.state.cull, None);
    assert_eq!(resolved.state.depth_compare, CompareFunction::Always);
    assert!(!resolved.state.depth_write);
    let front = floats(&[("_Cull", 1.0)]);
    let resolved = resolve_state(&state, &[&front, &material]).unwrap();
    assert_eq!(
        resolved.state.cull,
        Some(Face::Front),
        "the first layer wins"
    );
}

#[test]
fn a_missing_property_uses_the_default_and_is_reported() {
    let mut state = opaque_state();
    state.depth.cull = named(2.0, "_Cull");
    let resolved = resolve_state(&state, &[&PropertySet::new()]).unwrap();
    assert_eq!(resolved.state.cull, Some(Face::Back));
    assert_eq!(resolved.defaulted, vec!["_Cull".to_owned()]);
}

#[test]
fn every_unity_compare_function_maps_to_its_wgpu_counterpart() {
    let expected = [
        CompareFunction::Always,
        CompareFunction::Never,
        CompareFunction::Less,
        CompareFunction::Equal,
        CompareFunction::LessEqual,
        CompareFunction::Greater,
        CompareFunction::NotEqual,
        CompareFunction::GreaterEqual,
        CompareFunction::Always,
    ];
    for (unity, wanted) in (0..).zip(expected) {
        let mut state = opaque_state();
        state.depth.test = fixed(f32::from(u8::try_from(unity).unwrap()));
        assert_eq!(
            resolve_state(&state, &[]).unwrap().state.depth_compare,
            wanted,
            "ZTest {unity}"
        );
    }
}

#[test]
fn blend_factors_follow_unitys_blend_mode_numbering() {
    use BlendFactor::{
        Dst, DstAlpha, One, OneMinusDst, OneMinusDstAlpha, OneMinusSrc, OneMinusSrcAlpha, Src,
        SrcAlpha, SrcAlphaSaturated, Zero,
    };
    let expected = [
        Zero,
        One,
        Dst,
        Src,
        OneMinusDst,
        SrcAlpha,
        OneMinusSrc,
        DstAlpha,
        OneMinusDstAlpha,
        SrcAlphaSaturated,
        OneMinusSrcAlpha,
    ];
    for (unity, wanted) in (0u8..).zip(expected) {
        let mut state = opaque_state();
        state.blend[0].src = fixed(f32::from(unity));
        state.blend[0].dst_alpha = fixed(f32::from(unity));
        let blend = resolve_state(&state, &[]).unwrap().state.blend.unwrap();
        assert_eq!(blend.color.src_factor, wanted, "BlendMode {unity}");
        assert_eq!(blend.alpha.dst_factor, wanted, "BlendMode {unity}");
    }
}

#[test]
fn alpha_blending_comes_from_material_properties() {
    let mut state = opaque_state();
    state.blend[0].src = named(1.0, "_SrcBlend");
    state.blend[0].dst = named(0.0, "_DstBlend");
    let material = floats(&[("_SrcBlend", 5.0), ("_DstBlend", 10.0)]);
    let blend = resolve_state(&state, &[&material])
        .unwrap()
        .state
        .blend
        .unwrap();
    assert_eq!(blend.color.src_factor, BlendFactor::SrcAlpha);
    assert_eq!(blend.color.dst_factor, BlendFactor::OneMinusSrcAlpha);
    assert_eq!(blend.color.operation, BlendOperation::Add);
    assert_eq!(
        blend.alpha.src_factor,
        BlendFactor::One,
        "alpha factors stay fixed"
    );
}

#[test]
fn blend_operations_follow_unitys_numbering() {
    let expected = [
        BlendOperation::Add,
        BlendOperation::Subtract,
        BlendOperation::ReverseSubtract,
    ];
    for (unity, wanted) in (0u8..).zip(expected) {
        let mut state = opaque_state();
        state.blend[0].op = fixed(f32::from(unity));
        state.blend[0].src = fixed(5.0);
        let blend = resolve_state(&state, &[]).unwrap().state.blend.unwrap();
        assert_eq!(blend.color.operation, wanted, "BlendOp {unity}");
    }
}

#[test]
fn min_and_max_ignore_factors_as_in_unity() {
    for (unity, wanted) in [(3.0, BlendOperation::Min), (4.0, BlendOperation::Max)] {
        let mut state = opaque_state();
        state.blend[0].op = fixed(unity);
        state.blend[0].src = fixed(5.0);
        state.blend[0].dst = fixed(10.0);
        let blend = resolve_state(&state, &[]).unwrap().state.blend.unwrap();
        assert_eq!(blend.color.operation, wanted);
        assert_eq!(blend.color.src_factor, BlendFactor::One);
        assert_eq!(blend.color.dst_factor, BlendFactor::One);
    }
}

#[test]
fn the_color_mask_translates_unitys_bits() {
    let cases = [
        (15.0, ColorWrites::ALL),
        (8.0, ColorWrites::RED),
        (4.0, ColorWrites::GREEN),
        (2.0, ColorWrites::BLUE),
        (1.0, ColorWrites::ALPHA),
        (0.0, ColorWrites::empty()),
        (10.0, ColorWrites::RED | ColorWrites::BLUE),
    ];
    for (unity, wanted) in cases {
        let mut state = opaque_state();
        state.blend[0].color_mask = fixed(unity);
        assert_eq!(
            resolve_state(&state, &[]).unwrap().state.color_write,
            wanted
        );
    }
}

#[test]
fn unknown_values_are_named_errors() {
    let cases: [(&str, Change, f32); 7] = [
        ("cull", |s| s.depth.cull = fixed(3.0), 3.0),
        ("ZTest", |s| s.depth.test = fixed(9.0), 9.0),
        ("ZWrite", |s| s.depth.write = fixed(2.0), 2.0),
        ("blend factor", |s| s.blend[0].dst = fixed(11.0), 11.0),
        ("blend operation", |s| s.blend[0].op = fixed(5.0), 5.0),
        ("color mask", |s| s.blend[0].color_mask = fixed(16.0), 16.0),
        ("cull", |s| s.depth.cull = fixed(1.5), 1.5),
    ];
    for (field, change, value) in cases {
        let mut state = opaque_state();
        change(&mut state);
        assert_eq!(
            resolve_state(&state, &[]).unwrap_err(),
            StateError::UnknownValue { field, value }
        );
    }
    let mut state = opaque_state();
    state.depth.cull = fixed(f32::NAN);
    assert!(matches!(
        resolve_state(&state, &[]),
        Err(StateError::UnknownValue { field: "cull", .. })
    ));
}

#[test]
fn a_property_that_is_not_a_float_is_a_named_error() {
    let mut state = opaque_state();
    state.depth.cull = named(2.0, "_Cull");
    let mut material = PropertySet::new();
    material.set("_Cull", PropertyValue::Vector([2.0; 4]));
    assert_eq!(
        resolve_state(&state, &[&material]).unwrap_err(),
        StateError::NotAFloat {
            property: "_Cull".to_owned()
        }
    );
}

#[test]
fn state_this_slice_does_not_draw_is_a_named_error() {
    let cases: [(&str, Change); 6] = [
        ("stencil", |s| s.stencil.both.compare = fixed(3.0)),
        ("stencil", |s| s.stencil.front.pass = fixed(2.0)),
        ("depth offset", |s| s.depth.offset_units = fixed(-1.0)),
        ("depth offset", |s| s.depth.offset_factor = fixed(1.0)),
        ("alpha to coverage", |s| s.depth.alpha_to_mask = fixed(1.0)),
        ("depth clip", |s| s.depth.clip = fixed(0.0)),
    ];
    for (field, change) in cases {
        let mut state = opaque_state();
        change(&mut state);
        assert!(
            matches!(
                resolve_state(&state, &[]),
                Err(StateError::Unsupported { field: f, .. }) if f == field
            ),
            "{field}"
        );
    }
}

#[test]
fn an_integer_property_is_a_state_value_too() {
    let mut state = opaque_state();
    state.depth.cull = named(2.0, "_Cull");
    state.depth.test = named(4.0, "_ZTest");
    let mut material = PropertySet::new();
    material.set("_Cull", PropertyValue::Int(1));
    material.set("_ZTest", PropertyValue::Int(i32::MAX));
    assert_eq!(
        resolve_state(&state, &[&material]).unwrap_err(),
        StateError::IntegerOutOfRange {
            property: "_ZTest".to_owned(),
            value: i32::MAX
        }
    );
    material.set("_ZTest", PropertyValue::Int(5));
    let resolved = resolve_state(&state, &[&material]).unwrap();
    assert_eq!(resolved.state.cull, Some(Face::Front));
    assert_eq!(resolved.state.depth_compare, CompareFunction::Greater);
}
