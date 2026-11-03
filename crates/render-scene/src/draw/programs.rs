//! Player program draws admitted for the next frame.

use worldrunner_render_program::runtime::AdmittedPrograms;

use super::SceneGpu;

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
        self.programs = Some(programs);
    }

    /// Whether admitted draws are waiting for a frame.
    pub(crate) fn has_programs(&self) -> bool {
        self.programs.is_some()
    }

    /// The admitted draws no frame has taken yet.
    pub(crate) fn take_programs(&mut self) -> Option<AdmittedPrograms> {
        self.programs.take()
    }
}
