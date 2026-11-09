//! Fixed-function state a pipeline is built from, as plain data.
//!
//! The state describes more than the runtime draws today: stencil, alpha to coverage and several
//! colour targets are representable, and [`RenderState::check_drawable`] names the ones a draw
//! refuses. A depth bias is drawn: it is part of [`DepthState`] and of the pipeline. Stencil is
//! drawn by the prepared pipelines of a renderer's own pass ([`RenderState::check_targets`]),
//! whose depth attachment has a stencil aspect, and refused by the plain and material draws.

use crate::error::ProgramError;

/// What one colour target does with a fragment's colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorState {
    pub blend: Option<wgpu::BlendState>,
    pub write_mask: wgpu::ColorWrites,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RasterState {
    pub cull: Option<wgpu::Face>,
    /// The winding that counts as a front face, in framebuffer space.
    pub front_face: wgpu::FrontFace,
    pub alpha_to_coverage: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DepthState {
    pub test: wgpu::CompareFunction,
    pub write: bool,
    pub bias: wgpu::DepthBiasState,
}

/// The stencil test and operations of a pipeline, and the reference the pass is expected to set.
///
/// `ops` is the pipeline's. `reference` is not: it belongs to the pass, which the owner sets with
/// `set_stencil_reference` once for the pass, as the runtime binds no state the owner holds. It is
/// carried so that a state says what its test compares against.
#[derive(Clone, Debug, PartialEq)]
pub struct StencilState {
    pub ops: wgpu::StencilState,
    pub reference: u32,
}

/// The state of a draw. `colors` has one entry per colour target, `None` for a slot the draw
/// does not write.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderState {
    pub raster: RasterState,
    /// `None` tests nothing and writes no depth.
    pub depth: Option<DepthState>,
    pub colors: Vec<Option<ColorState>>,
    pub stencil: Option<StencilState>,
}

impl RenderState {
    /// Back faces culled, counter-clockwise fronts, `LessEqual` depth written, one target
    /// written in all channels without blending.
    #[must_use]
    pub fn opaque_depth() -> Self {
        Self {
            raster: RasterState {
                cull: Some(wgpu::Face::Back),
                front_face: wgpu::FrontFace::Ccw,
                alpha_to_coverage: false,
            },
            depth: Some(DepthState {
                test: wgpu::CompareFunction::LessEqual,
                write: true,
                bias: wgpu::DepthBiasState::default(),
            }),
            colors: vec![Some(ColorState {
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            stencil: None,
        }
    }

    /// No culling, no depth, one target written in all channels without blending: the state of
    /// a triangle that covers a target.
    #[must_use]
    pub(crate) fn fullscreen() -> Self {
        Self {
            raster: RasterState {
                cull: None,
                front_face: wgpu::FrontFace::Ccw,
                alpha_to_coverage: false,
            },
            depth: None,
            colors: vec![Some(ColorState {
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            stencil: None,
        }
    }

    /// Checks that a draw into one colour target can use this state and returns that target's
    /// state.
    ///
    /// # Errors
    /// Fails with `UnsupportedState` naming the first part of the state that a draw refuses.
    pub fn check_drawable(&self) -> Result<ColorState, ProgramError> {
        self.check_no_stencil()?;
        self.check_fixed_function()?;
        match self.colors.as_slice() {
            [Some(color)] => Ok(*color),
            [None] => Err(unwritten()),
            _ => Err(ProgramError::UnsupportedState {
                what: "several colour targets",
            }),
        }
    }

    /// Checks that a draw into `count` colour targets can use this state and returns each
    /// target's state, in target order. A target the fragment stage does not write is
    /// expressed by an empty write mask, which keeps the attachment compatible with the pass.
    /// A depth-only pass has `count` 0 and takes a state with no colour targets. A stencil state
    /// is accepted here; the pipeline checks it against the pass's depth attachment.
    ///
    /// # Errors
    /// Fails with `UnsupportedState` naming the first part of the state that a draw refuses,
    /// with `ColorStatesOnDepthOnly` when `count` is 0 and the state describes colour targets
    /// and with `ColorStates` when the state does not describe each of the `count` targets.
    pub fn check_targets(&self, count: usize) -> Result<Vec<ColorState>, ProgramError> {
        self.check_fixed_function()?;
        if count == 0 && !self.colors.is_empty() {
            return Err(ProgramError::ColorStatesOnDepthOnly {
                states: self.colors.len(),
            });
        }
        if self.colors.len() != count {
            return Err(ProgramError::ColorStates {
                states: self.colors.len(),
                targets: count,
            });
        }
        self.colors
            .iter()
            .map(|color| color.ok_or_else(unwritten))
            .collect()
    }

    fn check_fixed_function(&self) -> Result<(), ProgramError> {
        if self.raster.alpha_to_coverage {
            return Err(ProgramError::UnsupportedState {
                what: "alpha to coverage",
            });
        }
        Ok(())
    }

    fn check_no_stencil(&self) -> Result<(), ProgramError> {
        if self.stencil.is_some() {
            return Err(ProgramError::UnsupportedState { what: "stencil" });
        }
        Ok(())
    }
}

fn unwritten() -> ProgramError {
    ProgramError::UnsupportedState {
        what: "an unwritten colour target",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refused(state: &RenderState) -> &'static str {
        match state.check_drawable() {
            Err(ProgramError::UnsupportedState { what }) => what,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn the_opaque_state_is_what_its_name_says_and_is_drawable() {
        let state = RenderState::opaque_depth();
        assert_eq!(state.raster.cull, Some(wgpu::Face::Back));
        assert_eq!(state.raster.front_face, wgpu::FrontFace::Ccw);
        let depth = state.depth.unwrap();
        assert_eq!(depth.test, wgpu::CompareFunction::LessEqual);
        assert!(depth.write);
        assert_eq!(state.colors.len(), 1);
        let color = state.colors[0].unwrap();
        assert_eq!(color.blend, None);
        assert_eq!(color.write_mask, wgpu::ColorWrites::ALL);
        assert!(state.check_drawable().is_ok());
    }

    #[test]
    fn stencil_and_alpha_to_coverage_are_refused_by_name() {
        let mut state = RenderState::opaque_depth();
        state.stencil = Some(StencilState {
            ops: wgpu::StencilState::default(),
            reference: 1,
        });
        assert_eq!(refused(&state), "stencil");
        let mut state = RenderState::opaque_depth();
        state.raster.alpha_to_coverage = true;
        assert_eq!(refused(&state), "alpha to coverage");
    }

    #[test]
    fn a_stencil_state_describes_prepared_targets_and_is_refused_by_the_plain_draw() {
        let mut state = RenderState::opaque_depth();
        state.stencil = Some(StencilState {
            ops: wgpu::StencilState::default(),
            reference: 1,
        });
        assert_eq!(refused(&state), "stencil");
        assert!(state.check_targets(1).is_ok());
        state.raster.alpha_to_coverage = true;
        assert_eq!(refused_targets(&state, 1), "alpha to coverage");
    }

    #[test]
    fn a_depth_bias_is_part_of_a_drawable_state() {
        let mut state = RenderState::opaque_depth();
        state.depth.as_mut().unwrap().bias = wgpu::DepthBiasState {
            constant: 2,
            slope_scale: 1.5,
            clamp: 0.01,
        };
        assert!(state.check_drawable().is_ok());
        assert!(state.check_targets(1).is_ok());
    }

    #[test]
    fn a_depth_only_pass_takes_a_state_with_no_colour_targets() {
        let mut state = RenderState::opaque_depth();
        assert_eq!(
            state.check_targets(0).unwrap_err(),
            ProgramError::ColorStatesOnDepthOnly { states: 1 }
        );
        state.colors.clear();
        assert_eq!(state.check_targets(0).unwrap(), Vec::new());
        // The same state does not describe a pass that has a colour attachment.
        assert_eq!(
            state.check_targets(1).unwrap_err(),
            ProgramError::ColorStates {
                states: 0,
                targets: 1
            }
        );
    }

    #[test]
    fn a_draw_writes_exactly_one_colour_target() {
        let mut state = RenderState::opaque_depth();
        state.colors.push(state.colors[0]);
        assert_eq!(refused(&state), "several colour targets");
        state.colors = vec![None];
        assert_eq!(refused(&state), "an unwritten colour target");
        state.colors = Vec::new();
        assert_eq!(refused(&state), "several colour targets");
    }

    #[test]
    fn several_targets_are_described_one_state_each_with_the_same_fixed_function_rules() {
        let mut state = RenderState::opaque_depth();
        state.colors.push(Some(ColorState {
            blend: None,
            write_mask: wgpu::ColorWrites::empty(),
        }));
        let colors = state.check_targets(2).unwrap();
        assert_eq!(colors[0].write_mask, wgpu::ColorWrites::ALL);
        assert_eq!(colors[1].write_mask, wgpu::ColorWrites::empty());
        assert_eq!(
            state.check_targets(3).unwrap_err(),
            ProgramError::ColorStates {
                states: 2,
                targets: 3
            }
        );
        state.colors[1] = None;
        assert_eq!(
            state.check_targets(2).unwrap_err(),
            ProgramError::UnsupportedState {
                what: "an unwritten colour target"
            }
        );
        state.raster.alpha_to_coverage = true;
        assert_eq!(refused_targets(&state, 2), "alpha to coverage");
    }

    fn refused_targets(state: &RenderState, count: usize) -> &'static str {
        match state.check_targets(count) {
            Err(ProgramError::UnsupportedState { what }) => what,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }
}
