//! Stock resident identities are stable for the lifetime of their shared state.
use std::rc::Rc;
use worldrunner_render_gpu::submission::{Publications, capture::WriteKey};

pub(crate) fn key<T>(state: &Rc<T>) -> WriteKey {
    WriteKey::Resident(u64::try_from(Rc::as_ptr(state) as usize).expect("resident identity"))
}

pub(crate) fn stage<T>(
    context: &worldrunner_render_post::PrepareContext<'_>,
    state: &Rc<T>,
    publish: impl FnOnce() + 'static,
) {
    // Capture records any contract error and rejects preparation before submission.
    let _ = context.publications().stage_write(key(state), publish);
}

pub(crate) fn encode<T>(
    encoder: &wgpu::CommandEncoder,
    state: &Rc<T>,
    publish: impl FnOnce() + 'static,
) -> Result<(), worldrunner_render_gpu::submission::TransactionError> {
    let _ = Publications::for_encoder(encoder)?.stage_write(key(state), publish);
    Ok(())
}
