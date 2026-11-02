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
    pub sky: super::DrawCount,
    pub shadows: super::ReceiverBindings,
    pub ssao: super::ReceiverBindings,
}

/// Draw results remain local until the scheduler checks its encoder after rendering.
#[derive(Default)]
pub(crate) struct DrawPublications {
    selections: RefCell<Vec<SelectionPublication>>,
    effects: Cell<super::EffectStatistics>,
}

impl DrawPublications {
    pub fn record_color(&self, scene: &super::SceneGpu, draw: ColorDraw) {
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
