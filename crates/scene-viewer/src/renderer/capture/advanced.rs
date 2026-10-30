use super::{Products, accumulation::Accumulation};
use crate::renderer::Renderer;
use crate::{RetainedCaptureRecipe, RetainedHost, ViewerError};
use std::time::Duration;
use worldrunner_render_gpu::flight::CompletionTicket;
use worldrunner_render_scene::{SceneView, Viewport};

pub(in crate::renderer) struct Active {
    id: u64,
    frame: u64,
    time: Duration,
    view: SceneView,
    size: Viewport,
    recipe: RetainedCaptureRecipe,
    sample: u32,
    accumulation: Accumulation,
    pending: Option<(Products, CompletionTicket)>,
}
impl Active {
    pub(super) fn collect(&mut self, host: &mut RetainedHost) -> Result<bool, ViewerError> {
        let Some((image, ticket)) = &mut self.pending else {
            return Ok(false);
        };
        if !ticket.is_complete() {
            return Ok(false);
        }
        let Some(products) = image.try_read()? else {
            return Ok(false);
        };
        self.accumulation.add(&products);
        self.pending = None;
        self.sample += 1;
        if self.sample != self.recipe.temporal_samples() {
            return Ok(false);
        }
        let index = host
            .captures
            .waiting
            .iter()
            .position(|r| r.0 == self.id)
            .expect("active request");
        host.captures.waiting.remove(index);
        Ok(true)
    }
    pub(super) fn finish(self, host: &mut RetainedHost) {
        host.captures
            .complete_recipe(self.id, self.frame, self.time, self.accumulation.finish());
    }
}
impl Renderer {
    /// Returns None when ordinary frame execution should continue.
    pub(in crate::renderer) fn render_recipe(
        &mut self,
        host: &mut RetainedHost,
    ) -> Result<Option<bool>, ViewerError> {
        if self.streaming.captures.active.is_none() {
            let queued = host.captures.waiting.iter().any(|r| {
                host.captures
                    .recipes
                    .get(&r.0)
                    .is_some_and(RetainedCaptureRecipe::accumulated)
            });
            if !queued {
                return Ok(None);
            }
            self.publish_retained(host)?;
            let Some(view) = host.view()? else {
                return Ok(Some(false));
            };
            let pages = self.retained_pages_ready(host, &view)?;
            let request = host
                .captures
                .waiting
                .iter()
                .find(|r| host.capture_ready(r, pages));
            let Some(&(id, _, _)) = request else {
                return Ok(None);
            };
            let Some(recipe) = host
                .captures
                .recipes
                .get(&id)
                .filter(|r| r.accumulated())
                .cloned()
            else {
                return Ok(None);
            };
            host.captures
                .resize_reservation(id, host.size())
                .map_err(|e| ViewerError::Driver(e.to_string()))?;
            let size = Viewport {
                width: host.size()[0],
                height: host.size()[1],
            };
            self.streaming.captures.active = Some(Active {
                id,
                frame: self.streaming.captures.next_frame(),
                time: self.clock.time(),
                view,
                size,
                accumulation: Accumulation::new(size, &recipe),
                recipe,
                sample: 0,
                pending: None,
            });
        }
        self.submit_recipe_sample(host).map(Some)
    }
    fn submit_recipe_sample(&mut self, host: &mut RetainedHost) -> Result<bool, ViewerError> {
        let active = self
            .streaming
            .captures
            .active
            .as_ref()
            .expect("active recipe");
        if active.pending.is_some() {
            return Ok(false);
        }
        let id = active.id;
        let factor = active.recipe.supersample();
        let size = Viewport {
            width: active.size.width * factor,
            height: active.size.height * factor,
        };
        let _ = crate::presentation::Extents::for_device(
            [size.width, size.height],
            self.context.device(),
        )?;
        let view = active
            .view
            .with_raster_sample(size, active.recipe.jitter_offset(active.sample))?;
        let time = active.recipe.subframe_time(active.time, active.sample);
        let outputs = active.recipe.outputs().to_vec();
        self.presentation
            .resize_recipe(&self.context, size, active.recipe.native_rcas())?;
        let Some(ready) = self
            .execution
            .acquire_capture(&self.context, [size.width, size.height])?
        else {
            return Ok(false);
        };
        let pages = self.retained_pages_ready(host, &view)?;
        let waits_pages = !pages && host.captures.waiting.iter().any(|r| r.0 == id && r.2);
        let waits_geometry = host.captures.geometry.contains(&id) && !host.captures.geometry_ready;
        if waits_pages || waits_geometry {
            return self.warm_recipe_pages(&view, ready);
        }
        self.resident.select_frame_slot(ready.admission.slot());
        self.presentation
            .scheduler
            .select_frame_slot(ready.admission.slot());
        self.frame_time = time;
        self.presentation.scheduler.set_frame_time_duration(time)?;
        self.encode_recipe_sample(&view, ready, &outputs)
    }
    fn encode_recipe_sample(
        &mut self,
        view: &SceneView,
        ready: crate::renderer::execution::ReadyFrame,
        outputs: &[crate::CaptureOutput],
    ) -> Result<bool, ViewerError> {
        let mut image = Products::new(
            self.context.device(),
            &ready.frame.texture,
            self.presentation.scheduler.targets().texture(),
            outputs,
            None,
        )?;
        let output = ready
            .frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let prepared = self.encode_frame(view, None, &output, Some(&mut image))?;
        let ticket = self.execution.submit_capture(ready, prepared)?;
        self.streaming.captures.advance_frame();
        image.begin()?;
        self.streaming
            .captures
            .active
            .as_mut()
            .expect("active recipe")
            .pending = Some((image, ticket));
        Ok(true)
    }
    fn warm_recipe_pages(
        &mut self,
        view: &SceneView,
        ready: crate::renderer::execution::ReadyFrame,
    ) -> Result<bool, ViewerError> {
        self.resident.select_frame_slot(ready.admission.slot());
        self.presentation
            .scheduler
            .select_frame_slot(ready.admission.slot());
        let output = ready
            .frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let prepared = self.encode_frame(view, None, &output, None)?;
        self.execution.submit_capture(ready, prepared)?;
        self.streaming.captures.advance_frame();
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
