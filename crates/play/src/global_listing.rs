//! The textures the world's globals held at a piece of offscreen work.

/// The textures the world's globals held when one piece of work was mapped, as `name: kind`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlobalListing {
    /// The frame the work belongs to, counting from zero: a flush the managed engine asked for
    /// during a frame's script update, or that frame's own snapshot.
    pub frame: u64,
    /// `flush` or `frame`.
    pub source: &'static str,
    pub textures: Vec<String>,
}

impl GlobalListing {
    /// The listing as a report line.
    #[must_use]
    pub fn line(&self) -> String {
        format!(
            "globals at a {} of frame {}: {}",
            self.source,
            self.frame,
            self.textures.join(", ")
        )
    }
}
