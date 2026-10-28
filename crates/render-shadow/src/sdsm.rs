//! Depth statistics and stable receiver-range policy, in scene metres.
use crate::{InvalidFit, practical_splits};

/// Empty coverage is successful evidence that there are no receivers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DepthRange {
    EmptyDepth,
    Receivers { near: f32, far: f32 },
}
impl DepthRange {
    /// Decode positive-float atomic min/max, sample count and invalid-input flag.
    pub fn decode(words: &[u32]) -> Result<Self, InvalidFit> {
        let [near, far, count, invalid] = *words else {
            return Err(InvalidFit);
        };
        if invalid != 0 {
            return Err(InvalidFit);
        }
        if count == 0 {
            return if near == f32::MAX.to_bits() && far == 0 {
                Ok(Self::EmptyDepth)
            } else {
                Err(InvalidFit)
            };
        }
        let result = Self::Receivers {
            near: f32::from_bits(near),
            far: f32::from_bits(far),
        };
        result.validate()?;
        Ok(result)
    }
    pub fn validate(self) -> Result<(), InvalidFit> {
        if let Self::Receivers { near, far } = self
            && (!near.is_finite() || !far.is_finite() || near <= 0.0 || far < near)
        {
            return Err(InvalidFit);
        }
        Ok(())
    }
}

/// Which bound expands when quantized receivers span less than the requested minimum.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MinimumSpanPolicy {
    #[default]
    ExtendNear,
    /// Preserve near and request a farther end before applying camera and active caps.
    ExtendFar,
}

/// Outward quantization followed by clipping. Empty depth disables receivers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RangeFit {
    pub near_step: f32,
    pub far_step: f32,
    pub minimum_span: f32,
    pub minimum_span_policy: MinimumSpanPolicy,
    pub active_maximum: Option<f32>,
    pub split_lambda: f32,
    pub split_policy: crate::SplitPolicy,
}
impl RangeFit {
    pub fn validate(self) -> Result<(), InvalidFit> {
        self.split_policy.validate()?;
        if [self.near_step, self.far_step, self.minimum_span]
            .iter()
            .any(|v| !v.is_finite() || *v <= 0.0)
            || !self.split_lambda.is_finite()
            || !(0.0..=1.0).contains(&self.split_lambda)
            || self
                .active_maximum
                .is_some_and(|value| !value.is_finite() || value <= 0.0)
        {
            return Err(InvalidFit);
        }
        Ok(())
    }
    pub fn fit(self, depth: DepthRange, clip: [f32; 2]) -> Result<DepthRange, InvalidFit> {
        self.validate()?;
        depth.validate()?;
        let [start, end] = clip;
        if !start.is_finite() || !end.is_finite() || start <= 0.0 || end <= start {
            return Err(InvalidFit);
        }
        let DepthRange::Receivers { near, far } = depth else {
            return Ok(DepthRange::EmptyDepth);
        };
        if near > end || far < start {
            return Ok(DepthRange::EmptyDepth);
        }
        let quantized_near = ((near / self.near_step).floor() * self.near_step).max(start);
        if self
            .active_maximum
            .is_some_and(|maximum| maximum <= quantized_near)
        {
            return Ok(DepthRange::EmptyDepth);
        }
        let end = self.active_maximum.map_or(end, |maximum| end.min(maximum));
        let (near, far) = self.expand_bounds(quantized_near, far, [start, end]);
        if !near.is_finite() || !far.is_finite() {
            return Err(InvalidFit);
        }
        if far <= near {
            return match self.minimum_span_policy {
                MinimumSpanPolicy::ExtendFar => Ok(DepthRange::EmptyDepth),
                MinimumSpanPolicy::ExtendNear => Err(InvalidFit),
            };
        }
        Ok(DepthRange::Receivers { near, far })
    }
    fn expand_bounds(self, near: f32, far: f32, [start, end]: [f32; 2]) -> (f32, f32) {
        let far = (far / self.far_step).ceil() * self.far_step;
        match self.minimum_span_policy {
            MinimumSpanPolicy::ExtendNear => {
                let far = far.max(start + self.minimum_span).min(end);
                (near.min(far - self.minimum_span.min(far - start)), far)
            }
            MinimumSpanPolicy::ExtendFar => (near, far.max(near + self.minimum_span).min(end)),
        }
    }
    /// Reapply caps to carried evidence without quantizing or requesting its span again.
    pub fn clip(self, depth: DepthRange, clip: [f32; 2]) -> Result<DepthRange, InvalidFit> {
        self.validate()?;
        depth.validate()?;
        let [start, end] = clip;
        if !start.is_finite() || !end.is_finite() || start <= 0.0 || end <= start {
            return Err(InvalidFit);
        }
        let DepthRange::Receivers { near, far } = depth else {
            return Ok(depth);
        };
        if far <= near {
            return Err(InvalidFit);
        }
        let near = near.max(start);
        let far = far.min(self.active_maximum.map_or(end, |maximum| end.min(maximum)));
        Ok(if far <= near {
            DepthRange::EmptyDepth
        } else {
            DepthRange::Receivers { near, far }
        })
    }
    pub fn splits(self, depth: DepthRange, count: u16) -> Result<Vec<f32>, InvalidFit> {
        self.validate()?;
        match depth {
            DepthRange::EmptyDepth => Ok(Vec::new()),
            DepthRange::Receivers { near, far } => match self.split_policy {
                crate::SplitPolicy::Practical => {
                    practical_splits(near, far, count, self.split_lambda)
                }
                policy @ crate::SplitPolicy::OverlapRecurrence { .. } => Ok(policy
                    .intervals(near, far, count)?
                    .into_iter()
                    .map(|v| v.end)
                    .collect()),
            },
        }
    }
}

/// Frame-indexed contraction hysteresis. Expansion never waits for the timer.
#[derive(Clone, Copy, Debug, Default)]
pub struct RangeFilter {
    current: Option<(DepthRange, u64)>,
}
impl RangeFilter {
    pub fn update(
        &mut self,
        candidate: DepthRange,
        frame: u64,
        contraction_frames: u64,
    ) -> Result<DepthRange, InvalidFit> {
        candidate.validate()?;
        let Some((DepthRange::Receivers { near, far }, adopted)) = self.current else {
            self.current = Some((candidate, frame));
            return Ok(candidate);
        };
        let result = match candidate {
            DepthRange::EmptyDepth => candidate,
            DepthRange::Receivers {
                near: next_near,
                far: next_far,
            } => {
                if frame < adopted {
                    return Err(InvalidFit);
                }
                if frame - adopted >= contraction_frames {
                    candidate
                } else {
                    DepthRange::Receivers {
                        near: near.min(next_near),
                        far: far.max(next_far),
                    }
                }
            }
        };
        self.current = Some((result, if result == candidate { frame } else { adopted }));
        Ok(result)
    }
}

#[cfg(test)]
#[path = "sdsm_tests.rs"]
mod tests;
