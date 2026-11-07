//! Player program draws admitted for the next frame.

use worldrunner_render_program::offscreen::AdmittedOffscreen;
use worldrunner_render_program::runtime::AdmittedPrograms;

use super::SceneGpu;

/// Work for the player's programs that waits for the next frame: draws that join the opaque pass,
/// and offscreen operations that the frame encodes before its own.
#[derive(Debug, Default)]
pub(crate) struct PlayerWork {
    pub(crate) programs: Option<AdmittedPrograms>,
    pub(crate) offscreen: Option<AdmittedOffscreen>,
}

impl SceneGpu {
    /// Carries `programs` into the next frame this scene is encoded into, where they draw after
    /// the scene's own opaque geometry in the same pass.
    ///
    /// Only [`crate::compositor::FrameEncoding::encode_scene`] draws them. The resident path
    /// ([`crate::compositor::encode_resident`]) and the shadowed path
    /// ([`crate::shadows::ShadowedScene::encode`]) refuse a scene that holds admitted draws by
    /// name instead of ignoring them.
    ///
    /// Only draws admitted through [`worldrunner_render_program::runtime::ProgramRuntime::admit_draws`]
    /// can be given, so the arena, the frame's authority and the target signature were checked
    /// before they got here. A later call replaces draws that no frame took.
    pub fn admit_programs(&mut self, programs: AdmittedPrograms) {
        self.player.programs = Some(programs);
    }

    /// Whether admitted draws are waiting for a frame.
    pub(crate) fn has_programs(&self) -> bool {
        self.player.programs.is_some()
    }

    /// The admitted draws no frame has taken yet.
    pub(crate) fn take_programs(&mut self) -> Option<AdmittedPrograms> {
        self.player.programs.take()
    }
}

impl SceneGpu {
    /// Carries admitted offscreen operations into the next frame this scene is encoded into.
    /// That frame encodes them with its own timer before any of its stock operations, in the same
    /// submission, so a draw admitted with [`Self::admit_programs`] can sample what they write.
    ///
    /// Only [`crate::compositor::FrameEncoding::encode_scene`] encodes them. The resident path and
    /// the shadowed path refuse a scene that holds them by name. A later call replaces operations
    /// that no frame took.
    pub fn admit_offscreen(&mut self, offscreen: AdmittedOffscreen) {
        self.player.offscreen = Some(offscreen);
    }

    /// What a path that cannot draw player work refuses by name: admitted draws or admitted
    /// offscreen operations, if any wait for a frame.
    pub(crate) fn waiting_player_work(&self) -> Option<&'static str> {
        if self.has_programs() {
            Some("player programs")
        } else if self.has_offscreen() {
            Some("offscreen operations")
        } else {
            None
        }
    }

    /// Whether admitted offscreen operations are waiting for a frame.
    pub(crate) fn has_offscreen(&self) -> bool {
        self.player.offscreen.is_some()
    }

    /// The admitted offscreen operations no frame has taken yet.
    pub(crate) fn take_offscreen(&mut self) -> Option<AdmittedOffscreen> {
        self.player.offscreen.take()
    }
}
