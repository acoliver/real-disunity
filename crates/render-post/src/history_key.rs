//! What a TAA history bank is valid for. A bank is reused only for an identical key.

/// The extents, history kind and sample count a shared history bank was created for.
///
/// Native TAA keeps history on the render grid, so presenting at another output extent does not
/// change its key. Temporal reconstruction keeps history on the output grid, so either extent
/// changing does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HistoryKey {
    render: [u32; 2],
    output: [u32; 2],
    temporal: bool,
    samples: u32,
}
impl HistoryKey {
    /// `output` is `Some` only when temporal reconstruction was requested. At equal extents the
    /// stage is the native TAA, so the key is the native key.
    pub fn new(render: [u32; 2], output: Option<[u32; 2]>, samples: u32) -> Self {
        match output.filter(|&output| output != render) {
            Some(output) => Self {
                render,
                output,
                temporal: true,
                samples,
            },
            None => Self {
                render,
                output: render,
                temporal: false,
                samples,
            },
        }
    }

    /// The render extent, which sizes the native history.
    pub const fn render(&self) -> [u32; 2] {
        self.render
    }
}
