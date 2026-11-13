//! Deliberate uncovered loads in live consumer functions, confined to tests.
use super::ReceiverLayout;

#[derive(Clone, Copy, Debug)]
pub(crate) enum ConsumerFamily {
    SurfaceFront,
    SurfaceBack,
    CardFront,
    CardBack,
    Terrain,
}

impl ConsumerFamily {
    pub(crate) const ALL: [Self; 5] = [
        Self::SurfaceFront,
        Self::SurfaceBack,
        Self::CardFront,
        Self::CardBack,
        Self::Terrain,
    ];

    pub(crate) const fn side(self, layout: ReceiverLayout) -> u32 {
        if matches!(self, Self::SurfaceBack | Self::CardBack)
            && matches!(layout, ReceiverLayout::Opposing)
        {
            1
        } else {
            0
        }
    }
}

pub(crate) fn source(
    source: &str,
    layout: ReceiverLayout,
    family: ConsumerFamily,
    terrain: bool,
) -> String {
    if terrain != matches!(family, ConsumerFamily::Terrain) {
        return source.to_owned();
    }
    let name = match family {
        ConsumerFamily::SurfaceFront | ConsumerFamily::Terrain => "cascaded_sunlight_visibility",
        ConsumerFamily::SurfaceBack => "scene_receiver_back_cascaded",
        ConsumerFamily::CardFront => "scene_prepare_card_shadow",
        ConsumerFamily::CardBack => "scene_receiver_back_card",
    };
    let side = family.side(layout);
    let body = super::function_source(source, name);
    let load = format!("scene_receiver_load({side})");
    assert!(
        body.contains(&load),
        "{name} must consume its payload layer"
    );
    let changed = body.replace(&load, &format!("scene_uncovered_load({side})"));
    assert!(
        source.contains("vec4<f32>(published_environment, 1.0), vec4<f32>(published_factor, 1.0)")
    );
    let source = source.replace(body, &changed).replace(
        "vec4<f32>(published_environment, 1.0), vec4<f32>(published_factor, 1.0)",
        "scene_uncovered_a, vec4<f32>(scene_uncovered_b, 0.0, 0.0)",
    );
    format!(
        "{source}
var<private> scene_uncovered_a: vec4<f32>;
var<private> scene_uncovered_b: vec2<f32>;
fn scene_uncovered_load(side: i32) -> SceneReceiverDisplacements {{
    scene_uncovered_a = textureLoad(receiver_a, vec2<i32>(0), side, 0);
    scene_uncovered_b = textureLoad(receiver_b, vec2<i32>(0), side, 0).xy;
    return SceneReceiverDisplacements(scene_uncovered_a.xyz,
        vec3<f32>(scene_uncovered_a.w, scene_uncovered_b));
}}"
    )
}
