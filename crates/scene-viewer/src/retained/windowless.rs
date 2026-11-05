use super::{RetainedDriver, RetainedHost, Session};
use crate::{ViewerError, app::Content, renderer::Renderer};

/// Request-driven retained execution without a window or event loop.
/// No frames are scheduled between calls. Call `frame` again if admission is busy,
/// and `poll` to deliver CPU work and capture completions without advancing time.
pub struct WindowlessRetained<'a> {
    session: Session<'a>,
    renderer: Renderer,
    closed: bool,
    activity: Option<crate::activity::ActivityGuard>,
}
impl RetainedHost {
    pub fn windowless(
        self,
        driver: &mut impl RetainedDriver,
    ) -> Result<WindowlessRetained<'_>, ViewerError> {
        let ((session, renderer), activity) = self.start_windowless_with_activity(
            driver,
            crate::activity::ActivityGuard::begin,
            initialize,
        )?;
        Ok(WindowlessRetained {
            session,
            renderer,
            closed: false,
            activity: Some(activity),
        })
    }

    pub(crate) fn start_windowless_with_activity<'a, G, T>(
        mut self,
        driver: &'a mut dyn RetainedDriver,
        begin: impl FnOnce(&str) -> G,
        start: impl FnOnce(Self, &'a mut dyn RetainedDriver) -> Result<T, ViewerError>,
    ) -> Result<(T, G), ViewerError> {
        let activity = begin(self.options.activity_reason.as_ref());
        self.input_state.window = super::input::WindowControlTarget::Windowless;
        Ok((start(self, driver)?, activity))
    }
}

fn initialize(
    mut host: RetainedHost,
    driver: &mut dyn RetainedDriver,
) -> Result<(Session<'_>, Renderer), ViewerError> {
    host.picking.windowless = true;
    host.timers.freeze();
    let renderer = pollster::block_on(Renderer::windowless(&mut host));
    let mut session = Session::new(host, driver);
    match renderer {
        Ok(mut renderer) => {
            renderer.collect_retained(&mut session.host)?;
            Ok((session, renderer))
        }
        Err(error) => {
            let _ = session.shutdown();
            Err(error)
        }
    }
}
impl WindowlessRetained<'_> {
    pub fn host(&self) -> &RetainedHost {
        &self.session.host
    }
    pub fn host_mut(&mut self) -> &mut RetainedHost {
        &mut self.session.host
    }

    /// Advances the session's monotonic clock without dispatching or rendering.
    /// Timers and update ticks observe this time on the next `frame` or `poll`.
    pub fn advance_time(&mut self, elapsed: std::time::Duration) -> Result<(), ViewerError> {
        self.ensure_open()?;
        self.session
            .host
            .timers
            .advance(elapsed)
            .map_err(|error| ViewerError::Driver(error.to_string()))
    }

    /// Requests one update/submission through the same session as desktop mode.
    /// Returns false if no ordinary frame is submitted, including capture-only subframes.
    pub fn frame(&mut self) -> Result<bool, ViewerError> {
        if !self.begin()? {
            return Ok(false);
        }
        if self.renderer.viewport.is_none() {
            self.poll()?;
            return Ok(false);
        }
        let before = self.session.frame;
        let result = self.session.redraw(&mut self.renderer);
        if self.finish(result)? {
            self.shutdown()?;
        }
        Ok(self.session.frame != before)
    }
    /// Completes existing work and delivers notifications, without requesting a frame.
    pub fn poll(&mut self) -> Result<bool, ViewerError> {
        if !self.begin()? {
            return Ok(false);
        }
        let result = self
            .renderer
            .poll()
            .and_then(|()| self.session.poll(&mut self.renderer));
        let pending = self.finish(result)?;
        if self.session.host.exit {
            self.shutdown()?;
            return Ok(false);
        }
        Ok(pending)
    }
    fn begin(&mut self) -> Result<bool, ViewerError> {
        self.ensure_open()?;
        let result = self.session.host_ready();
        self.finish(result)?;
        if self.session.host.exit {
            self.shutdown()?;
            return Ok(false);
        }
        Ok(true)
    }

    fn finish<T>(&mut self, result: Result<T, ViewerError>) -> Result<T, ViewerError> {
        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                let _ = self.shutdown();
                Err(error)
            }
        }
    }
    /// Deterministic display-scale transition without a native window. Size stays physical.
    pub fn set_display_scale(&mut self, scale_factor: f64) -> Result<(), ViewerError> {
        self.ensure_open()?;
        if !scale_factor.is_finite() || scale_factor <= 0. || scale_factor > f64::from(f32::MAX) {
            return Err(ViewerError::InvalidOptions("invalid display scale"));
        }
        self.session.input(super::RetainedInput::DisplayChanged {
            scale_factor,
            physical_size: self.session.host.size(),
        });
        let result = self.session.take_failure().map_or(Ok(()), Err);
        self.finish(result)
    }

    /// Explicit physical-pixel resize, delivered even when the surface is suspended.
    pub fn resize(&mut self, size: [u32; 2]) -> Result<(), ViewerError> {
        self.ensure_open()?;
        self.session.surface_size(size);
        let result = self.session.take_failure().map_or(Ok(()), Err);
        self.finish(result)?;
        if self.session.host.exit {
            return self.shutdown();
        }
        let result = self.renderer.resize(size);
        self.finish(result)
    }
    /// Cancels captures, notifies the driver once, and drains submitted work.
    /// Call explicitly to receive errors; drop reports unreturnable errors to stderr.
    pub fn shutdown(&mut self) -> Result<(), ViewerError> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        crate::activity::release_after(&mut self.activity, || {
            let shutdown = self.session.shutdown();
            let drain = self.renderer.drain();
            shutdown?;
            drain
        })
    }
    fn ensure_open(&self) -> Result<(), ViewerError> {
        if self.closed {
            return Err(ViewerError::Driver("windowless host is closed".into()));
        }
        Ok(())
    }
}
impl Drop for WindowlessRetained<'_> {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown() {
            super::session::report_shutdown_error("windowless retained", &error);
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;
