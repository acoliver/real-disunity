use super::{Renderer, live::Capture};
use crate::{RetainedHost, ViewerError};
use worldrunner_render_gpu::flight::CompletionTicket;

struct Pending {
    id: u64,
    frame: u64,
    time: std::time::Duration,
    ticket: CompletionTicket,
    image: Capture,
}

#[derive(Default)]
pub(super) struct Captures {
    pending: Vec<Pending>,
    spare: Vec<Capture>,
    frame: u64,
}
impl Captures {
    pub(super) fn next_frame(&self) -> u64 {
        self.frame
    }
    pub(super) fn is_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    pub(super) fn prepare(
        &mut self,
        device: &wgpu::Device,
        host: &RetainedHost,
        texture: &wgpu::Texture,
        pages_ready: bool,
    ) -> Result<Option<(u64, Capture)>, ViewerError> {
        if self.pending.len() == 3 {
            return Ok(None);
        }
        let generation = host.ready_generation();
        let Some(&(id, _, _)) = host
            .captures
            .waiting
            .iter()
            .find(|r| r.1 == generation && !host.readiness.page_wait && (!r.2 || pages_ready))
        else {
            return Ok(None);
        };
        let image = match self.spare.pop() {
            Some(image) if image.matches(texture) => image,
            _ => Capture::for_texture(device, texture)?,
        };
        Ok(Some((id, image)))
    }

    pub(super) fn submitted(
        &mut self,
        host: &mut RetainedHost,
        image: Option<(u64, Capture)>,
        ticket: CompletionTicket,
        time: std::time::Duration,
    ) -> Result<(), ViewerError> {
        if let Some((id, mut image)) = image {
            let index = host
                .captures
                .waiting
                .iter()
                .position(|r| r.0 == id)
                .expect("capture request");
            host.captures.waiting.remove(index);
            image.begin()?;
            self.pending.push(Pending {
                id,
                frame: self.frame,
                time,
                ticket,
                image,
            });
        }
        self.frame += 1;
        Ok(())
    }

    pub(super) fn collect(&mut self, host: &mut RetainedHost) -> Result<(), ViewerError> {
        let mut index = 0;
        while index < self.pending.len() {
            let pending = &mut self.pending[index];
            if pending.ticket.is_complete()
                && let Some(image) = pending.image.try_read()?
            {
                host.captures
                    .complete(pending.id, pending.frame, pending.time, image);
                let pending = self.pending.remove(index);
                // The map and GPU ticket have both finished before slot reuse.
                if self.spare.len() < 3 {
                    self.spare.push(pending.image);
                }
            } else {
                index += 1;
            }
        }
        Ok(())
    }
}

impl Renderer {
    pub(crate) fn drain(&self) -> Result<(), ViewerError> {
        self.context
            .device()
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(std::time::Duration::from_secs(30)),
            })
            .map_err(|error| ViewerError::Gpu(error.to_string()))?;
        self.check_errors()
    }
}

#[cfg(test)]
mod tests;
