//! A frame whose scene is the game's own screen.
//!
//! A player draws the game into a render texture its host calls the screen. This entry samples
//! that texture in the frame's opaque pass in place of the stock scene, so the screen reaches the
//! window without a CPU readback, and the UI overlay still draws over it after the output stage.
//! The stock scene's draws are not encoded in a frame the screen covers.
//!
//! The sampling pass is a borrowed feature built once by [`GameScreenPass`], not one registered
//! on the renderer, so frames that show the stock scene never see it.

use super::offscreen::take_offscreen;
use super::{FrameRenderer, GameScreenComposite};
use crate::draw::SceneGpu;
use worldrunner_render_post::{
    AttachmentView, BorrowedFeature, BuildContext, FeatureDeclaration, FrameError, FrameFeature,
    FramePlan, OperationsGroup, PassDeclaration, PassEntry, PassSlot, PreparedFeature, ResourceRef,
};

const ENTRY: PassEntry = PassEntry::new(PassSlot::Opaque);

/// The pass that samples a game screen, built for one target format and reused by every frame
/// that shows a screen in that format.
pub struct GameScreenPass {
    resource: ResourceRef,
    format: wgpu::TextureFormat,
    declaration: FeatureDeclaration,
    prepared: Box<dyn PreparedFeature>,
}

impl GameScreenPass {
    /// The pass for the renderer's scene colour format, naming the screen `resource` in frame
    /// plans. The operations that write the screen export that same name.
    ///
    /// # Errors
    /// Fails when the device cannot build the composite for that format.
    pub(super) fn new(
        renderer: &FrameRenderer,
        device: &wgpu::Device,
        resource: ResourceRef,
    ) -> Result<Self, FrameError> {
        let targets = renderer.targets();
        let format = targets.scene().texture().format();
        let feature = GameScreenComposite::new(resource, ENTRY, format);
        let prepared = feature.build(&BuildContext { device, targets })?;
        Ok(Self {
            resource,
            format,
            declaration: feature.declare(),
            prepared,
        })
    }

    /// The scene colour format the pass draws into. A renderer whose scene colour changes needs a
    /// new pass.
    #[must_use]
    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }
}

/// What one frame shows of the game's screen.
pub struct GameScreenFrame<'a> {
    pub pass: &'a mut GameScreenPass,
    /// The screen: a single-sample `Rgba8UnormSrgb` texture the pass can sample, rows bottom first.
    pub screen: &'a wgpu::Texture,
    /// Whether work submitted before this frame already wrote the screen. Without it the frame's
    /// admitted offscreen operations must export the screen as the pass's resource name; a frame
    /// with neither is refused by the plan's validation, as a read with no producer.
    pub retained: bool,
    /// The scene colour the frame draws into.
    pub color: &'a AttachmentView,
}

/// Encodes a frame that shows the screen in place of the stock scene.
///
/// The scene's offscreen operations are taken as the stock path takes them and run before the
/// opaque pass, in the same encoder. Admitted player program draws are refused by name: they draw
/// into the stock scene, which this frame does not encode. The scene's UI overlay is prepared as
/// for a stock frame, so the output stage draws it over the screen.
///
/// # Errors
/// The errors of the frozen plan, and by name an arena that retired since admission.
pub(super) fn encode_game_screen(
    renderer: &mut FrameRenderer,
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    scene: &mut SceneGpu,
    frame: GameScreenFrame<'_>,
) -> Result<(), FrameError> {
    let programs = scene.take_programs();
    let offscreen = take_offscreen(scene, encoder)?;
    if programs.is_some() {
        return Err(FrameError::UnsupportedEffect(
            "player program draws with the game screen",
        ));
    }
    renderer.prepare_scene_inputs(device, scene);
    let GameScreenFrame {
        pass,
        screen,
        retained,
        color,
    } = frame;
    let group = offscreen.as_ref().map(|work| OperationsGroup {
        before: Some(ENTRY),
        work,
    });
    let mut screen_view = AttachmentView::new(pass.resource, screen)?;
    // Exported views must be marked retained. The plan only makes their contents
    // readable after the group that produces them, including on the first write.
    if retained
        || group.as_ref().is_some_and(|g| {
            g.work
                .produces()
                .iter()
                .any(|out| out.resource == pass.resource)
        })
    {
        screen_view = screen_view.with_retained_contents();
    }
    let mut opaque = PassDeclaration::new(PassSlot::Opaque);
    opaque.color = Some(renderer.color_attachment(color.resource(), color));
    let views = [color.clone(), screen_view];
    renderer.encode_frozen(
        device,
        encoder,
        &FramePlan {
            passes: std::slice::from_ref(&opaque),
            views: &views,
            requested: &[],
            operations: group.as_slice(),
        },
        &mut [BorrowedFeature {
            declaration: pass.declaration.clone(),
            prepared: &mut *pass.prepared,
        }],
    )
}
