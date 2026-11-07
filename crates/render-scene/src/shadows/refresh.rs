use glam::Vec3;
use worldrunner_render_shadow::{CascadeRefreshInput, CascadeRefreshState};

use super::{DirectionalShadowSettings, cascades::CascadeViews};

#[derive(Clone, Debug, Default)]
pub(super) struct RefreshCache {
    pub states: [CascadeRefreshState; 4],
    bias: [f32; 4],
    frame: u64,
    key: Option<RefreshKey>,
    pub capture: bool,
    /// Latest localized caster change these states have been checked against.
    caster_seen: u64,
    /// Slice hull each retained map's casters were culled to; none when unculled.
    slices: [Option<crate::draw::SliceHull>; 4],
}

/// Texels a stored slice hull extends beyond the one required, so that small camera motion
/// and projection jitter keep reusing a cascade's selection and retained map.
const SLICE_SLACK_TEXELS: f32 = 32.0;

#[derive(Clone, Debug, PartialEq)]
struct RefreshKey {
    splits: [f32; 4],
    near: f32,
    blend: f32,
    camera_splits: bool,
    no_receivers: bool,
    size: [u32; 2],
    layers: usize,
}

impl RefreshCache {
    pub fn clone_for_frame(&self, map_replaced: bool) -> Self {
        let mut next = self.clone();
        if map_replaced {
            next.invalidate();
        }
        next
    }

    pub fn publish(
        self,
        encoder: &wgpu::CommandEncoder,
        destination: &std::rc::Rc<std::cell::RefCell<Self>>,
    ) -> Result<(), super::ShadowError> {
        let state = destination.clone();
        crate::compositor::publication::encode(encoder, destination, move || {
            *state.borrow_mut() = self;
        })
        .map_err(worldrunner_render_post::FrameError::from)?;
        Ok(())
    }

    pub fn invalidate(&mut self) {
        for state in &mut self.states {
            state.invalidate();
        }
    }

    /// Invalidates only the retained cascades whose maps a caster change can reach.
    fn invalidate_touched(&mut self, gpu: &crate::draw::SceneGpu) {
        for state in &mut self.states {
            if state
                .matrix()
                .is_some_and(|matrix| gpu.casters_changed_within(self.caster_seen, matrix))
            {
                state.invalidate();
            }
        }
        self.caster_seen = gpu.caster_sequence();
    }

    /// Invalidates a retained map whose casters were culled to a slice that no longer
    /// covers the receivers sampling it.
    fn invalidate_uncovered(
        &mut self,
        index: usize,
        cascades: &CascadeViews,
        settings: DirectionalShadowSettings,
    ) {
        let state = &mut self.states[index];
        let (Some(rendered), Some(matrix)) = (&self.slices[index], state.matrix()) else {
            return;
        };
        if !cascades
            .slice_hull(index, matrix, settings)
            .is_none_or(|required| rendered.contains(&required))
        {
            state.invalidate();
        }
    }

    pub fn prepare(
        &mut self,
        gpu: &crate::draw::SceneGpu,
        cascades: &mut CascadeViews,
        settings: DirectionalShadowSettings,
        sun: Vec3,
        size: [u32; 2],
        generation: u64,
    ) -> [bool; 4] {
        let key = RefreshKey {
            splits: cascades.splits,
            near: cascades.near,
            blend: cascades.blend,
            camera_splits: cascades.camera_splits,
            no_receivers: cascades.no_receivers,
            size,
            layers: cascades.matrices.len(),
        };
        if self.key.as_ref() != Some(&key) {
            self.invalidate();
        }
        self.key = Some(key);
        let mut refresh = [false; 4];
        let count = cascades.matrices.len();
        for (index, refreshed) in refresh.iter_mut().enumerate().take(count) {
            self.invalidate_uncovered(index, cascades, settings);
            let input = CascadeRefreshInput {
                frame: self.frame,
                matrix: cascades.matrices[index],
                toward_light: sun,
                resolution: settings.resolution,
                generation,
                capture: self.capture,
            };
            *refreshed = self.refresh_cascade(gpu, cascades, (index, input), settings);
        }
        self.frame = self.frame.wrapping_add(1);
        self.capture = false;
        refresh
    }

    /// Records a refresh of cascade `index`, with the slice its casters are culled to, or
    /// restores the retained map's matrix and bias. Returns whether it refreshes.
    fn refresh_cascade(
        &mut self,
        gpu: &crate::draw::SceneGpu,
        cascades: &mut CascadeViews,
        (index, input): (usize, CascadeRefreshInput),
        settings: DirectionalShadowSettings,
    ) -> bool {
        let Some(reason) = settings
            .refresh_policy
            .reason(index, &self.states[index], input)
        else {
            cascades.matrices[index] = self.states[index].matrix().expect("rendered cascade");
            cascades.depth_bias[index] = self.bias[index];
            return false;
        };
        self.states[index].record(input, reason);
        self.bias[index] = cascades.depth_bias[index];
        let required = cascades.slice_hull(index, input.matrix, settings);
        let slack = SLICE_SLACK_TEXELS * 2.0
            / f32::from(u16::try_from(settings.resolution).expect("bounded resolution"));
        self.slices[index] = gpu.cascade_slice(index, input.matrix, required.as_ref(), slack);
        true
    }
}

impl super::ShadowedScene {
    pub(super) fn refresh_for_frame(&mut self, replaced: bool) -> RefreshCache {
        // Cascades draw the resident streaming chunks. A change to them reaches the
        // refresh policy through the caster generation, not as a per-frame invalidation.
        // Moved placements invalidate only the cascades they touch. Changes stay pending
        // until a submitted frame has checked them.
        let committed = self.refresh.borrow().caster_seen;
        self.gpu.forget_caster_changes_through(committed);
        let mut next = self.refresh.borrow().clone_for_frame(replaced);
        next.invalidate_touched(&self.gpu);
        next
    }

    /// Invalidates every retained cascade on the next submitted frame.
    pub fn invalidate_shadows(&mut self) {
        self.refresh.borrow_mut().invalidate();
    }

    /// Requests fresh maps for the next submitted capture. Abandoned encoders
    /// leave the request pending. Call before encoding any capture readback.
    pub fn request_shadow_capture(&mut self) {
        self.refresh.borrow_mut().capture = true;
    }

    /// Evidence for the maps used by receivers in the last submitted frame.
    pub fn cascade_refresh_states(&self) -> [CascadeRefreshState; 4] {
        self.refresh.borrow().states
    }
}
