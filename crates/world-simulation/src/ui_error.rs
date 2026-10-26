/// Process-unique font identity, never reused across font books.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontId(pub(crate) u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum UiError {
    #[error("invalid rectangle")]
    InvalidRect,
    #[error("invalid Canvas or scaler")]
    InvalidCanvas,
    #[error("invalid viewport")]
    InvalidViewport,
    #[error("screen Canvas must be an object root")]
    ScreenCanvasParent,
    #[error("camera must be outside a Canvas and have a nonsingular rigid transform")]
    InvalidCamera,
    #[error("UI requires a RectTransform and Canvas ancestor")]
    MissingLayout,
    #[error("invalid font bytes")]
    InvalidFont,
    #[error("font is not registered: {0:?}")]
    MissingFont(FontId),
    #[error("font storage limit")]
    FontLimit,
    #[error("invalid text settings")]
    InvalidText,
    #[error("unsupported shaping or control character: {0:?}")]
    UnsupportedShaping(char),
    #[error("rich text markup is not supported")]
    UnsupportedRichText,
    #[error("font has no glyph: {0:?}")]
    MissingGlyph(char),
    #[error("glyph count limit")]
    GlyphLimit,
    #[error("glyph atlas limit")]
    AtlasLimit,
}
