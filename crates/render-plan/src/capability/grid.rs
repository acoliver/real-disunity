//! Bounded pure U8 and exact little-endian U16 grid capability handlers.
//!
//! The caller supplies the exact shape semantics; the byte length of a grid is
//! derived once through checked arithmetic at handler construction, and every
//! session treats that length as both its independent accumulation bound and
//! its exact required length. No dimension is compiled into this module.

use std::sync::Arc;

use worldrunner_world_runtime::{CapabilityHandler, CapabilitySessionError};

use crate::capability::route::{CapabilityConfigError, ExactCapabilityRoute};
use crate::capability::session::{ExactCapabilityHandler, ExactDecoder};

/// Exact N-dimensional element extents one grid payload must fill.
///
/// Extents are wholly caller authority: an empty extent list denotes one
/// scalar element, and any zero extent denotes a structurally coherent empty
/// grid of zero elements and zero bytes. The element count and byte length
/// are always derived through checked arithmetic, never trusted arithmetic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GridShape {
    extents: Vec<usize>,
}

impl GridShape {
    /// Adopts caller-authored extents without interpretation.
    #[must_use]
    pub fn new(extents: &[usize]) -> Self {
        Self {
            extents: extents.to_vec(),
        }
    }

    /// Returns the exact caller-authored extents.
    #[must_use]
    pub fn extents(&self) -> &[usize] {
        &self.extents
    }

    /// Returns the checked product of all extents.
    ///
    /// # Errors
    ///
    /// Returns [`CapabilityConfigError::GridLengthOverflow`] when the element
    /// count overflows.
    pub fn element_count(&self) -> Result<usize, CapabilityConfigError> {
        self.extents
            .iter()
            .try_fold(1_usize, |product, extent| product.checked_mul(*extent))
            .ok_or(CapabilityConfigError::GridLengthOverflow)
    }

    /// Returns the checked byte length for elements of `element_bytes` bytes.
    ///
    /// # Errors
    ///
    /// Returns [`CapabilityConfigError::GridLengthOverflow`] when the element
    /// count or byte length overflows.
    pub fn byte_length(&self, element_bytes: usize) -> Result<usize, CapabilityConfigError> {
        self.element_count()?
            .checked_mul(element_bytes)
            .ok_or(CapabilityConfigError::GridLengthOverflow)
    }
}

/// Owned immutable pure-U8 grid payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct U8Grid {
    shape: GridShape,
    values: Vec<u8>,
}

impl U8Grid {
    /// Returns the exact shape the values fill.
    #[must_use]
    pub fn shape(&self) -> &GridShape {
        &self.shape
    }

    /// Returns the wholly owned values in row-major stream order.
    #[must_use]
    pub fn values(&self) -> &[u8] {
        &self.values
    }
}

/// Owned immutable grid payload of exact little-endian U16 values.
///
/// The streamed bytes are converted to native `u16` values by exact
/// little-endian interpretation; the payload never retains the raw bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct U16LeGrid {
    shape: GridShape,
    values: Vec<u16>,
}

impl U16LeGrid {
    /// Returns the exact shape the values fill.
    #[must_use]
    pub fn shape(&self) -> &GridShape {
        &self.shape
    }

    /// Returns the wholly owned values in row-major stream order.
    #[must_use]
    pub fn values(&self) -> &[u16] {
        &self.values
    }
}

fn grid_length_rejection(observed: usize, expected: usize) -> CapabilitySessionError {
    CapabilitySessionError::Rejected {
        reason: format!(
            "grid payload carries {observed} bytes but the exact shape requires {expected} bytes"
        ),
    }
}

struct U8GridDecoder {
    shape: GridShape,
    expected_bytes: usize,
}

impl ExactDecoder for U8GridDecoder {
    type Payload = U8Grid;

    fn max_stream_bytes(&self) -> usize {
        self.expected_bytes
    }

    fn decode(&self, bytes: Vec<u8>) -> Result<Self::Payload, CapabilitySessionError> {
        if bytes.len() != self.expected_bytes {
            return Err(grid_length_rejection(bytes.len(), self.expected_bytes));
        }
        Ok(U8Grid {
            shape: self.shape.clone(),
            values: bytes,
        })
    }
}

struct U16LeGridDecoder {
    shape: GridShape,
    expected_bytes: usize,
}

impl ExactDecoder for U16LeGridDecoder {
    type Payload = U16LeGrid;

    fn max_stream_bytes(&self) -> usize {
        self.expected_bytes
    }

    fn decode(&self, bytes: Vec<u8>) -> Result<Self::Payload, CapabilitySessionError> {
        if bytes.len() != self.expected_bytes {
            return Err(grid_length_rejection(bytes.len(), self.expected_bytes));
        }
        let values = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect();
        Ok(U16LeGrid {
            shape: self.shape.clone(),
            values,
        })
    }
}

/// Builds the pure-U8 grid handler for one caller-authored route and shape.
///
/// The caller is the sole authority for the exact capability URI, the exact
/// payload-contract URI, the handler descriptor, the element policy in
/// `route`, and every grid extent in `shape`. The exact byte length is
/// derived once through checked arithmetic and becomes the session's
/// independent bound; zero element counts are accepted as coherent empty
/// grids.
///
/// # Errors
///
/// Returns [`CapabilityConfigError::GridLengthOverflow`] when the derived
/// byte length overflows.
pub fn u8_grid_handler(
    route: ExactCapabilityRoute,
    shape: GridShape,
) -> Result<Arc<dyn CapabilityHandler>, CapabilityConfigError> {
    let expected_bytes = shape.byte_length(1)?;
    Ok(Arc::new(ExactCapabilityHandler::new(
        route,
        U8GridDecoder {
            shape,
            expected_bytes,
        },
    )))
}

/// Builds the little-endian U16 grid handler for one caller-authored route and shape.
///
/// The authority boundary matches [`u8_grid_handler`]; the derived byte length
/// counts two bytes per element, and streamed pairs are converted by exact
/// little-endian interpretation at finish.
///
/// # Errors
///
/// Returns [`CapabilityConfigError::GridLengthOverflow`] when the derived
/// byte length overflows.
pub fn u16_le_grid_handler(
    route: ExactCapabilityRoute,
    shape: GridShape,
) -> Result<Arc<dyn CapabilityHandler>, CapabilityConfigError> {
    let expected_bytes = shape.byte_length(2)?;
    Ok(Arc::new(ExactCapabilityHandler::new(
        route,
        U16LeGridDecoder {
            shape,
            expected_bytes,
        },
    )))
}
