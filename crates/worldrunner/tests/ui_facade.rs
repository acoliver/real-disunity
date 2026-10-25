#![cfg(feature = "ui")]

use worldrunner::simulation::{World, WorldSettings};
use worldrunner::ui::{
    FontBook, UiLimits,
    plan::{
        SceneTexture,
        ui::{UiRenderPlan, extract_ui},
    },
};

#[test]
fn cpu_ui_feature_exposes_layout_and_render_planning() {
    let world = World::new(WorldSettings::default()).unwrap();
    let fonts = FontBook::new(UiLimits::default()).unwrap();
    let plan: UiRenderPlan = extract_ui(&world, &fonts, [800, 600]).unwrap();
    let textures: Vec<SceneTexture> = plan
        .texts
        .into_iter()
        .filter_map(|text| text.texture)
        .collect();
    assert!(textures.is_empty());
    assert!(plan.layout.rects.is_empty());
}
