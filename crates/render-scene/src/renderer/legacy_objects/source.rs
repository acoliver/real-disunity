//! The object shader sources of the fixed path, composed from the snapshots in
//! `object/fixtures` instead of the live WGSL, so that a change to the live shaders is a change
//! to what the captures compare and not something both sides pick up together.
//!
//! The text rewriters that specialise a source for a pass (velocity, ambient occlusion,
//! reflection, the gathering receiver) are the live ones. They only transform text and are
//! not the material code the fixtures pin.

use worldrunner_render_plan::SceneSsaoReceiver;

macro_rules! fixture {
    ($name:literal) => {
        include_str!(concat!("../../object/fixtures/", $name, ".wgsl"))
    };
}

/// The decal, camera ray and geometry files both bases share.
macro_rules! shared_tail {
    () => {
        concat!(
            fixture!("geometry"),
            fixture!("foliage"),
            fixture!("impostor"),
            fixture!("surface_sample"),
            fixture!("impostor_end"),
            fixture!("normal_forward"),
            fixture!("normal_sample"),
            fixture!("environment"),
            fixture!("forward_cards"),
            "\n",
        )
    };
}

/// `shader::ENVIRONMENT_WGSL` as it was at the freeze.
pub(crate) const ENVIRONMENT: &str = concat!(
    fixture!("lighting_crate"),
    // The crate's text ends in two blank lines, which the snapshot file leaves out.
    "\n\n",
    fixture!("post_crate"),
    fixture!("lighting"),
    fixture!("ambient_visibility"),
    fixture!("decals"),
    fixture!("decals_sample"),
    fixture!("camera_rays"),
    fixture!("unshadowed"),
    shared_tail!(),
);

/// The shadow receiver source, as `shadows::resources::SHADOWED_WGSL` was at the freeze.
pub(crate) const SHADOWED: &str = concat!(
    fixture!("shadow_crate"),
    fixture!("lighting_crate"),
    // The crate's text ends in two blank lines, which the snapshot file leaves out.
    "\n\n",
    fixture!("post_crate"),
    fixture!("lighting"),
    fixture!("ambient_visibility"),
    fixture!("decals"),
    fixture!("decals_sample"),
    fixture!("camera_rays"),
    fixture!("shadows"),
    shared_tail!(),
);

/// The pass features a forward object source is specialised for.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Recipe {
    /// Samples the directional shadow map at set two.
    pub shadowed: bool,
    /// Reads the ambient texture at set three.
    pub ssao: bool,
    /// Writes the three-target reflection output.
    pub reflection: bool,
    /// Writes the object-motion residual.
    pub velocity: bool,
}

/// The object source of `recipe`, composed as the fixed contexts composed it: the context
/// constructors in `ScenePipelines` and `shadows::resources::receiver_sources`.
pub(crate) fn compose(recipe: Recipe) -> String {
    let base = if recipe.shadowed {
        SHADOWED
    } else {
        ENVIRONMENT
    };
    if recipe.reflection {
        return crate::ssr::forward::velocity_sources(
            (base, crate::shader::TERRAIN_WGSL),
            recipe.velocity,
            false,
        )
        .0;
    }
    let object = if recipe.velocity {
        crate::velocity::object_source(base)
    } else {
        base.to_owned()
    };
    if recipe.ssao {
        crate::ssao::ambient_source(&object)
    } else {
        object
    }
}

/// `source` with the AO gathering receiver of `receiver`, as `ScenePipelines::for_receiver`
/// composed it.
pub(crate) fn gathered(source: &str, receiver: SceneSsaoReceiver) -> String {
    crate::ssao::receiver::source(source, receiver)
}
