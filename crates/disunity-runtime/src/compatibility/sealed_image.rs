//! Linux descriptor-owned attachment to one immutable native image.

mod boot;
mod lease;
mod process;

pub use lease::NativeImageLease;

use crate::HostError;

fn rejected(message: impl std::fmt::Display) -> HostError {
    HostError::Compatibility(format!("sealed native image: {message}"))
}
