//! Values the tests build the way Unity's serialized shader describes them.

use worldrunner_unity_serialized::shader_state::{
    Depth, Fog, ShaderState, StateValue, Stencil, StencilOps, TargetBlend,
};

pub fn fixed(value: f32) -> StateValue {
    StateValue {
        value,
        property: None,
    }
}

pub fn named(value: f32, property: &str) -> StateValue {
    StateValue {
        value,
        property: Some(property.to_owned()),
    }
}

fn stencil_ops() -> StencilOps {
    StencilOps {
        pass: fixed(0.0),
        fail: fixed(0.0),
        z_fail: fixed(0.0),
        compare: fixed(8.0),
    }
}

fn target_blend() -> TargetBlend {
    TargetBlend {
        src: fixed(1.0),
        dst: fixed(0.0),
        src_alpha: fixed(1.0),
        dst_alpha: fixed(0.0),
        op: fixed(0.0),
        op_alpha: fixed(0.0),
        color_mask: fixed(15.0),
    }
}

/// A pass with no blending, back-face culling, `ZTest LEqual` and `ZWrite On`.
pub fn opaque_state() -> ShaderState {
    ShaderState {
        name: String::new(),
        blend: (0..8).map(|_| target_blend()).collect(),
        separate_blend: false,
        depth: Depth {
            clip: fixed(1.0),
            test: fixed(4.0),
            write: fixed(1.0),
            cull: fixed(2.0),
            conservative: fixed(0.0),
            offset_factor: fixed(0.0),
            offset_units: fixed(0.0),
            alpha_to_mask: fixed(0.0),
        },
        stencil: Stencil {
            both: stencil_ops(),
            front: stencil_ops(),
            back: stencil_ops(),
            read_mask: fixed(255.0),
            write_mask: fixed(255.0),
            reference: fixed(0.0),
        },
        fog: Fog {
            start: fixed(0.0),
            end: fixed(0.0),
            density: fixed(0.0),
            color: [fixed(0.0), fixed(0.0), fixed(0.0), fixed(0.0)],
            color_property: None,
            mode: 0,
        },
        gpu_program_id: 0,
        tags: Vec::new(),
        lod: 0,
        lighting: false,
    }
}
