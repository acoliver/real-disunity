//! Samples a caller-owned game screen in an authored colour pass.
use worldrunner_render_post::{
    AttachmentView, BuildContext, CopyOutput, CopySource, DrawContext, FeatureDeclaration,
    FeaturePass, FrameError, FrameFeature, PassEntry, PrepareContext, PreparedFeature, ResourceRef,
};

const SCREEN_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// Samples one named, bottom-first, single-sample sRGB screen in an authored colour pass.
///
/// Register this feature on `FrameRenderer`, and include `screen` as an
/// `AttachmentView` in each frame plan. The view may be retained between frames.
/// Supply this feature in place of the stock scene draw at `entry`. The target pass
/// must have one colour attachment of `target_format`, one sample and no depth
/// attachment. Place later UI entries after `entry`.
pub struct GameScreenComposite {
    screen: ResourceRef,
    entry: PassEntry,
    target_format: wgpu::TextureFormat,
}

impl GameScreenComposite {
    pub fn new(screen: ResourceRef, entry: PassEntry, target_format: wgpu::TextureFormat) -> Self {
        Self {
            screen,
            entry,
            target_format,
        }
    }
}

impl FrameFeature for GameScreenComposite {
    fn declare(&self) -> FeatureDeclaration {
        let mut pass = FeaturePass::nth(self.entry.slot(), self.entry.occurrence());
        pass.reads.push(self.screen);
        pass.color_format = Some(self.target_format);
        pass.requires_no_depth = true;
        FeatureDeclaration {
            passes: vec![pass],
            effects: Vec::new(),
        }
    }

    fn build(&self, context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        let copy = CopyOutput::bottom_first(context.device, self.target_format);
        copy.validate_config(
            context.device,
            worldrunner_render_post::FrameConfig::new([1, 1], SCREEN_FORMAT, 1),
        )
        .map_err(|error| copy_error(&error))?;
        Ok(Box::new(PreparedScreen {
            screen: self.screen,
            copy,
            texture: None,
            source: None,
        }))
    }
}

struct PreparedScreen {
    screen: ResourceRef,
    copy: CopyOutput,
    texture: Option<wgpu::Texture>,
    source: Option<CopySource>,
}

impl PreparedFeature for PreparedScreen {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        let source = context.resource(self.screen)?;
        validate_source(source)?;
        let texture = source.texture();
        if self.texture.as_ref() != Some(texture) {
            self.source = Some(
                self.copy
                    .source(context.device(), source)
                    .map_err(|error| copy_error(&error))?,
            );
            self.texture = Some(texture.clone());
        }
        Ok(())
    }

    fn draw(&self, _: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        self.source
            .as_ref()
            .expect("screen prepared before drawing")
            .record(pass);
    }
}

fn copy_error(error: &worldrunner_render_post::PostError) -> FrameError {
    FrameError::FeaturePreparation {
        feature: "game screen composite",
        reason: error.to_string(),
    }
}

fn validate_source(source: &AttachmentView) -> Result<(), FrameError> {
    if source.format() != SCREEN_FORMAT
        || source.texture().sample_count() != 1
        || !source
            .texture()
            .usage()
            .contains(wgpu::TextureUsages::TEXTURE_BINDING)
    {
        return Err(FrameError::FeaturePreparation {
            feature: "game screen composite",
            reason: format!(
                "screen {:?} must be a single-sample Rgba8UnormSrgb sampled attachment",
                source.resource()
            ),
        });
    }
    Ok(())
}
