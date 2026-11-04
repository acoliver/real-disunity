use glam::{UVec2, Vec2};
use worldrunner_world_simulation::ui::{
    Canvas, RectTransform, UiControl, UiLayout, UiRouter, UiSemantics,
};
use worldrunner_world_simulation::{
    ComponentDesc, ObjectDesc, ObjectHandle, ParentSpace, World, WorldSettings,
};

pub fn rect(w: &mut World, parent: Option<ObjectHandle>, y: f32) -> ObjectHandle {
    let object = w.create_object(ObjectDesc::default()).unwrap();
    w.add_component(
        object,
        ComponentDesc::RectTransform(RectTransform {
            size_delta: Vec2::new(100., 20.),
            anchored_position: Vec2::new(0., y),
            ..Default::default()
        }),
    )
    .unwrap();
    w.set_parent(object, parent, ParentSpace::KeepLocal)
        .unwrap();
    object
}
pub fn fixture() -> (World, UiRouter, ObjectHandle, ObjectHandle) {
    let mut w = World::new(WorldSettings::default()).unwrap();
    let root = rect(&mut w, None, 0.);
    w.add_component(root, ComponentDesc::Canvas(Canvas::default()))
        .unwrap();
    let button = rect(&mut w, Some(root), 0.);
    let mut r = UiRouter::default();
    r.controls.insert(button, UiControl::button());
    r.semantics.insert(button, UiSemantics::label("Play"));
    (w, r, root, button)
}
pub fn layout(w: &World, r: &UiRouter) -> UiLayout {
    w.ui_layout_with(UVec2::new(400, 300), &r.layout).unwrap()
}
