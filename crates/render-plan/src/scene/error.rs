//! Typed fail-closed validation failures for renderer-domain scene closure.

use thiserror::Error;

/// One violated invariant inside a composed [`crate::scene::RenderScene`].
///
/// Every constructor of the scene model validates its own closure before
/// adopting content, so a scene value and its parts are never observed in a
/// state that violates these invariants. The error carries the exact part
/// and the offending quantities so a refusal is attributable to one rule.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum SceneValidationError {
    /// Mesh ceilings must be present per level, positive and strictly increasing.
    #[error("mesh LOD distances require one positive, strictly increasing ceiling per level")]
    LodDistances,
    /// A light direction is not a quantized unit vector.
    #[error("scene direction must be a unit vector in micro-units")]
    DirectionNotNormalized,
    /// A mandatory collection was empty.
    #[error("{part} must not be empty")]
    EmptyCollection {
        /// The scene part whose collection was empty.
        part: &'static str,
    },
    /// A checked dimension, count, or byte computation left the `u64` range.
    #[error("{part} dimension or byte arithmetic overflowed")]
    ArithmeticOverflow {
        /// The scene part whose computation overflowed.
        part: &'static str,
    },
    /// A texel grid carried zero width or height.
    #[error("{part} dimensions {width}x{height} must both be strictly positive")]
    InvalidDimensions {
        /// The scene part carrying the grid.
        part: &'static str,
        /// Authored width.
        width: u32,
        /// Authored height.
        height: u32,
    },
    /// A mip chain count exceeds the addressable levels of its grid.
    #[error("{part} mip count {observed} exceeds addressable level count {maximum}")]
    MipOverflow {
        /// The scene part carrying the mip chain.
        part: &'static str,
        /// Authored mip count.
        observed: u32,
        /// Highest legal mip count for the grid dimensions.
        maximum: u32,
    },
    /// A byte payload length did not satisfy its exact format arithmetic.
    #[error("{part} byte length {observed} does not satisfy its format ({reason})")]
    ByteLengthMismatch {
        /// The scene part carrying the payload.
        part: &'static str,
        /// Why the length violated the format.
        reason: &'static str,
        /// The payload length the format requires.
        expected: u64,
        /// The payload length observed.
        observed: u64,
    },
    /// An encoded container arrived with zero bytes although its encoding
    /// requires a non-empty payload.
    #[error("{part} encoding {encoding:?} requires a non-empty payload")]
    EmptyEncodedPayload {
        /// The scene part carrying the payload.
        part: &'static str,
        /// The container encoding that requires content.
        encoding: &'static str,
    },
    /// One referenced handle lies outside the addressed table.
    #[error("{part} references {resource} handle {handle} outside 0..{limit}")]
    HandleOutOfRange {
        /// The scene part carrying the reference.
        part: &'static str,
        /// The referenced resource table.
        resource: &'static str,
        /// The offending handle value.
        handle: u32,
        /// The exclusive upper bound of the table.
        limit: u64,
    },
    /// One index buffer entry lies outside its LOD's vertex table.
    #[error("{part} index {index} lies outside vertex count {limit}")]
    IndexOutOfRange {
        /// The scene part carrying the index buffer.
        part: &'static str,
        /// The offending index.
        index: u64,
        /// The vertex count bounding legal indices.
        limit: u64,
    },
    /// An index buffer does not describe whole triangles.
    #[error("{part} carries {observed} indices, not a multiple of three")]
    NonTriangleList {
        /// The scene part carrying the index buffer.
        part: &'static str,
        /// The offending index count.
        observed: u64,
    },
    /// LOD level indices are not contiguous and zero-based.
    #[error(
        "{part} level at position {position} carries index {observed} but {expected} is required"
    )]
    LodOrdering {
        /// The scene part carrying the LOD chain.
        part: &'static str,
        /// The zero-based position of the offending level.
        position: usize,
        /// The required contiguous index.
        expected: u8,
        /// The authored index.
        observed: u8,
    },
    /// A rotation quaternion is not normalized in micro-units to within the
    /// quantization tolerance.
    #[error("{part} rotation is not normalized in micro-units within the quantization tolerance")]
    RotationNotNormalized {
        /// The scene part carrying the rotation.
        part: &'static str,
    },
    /// An integer quantity lies outside its inclusive range.
    #[error("{part} value {observed} lies outside {minimum}..={maximum}")]
    RangeViolation {
        /// The scene part carrying the quantity.
        part: &'static str,
        /// Inclusive lower bound.
        minimum: u64,
        /// Inclusive upper bound.
        maximum: u64,
        /// The offending value.
        observed: u64,
    },
    /// A grading control has no governed shader implementation.
    #[error(
        "{part} value {observed} is unsupported: governed rendering supports only neutral {neutral}; no shader consumer exists"
    )]
    UnsupportedColorGrading {
        /// The unsupported grading field.
        part: &'static str,
        /// The authored value in micro-units.
        observed: u32,
        /// The supported neutral value in micro-units.
        neutral: u64,
    },
    /// Shadow cascade extents do not strictly increase.
    #[error("{part} cascade extent at position {position} does not strictly increase")]
    CascadeNotIncreasing {
        /// The scene part carrying the cascades.
        part: &'static str,
        /// The zero-based position of the offending cascade.
        position: usize,
    },
    /// The camera far plane does not lie strictly beyond the near plane.
    #[error("{part} far plane must lie strictly beyond the near plane")]
    PlaneOrdering {
        /// The scene part carrying the planes.
        part: &'static str,
    },
    /// A terrain grid byte length did not match its declared dimensions.
    #[error("{part} grid carries {observed} samples but its dimensions require {expected}")]
    TerrainSampleCount {
        /// The scene part carrying the grid.
        part: &'static str,
        /// The required sample count.
        expected: u64,
        /// The observed sample count.
        observed: u64,
    },
    /// A terrain material index lies outside the palette.
    #[error("{part} material index {index} lies outside palette limit {limit}")]
    MaterialIndexOutOfRange {
        /// The scene part carrying the material grid.
        part: &'static str,
        /// The offending palette index.
        index: u8,
        /// The exclusive upper bound of the palette.
        limit: u64,
    },
    /// A rectangle extent is not strictly positive.
    #[error("{part} extent must be strictly positive")]
    ExtentNotPositive {
        /// The scene part carrying the rectangle.
        part: &'static str,
    },
}
