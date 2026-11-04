//! Fixed-function state a pipeline is built from, as plain data.
//!
//! The state describes more than the runtime draws today: stencil, depth bias, alpha to
//! coverage and several colour targets are representable, and [`RenderState::check_drawable`]
//! names the ones a draw refuses.

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

    /// Checks that a draw into one colour target can use this state.
    ///
    /// # Errors
    /// Fails with `UnsupportedState` naming the first part of the state that a draw refuses.
    pub fn check_drawable(&self) -> Result<(), ProgramError> {
        let refuse = |what| Err(ProgramError::UnsupportedState { what });
        if self.stencil.is_some() {
            return refuse("stencil");
        }
        if self
            .depth
            .is_some_and(|depth| depth.bias != wgpu::DepthBiasState::default())
        {
            return refuse("depth bias");
        }
        if self.raster.alpha_to_coverage {
            return refuse("alpha to coverage");
        }
        match self.colors.as_slice() {
            [Some(_)] => Ok(()),
            [None] => refuse("an unwritten colour target"),
            _ => refuse("several colour targets"),
        }
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
    fn stencil_depth_bias_and_alpha_to_coverage_are_refused_by_name() {
        let mut state = RenderState::opaque_depth();
        state.stencil = Some(StencilState {
            ops: wgpu::StencilState::default(),
            reference: 1,
        });
        assert_eq!(refused(&state), "stencil");
        let mut state = RenderState::opaque_depth();
        state.depth.as_mut().unwrap().bias.constant = 2;
        assert_eq!(refused(&state), "depth bias");
        let mut state = RenderState::opaque_depth();
        state.raster.alpha_to_coverage = true;
        assert_eq!(refused(&state), "alpha to coverage");
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
}
