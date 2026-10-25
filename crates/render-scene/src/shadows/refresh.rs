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
}

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

    pub fn prepare(
        &mut self,
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
        for (index, matrix) in cascades.matrices.iter_mut().enumerate() {
            let input = CascadeRefreshInput {
                frame: self.frame,
                matrix: *matrix,
                toward_light: sun,
                resolution: settings.resolution,
                generation,
                capture: self.capture,
            };
            if let Some(reason) = settings
                .refresh_policy
                .reason(index, &self.states[index], input)
            {
                refresh[index] = true;
                self.states[index].record(input, reason);
                self.bias[index] = cascades.depth_bias[index];
            } else {
                *matrix = self.states[index].matrix().expect("rendered cascade");
                cascades.depth_bias[index] = self.bias[index];
            }
        }
        self.frame = self.frame.wrapping_add(1);
        self.capture = false;
        refresh
    }
}

impl super::ShadowedScene {
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
