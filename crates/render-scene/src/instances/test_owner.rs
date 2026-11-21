//! A recording transaction's write set for the CPU tests of instance history and staging.
use worldrunner_render_gpu::submission::{PreparedFrame, Publications, SubmissionAuthority};

/// Only a recording frame makes a write set, so each owner begins one on its own authority over
/// the shared test queue, which lets several stay open at once. The frame is finished but never
/// submitted: its write set stays `Preparing` and is not the thread's current preparation, which
/// is how the code under test sees an earlier frame's owner. Call it inside a scene test session.
pub(crate) struct Owner {
    publications: Publications,
    _frame: PreparedFrame,
}

impl Owner {
    pub(crate) fn new() -> Self {
        let context =
            crate::device::GpuContext::shared_for_test_with_features(wgpu::Features::empty())
                .expect("instance staging tests need the shared test device");
        let authority = SubmissionAuthority::new(context.queue().clone());
        let recording = authority
            .begin(context.device())
            .expect("a new authority has no frame in progress");
        let publications = Publications::current().expect("recording installs its write set");
        Self {
            publications,
            _frame: recording.finish(),
        }
    }
}

impl std::ops::Deref for Owner {
    type Target = Publications;
    fn deref(&self) -> &Publications {
        &self.publications
    }
}
