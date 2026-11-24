//! Shows the game's screen in place of the stock scene in the frames that have contents for it.
//!
//! The retained host offers the screen before each frame. The frame takes the player's offscreen
//! operations as it always does, and when they export the screen, or an earlier submitted frame
//! wrote it, encodes the screen through the compositor instead of the stock scene, in the same
//! encoder and submission. A frame with neither shows the stock scene.
//!
//! Every submission that encodes the scene does this bookkeeping: ordinary frames and each
//! accumulated capture sample. A frame that shows the screen does not encode the stock scene, so
//! it cannot fill the attachments that scene writes, and a capture of those is refused before
//! anything is submitted (`Renderer::reject_stale_attachments`).

use super::Resident;
use crate::game_screen::{GAME_SCREEN, GameScreen, Ledger};
use crate::{RetainedHost, ViewerError, presentation::Targets};
use worldrunner_render_program::offscreen::PendingOffscreen;
use worldrunner_render_scene::{
    SceneView,
    compositor::{FrameEncoding, GameScreenFrame, GameScreenPass},
    device::GpuContext,
    draw::SceneGpu,
};

/// The offered screen, what has been written to it, and the pass that samples it.
#[derive(Default)]
pub(super) struct Frames {
    ledger: Ledger<wgpu::Texture>,
    pass: Option<GameScreenPass>,
}

/// Terrain dependencies of a view that are not resident yet. A frame that shows the screen does
/// not encode the stock scene, which is what normally advances them, so the screen frame stages
/// them itself. The value belongs to the one frame whose view was checked and is never stored.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ViewMisses {
    pub(super) pages: bool,
    pub(super) geometry: bool,
}

impl ViewMisses {
    /// Stages the missing terrain uploads for `view` into `encoder` through `scene`.
    fn stage(
        self,
        scene: &mut SceneGpu,
        (device, encoder): (&wgpu::Device, &mut wgpu::CommandEncoder),
        view: &SceneView,
        viewport: [u32; 2],
    ) -> Result<(), ViewerError> {
        if !(self.pages || self.geometry) {
            return Ok(());
        }
        let frame = worldrunner_render_scene::frame::FrameUniform::new(view, None)?;
        if self.geometry {
            scene.prepare_terrain_geometry(device, encoder, &frame, viewport)?;
        }
        if self.pages {
            scene.prepare_terrain_pages(device, encoder, &frame, viewport)?;
        }
        Ok(())
    }
}

/// Offscreen work and terrain dependencies for this encoder.
pub(super) struct ScreenWork {
    pub(super) operations: Option<PendingOffscreen>,
    pub(super) misses: ViewMisses,
}

impl Frames {
    /// The screen the host offers for the next frame, or `None` when it offers none.
    pub(super) fn offer(&mut self, screen: Option<&GameScreen>) {
        if self
            .ledger
            .offer(screen.map(|s| (s.texture(), s.submitted())))
        {
            // The prepared pass owns a sampled view even while stock frames are drawn.
            self.pass = None;
        }
    }

    /// Whether `operations` write the offered texture itself under [`GAME_SCREEN`]. An export of
    /// another texture under that name is not a write of the offered screen.
    fn writes_offered(&self, operations: Option<&PendingOffscreen>) -> bool {
        self.ledger
            .offered()
            .zip(operations)
            .is_some_and(|(screen, work)| work.exports_texture(GAME_SCREEN, screen))
    }

    /// The operations as the frame hosts them, and whether they write the offered screen. An
    /// export under [`GAME_SCREEN`] that does not name the offered texture is dropped: the
    /// frame's plan hosts no view for it, so it would fail validation, or count another texture
    /// as written.
    fn screen_writer(
        &self,
        operations: Option<PendingOffscreen>,
    ) -> (Option<PendingOffscreen>, bool) {
        let writes = self.writes_offered(operations.as_ref());
        let operations = operations.map(|mut operations| {
            if !writes {
                operations.withdraw_export(GAME_SCREEN);
            }
            operations
        });
        (operations, writes)
    }

    pub(super) fn covers_stock_scene(&self) -> bool {
        self.ledger.has_submitted_screen()
    }

    /// The frame just encoded was submitted, so the write its operations carried happened.
    pub(super) fn submitted(&mut self) {
        self.ledger.submitted();
    }

    /// Encodes the scene of one frame: the game's screen when there is one to show, otherwise the
    /// stock scene. `work` carries the frame's offscreen operations and
    /// which terrain dependencies of its view the screen frame must stage.
    pub(super) fn render(
        &mut self,
        resident: &mut Resident,
        scene: (&GpuContext, &SceneView, &Targets),
        encoder: &mut wgpu::CommandEncoder,
        mut scheduler: FrameEncoding<'_>,
        work: ScreenWork,
    ) -> Result<(), ViewerError> {
        let (context, view, targets) = scene;
        let (operations, writes) = self.screen_writer(work.operations);
        let composite = if let Some(show) = self.ledger.plan(writes) {
            let color = targets.scene_attachments()?.0.clone();
            if self
                .pass
                .as_ref()
                .is_none_or(|pass| pass.format() != color.format())
            {
                self.pass = Some(scheduler.game_screen_pass(context.device(), GAME_SCREEN)?);
            }
            work.misses.stage(
                resident.scene_mut(),
                (context.device(), encoder),
                view,
                color.size(),
            )?;
            Some((show, color))
        } else {
            None
        };
        // Keep the packet local until the selected frame has finished its preparation.
        if let Some(operations) = operations {
            let admitted = operations
                .admit(encoder)
                .map_err(|error| ViewerError::Offscreen(error.to_string()))?;
            resident.scene_mut().admit_offscreen(admitted);
        }
        let Some((show, color)) = composite else {
            return resident.render(context, view, targets, encoder, scheduler);
        };
        let pass = self.pass.as_mut().expect("the pass was built above");
        scheduler.encode_game_screen(
            context.device(),
            encoder,
            resident.scene_mut(),
            GameScreenFrame {
                pass,
                screen: show.screen,
                retained: show.retained,
                color: &color,
            },
        )?;
        Ok(())
    }
}

impl super::Renderer {
    /// Refuses outputs read from frame attachments that only the stock scene writes while the
    /// frame can show the screen in place of that scene. The screen frame does not encode the
    /// stock scene, so those attachments would hold the contents of an earlier frame. This runs
    /// before the frame takes the host's queued operations or any capture request, so a refusal
    /// leaves both with the host.
    pub(super) fn reject_stale_attachments(
        &self,
        host: &RetainedHost,
        id: u64,
    ) -> Result<(), ViewerError> {
        let outputs = host
            .captures
            .recipes
            .get(&id)
            .map_or(&[][..], crate::RetainedCaptureRecipe::outputs);
        if super::capture::Products::requires_stock_scene(outputs)
            && self
                .game_screen
                .ledger
                .will_show(self.game_screen.writes_offered(host.offscreen.as_ref()))
        {
            return Err(ViewerError::UnsupportedScene(
                "capture of frame attachments while the game screen replaces the stock scene",
            ));
        }
        Ok(())
    }

    /// A submitted screen replaces all stock scene draws, including a fitted shadow pass.
    pub(super) fn fit_stock_scene(
        &mut self,
        view: &SceneView,
        retained: Option<&super::capture::Products>,
    ) -> Result<bool, ViewerError> {
        if self.game_screen.covers_stock_scene() {
            return Ok(false);
        }
        let capturing = retained.is_some();
        if capturing && let Resident::Shadowed(scene) = &mut self.resident {
            scene.request_shadow_capture();
        }
        self.render_depth_fitted(view, capturing, retained)
    }
}
