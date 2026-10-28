use super::{RetainedDriver, RetainedHost, Session};
use crate::{ViewerError, app::Content, renderer::Renderer};

/// Request-driven retained execution without a window or event loop.
/// No work is scheduled between calls. Call `frame` again if admission is busy,
/// and `poll` to deliver asynchronous capture completions without advancing time.
pub struct WindowlessRetained<'a> {
    session: Session<'a>,
    renderer: Renderer,
    closed: bool,
}
impl RetainedHost {
    pub fn windowless(
        self,
        driver: &mut impl RetainedDriver,
    ) -> Result<WindowlessRetained<'_>, ViewerError> {
        let renderer = pollster::block_on(Renderer::windowless(&self));
        let mut session = Session::new(self, driver);
        match renderer {
            Ok(renderer) => Ok(WindowlessRetained {
                session,
                renderer,
                closed: false,
            }),
            Err(error) => {
                let _ = session.shutdown();
                Err(error)
            }
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

    /// Requests one update/submission through the same session as desktop mode.
    /// Returns false if no ordinary frame is submitted, including capture-only subframes.
    pub fn frame(&mut self) -> Result<bool, ViewerError> {
        self.ensure_open()?;
        if self.session.host.exit {
            self.shutdown()?;
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
        self.ensure_open()?;
        if self.session.host.exit {
            self.shutdown()?;
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
    fn finish<T>(&mut self, result: Result<T, ViewerError>) -> Result<T, ViewerError> {
        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                let _ = self.shutdown();
                Err(error)
            }
        }
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
        let shutdown = self.session.shutdown();
        let drain = self.renderer.drain();
        shutdown?;
        drain
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
