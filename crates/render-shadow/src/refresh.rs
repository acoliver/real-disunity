//! Pure periodic scheduling and invalidation for up to four retained cascades.
use glam::{Mat4, Vec3};

use crate::InvalidFit;

/// Periodic slots bound ordinary far-cascade refreshes. Forced refreshes bypass
/// the budget. Each phase must be below its interval; incompatible slots fail validation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CascadeRefreshPolicy {
    pub intervals: [u16; 4],
    pub phases: [u16; 4],
    pub max_non_near: u8,
    pub light_delta_radians: f32,
    pub projection_delta_texels: f32,
}
impl Default for CascadeRefreshPolicy {
    fn default() -> Self {
        Self {
            intervals: [1; 4],
            phases: [0; 4],
            max_non_near: 3,
            light_delta_radians: 0.0,
            projection_delta_texels: 0.0,
        }
    }
}

/// Evidence for the candidate projection. Generation covers caller-owned settings
/// and caster membership. Frame numbers advance only when work is submitted.
#[derive(Clone, Copy, Debug)]
pub struct CascadeRefreshInput {
    pub frame: u64,
    pub matrix: Mat4,
    pub toward_light: Vec3,
    pub resolution: u32,
    pub generation: u64,
    pub capture: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefreshReason {
    Uninitialized,
    Interval,
    Light,
    Projection,
    DepthRange,
    Generation,
    Resolution,
    Invalidated,
    Capture,
}

/// Last submitted map contents. Deciding to render never changes this evidence.
#[derive(Clone, Copy, Debug, Default)]
pub struct CascadeRefreshState {
    rendered: Option<CascadeRefreshInput>,
    reason: Option<RefreshReason>,
    invalidated: bool,
}
impl CascadeRefreshState {
    pub fn matrix(&self) -> Option<Mat4> {
        self.rendered.map(|input| input.matrix)
    }
    pub fn last_frame(&self) -> Option<u64> {
        self.rendered.map(|input| input.frame)
    }
    pub fn last_reason(&self) -> Option<RefreshReason> {
        self.reason
    }
    pub fn invalidate(&mut self) {
        self.invalidated = true;
    }
    pub fn record(&mut self, input: CascadeRefreshInput, reason: RefreshReason) {
        self.rendered = Some(input);
        self.reason = Some(reason);
        self.invalidated = false;
    }
}
impl CascadeRefreshPolicy {
    pub fn validate(self) -> Result<(), InvalidFit> {
        if self.max_non_near == 0
            || self.max_non_near > 3
            || !self.light_delta_radians.is_finite()
            || !(0.0..=std::f32::consts::PI).contains(&self.light_delta_radians)
            || !self.projection_delta_texels.is_finite()
            || self.projection_delta_texels < 0.0
            || (0..4).any(|i| self.intervals[i] == 0 || self.phases[i] >= self.intervals[i])
        {
            return Err(InvalidFit);
        }
        let overlaps = |a, b| slots_overlap(self, a, b);
        let pairs = [(1, 2), (1, 3), (2, 3)];
        let invalid = match self.max_non_near {
            1 => pairs.iter().any(|&(a, b)| overlaps(a, b)),
            2 => pairs.iter().all(|&(a, b)| overlaps(a, b)),
            _ => false,
        };
        if invalid { Err(InvalidFit) } else { Ok(()) }
    }

    /// Returns a refresh reason without mutating the retained map evidence.
    /// Call only with a validated policy, index below four and finite invertible fits.
    pub fn reason(
        self,
        index: usize,
        state: &CascadeRefreshState,
        input: CascadeRefreshInput,
    ) -> Option<RefreshReason> {
        use RefreshReason as R;
        if input.capture {
            return Some(R::Capture);
        }
        if state.invalidated {
            return Some(R::Invalidated);
        }
        let Some(old) = state.rendered else {
            return Some(R::Uninitialized);
        };
        if input.generation != old.generation {
            return Some(R::Generation);
        }
        if input.resolution != old.resolution {
            return Some(R::Resolution);
        }
        let dot = input
            .toward_light
            .normalize()
            .dot(old.toward_light.normalize());
        if dot.clamp(-1.0, 1.0) < self.light_delta_radians.cos() {
            return Some(R::Light);
        }
        if input.matrix.transpose().z_axis != old.matrix.transpose().z_axis {
            return Some(R::DepthRange);
        }
        if projection_delta(old.matrix, input) > self.projection_delta_texels {
            return Some(R::Projection);
        }
        (input.frame % u64::from(self.intervals[index]) == u64::from(self.phases[index]))
            .then_some(R::Interval)
    }
}

fn slots_overlap(policy: CascadeRefreshPolicy, a: usize, b: usize) -> bool {
    let mut x = policy.intervals[a];
    let mut y = policy.intervals[b];
    while y != 0 {
        (x, y) = (y, x % y);
    }
    policy.phases[a] % x == policy.phases[b] % x
}

fn projection_delta(old: Mat4, input: CascadeRefreshInput) -> f32 {
    let transform = input.matrix * old.inverse();
    let mut delta = 0.0_f32;
    for x in [-1.0, 1.0] {
        for y in [-1.0, 1.0] {
            for z in [0.0, 1.0] {
                let point = Vec3::new(x, y, z);
                let moved = transform.project_point3(point) - point;
                delta = delta.max(moved.x.abs()).max(moved.y.abs());
            }
        }
    }
    delta * f32::from(u16::try_from(input.resolution).expect("bounded resolution")) * 0.5
}

#[cfg(test)]
#[path = "refresh_tests.rs"]
mod tests;
