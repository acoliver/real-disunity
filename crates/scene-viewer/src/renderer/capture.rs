use super::{Renderer, game_screen::ViewMisses, readback::Capture};
mod accumulation;
pub(super) mod advanced;
mod products;
use crate::retained::CaptureCompletion;
use crate::{RetainedHost, ViewerError};
pub(super) use products::Products;
use worldrunner_render_gpu::flight::CompletionTicket;

struct Pending {
    id: u64,
    frame: u64,
    scene: worldrunner_render_scene::draw::SceneStatistics,
    temporal_history_reset: crate::TemporalHistoryReset,
    time: std::time::Duration,
    ticket: CompletionTicket,
    image: Products,
}

/// A retained capture the next frame takes, and the terrain its view still lacks.
#[derive(Default)]
pub(super) struct HostCapture {
    pub(super) request: Option<(u64, Products)>,
    pub(super) misses: ViewMisses,
}

impl Renderer {
    pub(super) fn prepare_host_capture(
        &mut self,
        host: Option<&mut RetainedHost>,
        view: &worldrunner_render_scene::SceneView,
        output: &wgpu::Texture,
    ) -> Result<HostCapture, ViewerError> {
        let Some(host) = host else {
            return Ok(HostCapture::default());
        };
        let readiness = self.retained_pages_ready(host, view)?;
        let request = self.streaming.captures.prepare(
            self.context.device(),
            host,
            output,
            &self.presentation.scheduler,
            readiness.pages_ready,
        )?;
        if let Some((id, _)) = &request {
            self.reject_stale_attachments(host, *id)?;
        }
        Ok(HostCapture {
            request,
            misses: readiness.misses,
        })
    }
}

#[derive(Default)]
pub(super) struct Captures {
    pending: Vec<Pending>,
    spare: Vec<Capture>,
    frame: u64,
    pub(super) active: Option<advanced::Active>,
}
impl Captures {
    pub(super) fn next_frame(&self) -> u64 {
        self.frame
    }
    pub(super) fn advance_frame(&mut self) {
        self.frame = self
            .frame
            .checked_add(1)
            .expect("render frame identity overflow");
    }
    pub(super) fn is_pending(&self) -> bool {
        !self.pending.is_empty() || self.active.is_some()
    }

    pub(super) fn prepare(
        &mut self,
        device: &wgpu::Device,
        host: &mut RetainedHost,
        texture: &wgpu::Texture,
        scheduler: &worldrunner_render_scene::compositor::ConfiguredFrameRenderer,
        pages_ready: bool,
    ) -> Result<Option<(u64, Products)>, ViewerError> {
        if self.pending.len() == 3 {
            return Ok(None);
        }
        let Some(&(id, _, _)) = host.captures.waiting.iter().find(|r| {
            host.capture_ready(r, pages_ready)
                && !host
                    .captures
                    .recipes
                    .get(&r.0)
                    .is_some_and(crate::RetainedCaptureRecipe::accumulated)
        }) else {
            return Ok(None);
        };
        let scene = scheduler.targets().texture();
        let pool = host.capture_pool_bytes();
        host.captures
            .resize_reservation(
                id,
                [
                    texture.width().max(scene.width()),
                    texture.height().max(scene.height()),
                ],
                pool,
            )
            .map_err(|error| ViewerError::Driver(error.to_string()))?;
        let outputs = host
            .captures
            .recipes
            .get(&id)
            .map_or(&[crate::CaptureOutput::Srgb8][..], |recipe| {
                recipe.outputs()
            });
        let mut image = Products::new(device, texture, scene, outputs, self.spare.pop())?;
        image.features(device, scheduler, outputs)?;
        Ok(Some((id, image)))
    }

    pub(super) fn submitted(
        &mut self,
        host: &mut RetainedHost,
        image: Option<(u64, Products)>,
        ticket: CompletionTicket,
        completion: &CaptureCompletion,
    ) -> Result<(), ViewerError> {
        self.advance_frame();
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
                frame: completion.frame,
                scene: completion.scene,
                temporal_history_reset: completion.temporal_history_reset,
                time: completion.time,
                ticket,
                image,
            });
        }
        Ok(())
    }

    pub(super) fn collect(&mut self, host: &mut RetainedHost) -> Result<(), ViewerError> {
        if host.captures.waiting.is_empty() && self.active.is_some() {
            self.active = None;
            return Ok(());
        }
        if let Some(active) = &mut self.active
            && active.collect(host)?
        {
            self.active
                .take()
                .expect("completed accumulation")
                .finish(host);
        }
        let mut index = 0;
        while index < self.pending.len() {
            let pending = &mut self.pending[index];
            if pending.ticket.is_complete()
                && let Some(image) = pending.image.try_read()?
            {
                if !host.captures.accepts_result(pending.id) {
                    // Cancellation delivery must not publish a second result.
                } else if host.captures.recipes.contains_key(&pending.id) {
                    host.captures.complete_recipe(
                        pending.id,
                        &CaptureCompletion {
                            frame: pending.frame,
                            time: pending.time,
                            scene: pending.scene,
                            temporal_history_reset: pending.temporal_history_reset,
                        },
                        image,
                    );
                } else {
                    host.captures.complete(
                        pending.id,
                        &CaptureCompletion {
                            frame: pending.frame,
                            time: pending.time,
                            scene: pending.scene,
                            temporal_history_reset: pending.temporal_history_reset,
                        },
                        image.srgb8.expect("plain capture"),
                    );
                }
                let pending = self.pending.remove(index);
                // The map and GPU ticket have both finished before slot reuse.
                if self.spare.len() < 3
                    && let Some(display) = pending.image.display
                {
                    self.spare.push(display);
                }
            } else {
                index += 1;
            }
        }
        Ok(())
    }
}

pub(super) struct Encoding<'a> {
    pub(super) device: &'a wgpu::Device,
    pub(super) resident: &'a super::Resident,
    pub(super) presentation: &'a crate::presentation::Presentation,
}

impl Encoding<'_> {
    pub(super) fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        output: &wgpu::TextureView,
        retained: Option<&mut Products>,
        fitted: bool,
    ) -> Result<(), ViewerError> {
        if let Some(retained) = retained {
            if !fitted {
                retained.replay(
                    self.device,
                    encoder,
                    self.resident,
                    &self.presentation.scheduler,
                )?;
            }
            retained.encode(
                encoder,
                output.texture(),
                self.presentation.scheduler.targets().texture(),
            )?;
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

#[cfg(test)]
mod feature_tests;

#[cfg(test)]
mod specular_tests;

#[cfg(test)]
mod lighting_term_tests;

#[cfg(test)]
mod screen_tests;
