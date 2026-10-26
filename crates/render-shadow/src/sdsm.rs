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

/// Outward quantization followed by camera clipping. Empty depth disables receivers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RangeFit {
    pub near_step: f32,
    pub far_step: f32,
    pub minimum_span: f32,
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
        let far = ((far / self.far_step).ceil() * self.far_step)
            .max(start + self.minimum_span)
            .min(end);
        let near = ((near / self.near_step).floor() * self.near_step)
            .max(start)
            .min(far - self.minimum_span.min(far - start));
        if !near.is_finite() || !far.is_finite() || far <= near {
            return Err(InvalidFit);
        }
        Ok(DepthRange::Receivers { near, far })
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
