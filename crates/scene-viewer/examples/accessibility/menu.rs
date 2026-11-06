use glam::{UVec2, Vec2};
use worldrunner_world_simulation::ui::{
    Canvas, FontBook, FontId, RectTransform, TextRun, UiAction, UiAxis, UiControl, UiLayout,
    UiLimits, UiRouter, UiSemantics, UiSlider, UiStyle,
};
use worldrunner_world_simulation::{
    ComponentDesc, ComponentHandle, ObjectDesc, ObjectHandle, ParentSpace, World, WorldError,
    WorldSettings,
};

pub struct Menu {
    pub world: World,
    pub router: UiRouter,
    pub fonts: FontBook,
    pub size: [u32; 2],
    status: ComponentHandle,
    open: ObjectHandle,
    dialog: ObjectHandle,
    close: ObjectHandle,
}
impl Menu {
    pub fn new(bytes: Vec<u8>) -> Result<Self, WorldError> {
        let mut fonts = FontBook::new(UiLimits::default())?;
        let font = fonts.register(bytes)?;
        let mut world = World::new(WorldSettings::default())?;
        let root = rectangle(&mut world, None, 0.)?;
        world.add_component(root, ComponentDesc::Canvas(Canvas::default()))?;
        let mut router = UiRouter::default();
        value_controls(&mut world, &mut router, root, font)?;
        let open = control(
            &mut world,
            &mut router,
            root,
            font,
            ("Open dialog", -40.),
            UiControl::button(),
        )?;
        let status_object = rectangle(&mut world, Some(root), -180.)?;
        let status = text(&mut world, status_object, font, "Ready")?;
        router.semantics.insert(
            status_object,
            UiSemantics {
                label: None,
                live: true,
            },
        );
        let dialog = rectangle(&mut world, Some(root), 0.)?;
        router
            .semantics
            .insert(dialog, UiSemantics::label("Example dialog"));
        let close = control(
            &mut world,
            &mut router,
            dialog,
            font,
            ("Close dialog", 0.),
            UiControl::button(),
        )?;
        world.set_active(dialog, false)?;
        router.modals.insert(dialog);
        Ok(Self {
            world,
            router,
            fonts,
            size: [800, 600],
            status,
            open,
            dialog,
            close,
        })
    }
    pub fn layout(&self, size: [u32; 2]) -> Result<UiLayout, String> {
        self.world
            .ui_layout_with(UVec2::from(size), &self.router.layout)
            .map_err(|e| e.to_string())
    }
    pub fn actions(&mut self, actions: Vec<UiAction>) -> Result<(), String> {
        for action in actions {
            if let UiAction::Activated { object } = action
                && (object == self.open || object == self.close)
            {
                self.world
                    .set_active(self.dialog, object == self.open)
                    .map_err(|e| e.to_string())?;
            }
            let ComponentDesc::Text(mut run) = self
                .world
                .component(self.status)
                .map_err(|e| e.to_string())?
                .desc
                .clone()
            else {
                unreachable!("status text");
            };
            run.text = match action {
                UiAction::Activated { object } if object == self.open => "Dialog opened".into(),
                UiAction::Activated { object } if object == self.close => "Dialog closed".into(),
                UiAction::Activated { .. } => "Activated".into(),
                UiAction::Toggled { on, .. } => format!("Sound enabled: {on}"),
                UiAction::ValueChanged { value, .. } => format!("Volume: {value}"),
                _ => "Selection changed".into(),
            };
            self.world
                .set_component(self.status, ComponentDesc::Text(run))
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}
fn rectangle(
    world: &mut World,
    parent: Option<ObjectHandle>,
    y: f32,
) -> Result<ObjectHandle, WorldError> {
    let object = world.create_object(ObjectDesc::default())?;
    world.add_component(
        object,
        ComponentDesc::RectTransform(RectTransform {
            size_delta: Vec2::new(320., 40.),
            anchored_position: Vec2::new(0., y),
            ..Default::default()
        }),
    )?;
    world.set_parent(object, parent, ParentSpace::KeepLocal)?;
    Ok(object)
}
fn text(
    world: &mut World,
    object: ObjectHandle,
    font: FontId,
    label: &str,
) -> Result<ComponentHandle, WorldError> {
    world.add_component(
        object,
        ComponentDesc::Text(TextRun {
            text: label.into(),
            font_size: 24.,
            ..TextRun::new(font)
        }),
    )
}
fn control(
    world: &mut World,
    router: &mut UiRouter,
    parent: ObjectHandle,
    font: FontId,
    (label, y): (&str, f32),
    control: UiControl,
) -> Result<ObjectHandle, WorldError> {
    let object = rectangle(world, Some(parent), y)?;
    text(world, object, font, label)?;
    router.controls.insert(object, control);
    router.styles.insert(
        object,
        UiStyle {
            focused: [1., 1., 0., 1.],
            selected: [0., 1., 0., 1.],
            ..Default::default()
        },
    );
    Ok(object)
}

fn value_controls(
    world: &mut World,
    router: &mut UiRouter,
    root: ObjectHandle,
    font: FontId,
) -> Result<(), WorldError> {
    let slider = UiSlider::new(0., 10., Some(1.), 5., UiAxis::Horizontal)?;
    for (label, y, widget) in [
        ("Activate", 140., UiControl::button()),
        ("Sound enabled", 80., UiControl::toggle(true)),
        ("Volume", 20., UiControl::slider(slider)),
    ] {
        control(world, router, root, font, (label, y), widget)?;
    }
    Ok(())
}
