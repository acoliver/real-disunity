use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use super::visibility::Selection;

pub(crate) struct SelectionPublication {
    pub(super) destination: Rc<RefCell<Selection>>,
    pub(super) selection: Selection,
}

pub(crate) struct ColorDraw {
    pub selection: SelectionPublication,
    pub terrain: super::DrawCount,
    /// Whether the terrain's pipeline was bound, so that terrain received what the pass binds.
    pub terrain_bound: bool,
    pub sky: super::DrawCount,
    pub shadows: super::ReceiverBindings,
    pub ssao: super::ReceiverBindings,
}

/// Draw results remain local until the scheduler checks its encoder after rendering.
#[derive(Default)]
pub(crate) struct DrawPublications {
    selections: RefCell<Vec<SelectionPublication>>,
    effects: Cell<super::EffectStatistics>,
    geometry: Cell<Option<super::TerrainGeometryStatistics>>,
    /// The G-buffer slot's draw when the frame renders from the G-buffer, published with the
    /// sky count once Opaque has drawn the rest.
    gbuffer: RefCell<Option<ColorDraw>>,
}

impl DrawPublications {
    /// Holds the G-buffer slot's draw of a frame that renders from the G-buffer. Its surfaces
    /// receive the lighting pass's shadows (`shadows`) and screen AO (`ao`), as the colour
    /// pass's surfaces receive its own.
    pub fn record_gbuffer(&self, mut draw: ColorDraw, shadows: bool, ao: bool) {
        draw.shadows = super::ReceiverBindings::new(shadows, draw.terrain_bound && shadows);
        draw.ssao = super::ReceiverBindings::new(ao, draw.terrain_bound && ao);
        *self.gbuffer.borrow_mut() = Some(draw);
    }

    /// Publishes the held G-buffer draw as the frame's colour draw, with the sky count of
    /// Opaque's remaining forward draws.
    pub fn record_remainder(&self, scene: &super::SceneGpu, sky: super::DrawCount) {
        let mut draw = self
            .gbuffer
            .borrow_mut()
            .take()
            .expect("the G-buffer slot draws before Opaque");
        draw.sky = sky;
        self.record_color(scene, draw);
    }

    pub fn record_color(&self, scene: &super::SceneGpu, draw: ColorDraw) {
        self.geometry
            .set(scene.drawing_geometry_statistics(draw.terrain));
        scene.pass_counters.terrain.record(draw.terrain);
        scene.pass_counters.sky.record(draw.sky);
        let mut effects = self.effects.get();
        effects.shadows.receivers = draw.shadows;
        effects.ssao.receivers = draw.ssao;
        self.effects.set(effects);
        self.record(draw.selection);
    }

    pub fn record(&self, selection: SelectionPublication) {
        self.selections.borrow_mut().push(selection);
    }

    pub fn install(
        &self,
        context: &worldrunner_render_post::PrepareContext<'_>,
        shadows: super::PassInstallation,
        screen: bool,
        ao: Option<worldrunner_render_plan::SceneSsao>,
    ) {
        let mut effects = super::EffectStatistics::default();
        effects.shadows.installation = shadows;
        if context.completed_in_prefix(worldrunner_render_post::PassSlot::DepthNormalPrepass) {
            effects.ssao.prepass = super::PassExecution::Ran;
        }
        if screen {
            effects.ssao.installation = super::PassInstallation::Installed;
            effects.ssao.enabled =
                super::EffectEnabled::from_enabled(ao.is_some_and(|ao| ao.intensity_micro != 0));
        }
        self.effects.set(effects);
    }

    pub fn prepass(&self) {
        let mut effects = self.effects.get();
        effects.ssao.prepass = super::PassExecution::Ran;
        self.effects.set(effects);
    }

    pub fn publish(
        &mut self,
        context: &mut worldrunner_render_post::PrepareContext<'_>,
        scene: &super::SceneGpu,
    ) {
        let destination = scene.pass_counters.geometry.clone();
        let geometry = self.geometry.take();
        crate::compositor::publication::stage(context, &destination.clone(), move || {
            destination.set(geometry);
        });
        let destination = scene.effect_statistics.clone();
        let effects = self.effects.get();
        crate::compositor::publication::stage(context, &destination.clone(), move || {
            destination.set(effects);
        });
        for SelectionPublication {
            destination,
            selection,
        } in self.selections.get_mut().drain(..)
        {
            crate::compositor::publication::stage(context, &destination.clone(), move || {
                *destination.borrow_mut() = selection;
            });
        }
    }
}
