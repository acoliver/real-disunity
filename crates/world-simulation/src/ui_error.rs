/// Process-unique font identity, never reused across font books.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontId(pub(crate) u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum UiError {
    #[error("invalid linear RGBA colour or alpha")]
    InvalidColor,
    #[error("invalid image source, insets or tile scale")]
    InvalidImage,
    #[error("draw-list expanded vertex budget exceeded")]
    DrawLimit,
    #[error("draw-list hierarchy range exhausted")]
    HierarchyLimit,
    #[error("draw-list stack depth exceeded")]
    StackLimit,
    #[error("cannot pop the draw-list root state")]
    StackUnderflow,
    #[error("draw-list stacks must be balanced before finishing")]
    UnbalancedStack,
    #[error("invalid text effect parameters")]
    InvalidTextEffect,
    #[error("distance field exceeds side, spread or scratch limits")]
    DistanceFieldLimit,
    #[error("distance text requires an invertible affine quad and UV mapping")]
    InvalidDistanceMapping,
    #[error("invalid annotation geometry or alignment")]
    InvalidAnnotation,
    #[error("annotation segment or frame vertex budget exceeded")]
    AnnotationLimit,
    #[error("depth-tested annotations require scene depth unavailable in the display overlay")]
    UnsupportedAnnotationDepth,
    #[error("invalid rectangle")]
    InvalidRect,
    #[error("invalid Canvas or scaler")]
    InvalidCanvas,
    #[error("invalid CanvasGroup alpha")]
    InvalidCanvasGroup,
    #[error("invalid CanvasRenderer")]
    InvalidCanvasRenderer,
    #[error("invalid viewport")]
    InvalidViewport,
    #[error("camera must be outside a Canvas and have a nonsingular rigid transform")]
    InvalidCamera,
    #[error("UI requires a RectTransform and Canvas ancestor")]
    MissingLayout,
    #[error("invalid font bytes")]
    InvalidFont,
    #[error("invalid GPOS table")]
    InvalidGpos,
    #[error("invalid GPOS extension lookup")]
    InvalidGposExtension,
    #[error("GPOS positioning work or subtable limit exceeded")]
    PositioningLimit,
    #[error("font is not registered: {0:?}")]
    MissingFont(FontId),
    #[error("font face is not loaded")]
    MissingFace,
    #[error("font has no glyph with that index")]
    InvalidGlyph,
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
