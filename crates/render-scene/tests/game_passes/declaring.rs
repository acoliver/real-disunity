//! A game that declares which stock surfaces it draws and plans nothing, for tests of what the
//! declaration takes out of the stock frame.
use std::sync::Arc;
use worldrunner_render_post::{
    AttachmentView, BuildContext, DrawContext, FeatureDeclaration, FrameError, FrameFeature,
    PassDeclaration, PrepareContext, PreparedFeature,
};
use worldrunner_render_scene::{
    compositor::{GameBuild, GameFeatureFactory, GamePasses, StockReplacement},
    wgpu,
};

struct Declaring(StockReplacement);

struct Silent;

impl FrameFeature for Silent {
    fn declare(&self) -> FeatureDeclaration {
        FeatureDeclaration {
            passes: Vec::new(),
            effects: Vec::new(),
        }
    }

    fn plan(
        &self,
        _passes: &mut Vec<PassDeclaration>,
        _views: &mut Vec<AttachmentView>,
    ) -> Result<FeatureDeclaration, FrameError> {
        Ok(self.declare())
    }

    fn build(&self, _context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(Silent))
    }
}

impl PreparedFeature for Silent {
    fn prepare(&mut self, _context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        Ok(())
    }

    fn draw(&self, _context: &DrawContext, _pass: &mut wgpu::RenderPass<'_>) {}
}

impl GameFeatureFactory for Declaring {
    fn name(&self) -> &'static str {
        "declaring"
    }

    fn replaces_stock(&self) -> StockReplacement {
        self.0
    }

    fn build(&self, _context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
        Ok(Box::new(Silent))
    }
}

/// An inventory of one game that draws `replaces` and nothing else.
pub fn declaring(replaces: StockReplacement) -> GamePasses {
    GamePasses::new().with(Arc::new(Declaring(replaces)))
}
