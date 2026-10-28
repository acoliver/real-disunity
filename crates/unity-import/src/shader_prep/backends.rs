//! Whether naga can write a checked module for each backend wgpu uses on desktop.
//!
//! The writers run with the bounds checks and SPIR-V flags wgpu uses when a device does not
//! make out-of-bounds access safe. Versions are fixed, where wgpu picks them per device, and
//! bindings the module does not map get naga's placeholders, where wgpu supplies its own maps.
//! The output is discarded: no Vulkan driver, Metal compiler, FXC or DXC sees it.

use super::{NagaModule, PrepareError, guarded, variant_name};

/// A naga writer for one of wgpu's desktop backends.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Backend {
    /// SPIR-V 1.0, for Vulkan 1.0; wgpu writes newer SPIR-V for newer Vulkan.
    SpirV,
    /// Metal Shading Language 2.4, for macOS 12; wgpu writes newer MSL for newer systems.
    Msl,
    /// HLSL shader model 5.1, for DX12 through FXC; with DXC, wgpu uses shader model 6.
    Hlsl,
}

impl Backend {
    pub const ALL: [Self; 3] = [Self::SpirV, Self::Msl, Self::Hlsl];

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::SpirV => "SPIR-V",
            Self::Msl => "MSL",
            Self::Hlsl => "HLSL",
        }
    }

    /// Writes a module for this backend and discards the output.
    ///
    /// # Errors
    /// Fails with the writer's error, named by its variant, or when the writer panics.
    pub fn write(self, naga: &NagaModule) -> Result<(), PrepareError> {
        guarded(self.name(), || {
            let (module, info) = (&naga.module, &naga.info);
            let written = match self {
                Self::SpirV => naga::back::spv::write_vec(module, info, &spv_options(), None)
                    .map(drop)
                    .map_err(|error| failure(&error)),
                Self::Msl => {
                    let options = naga::back::msl::Options {
                        lang_version: (2, 4),
                        bounds_check_policies: restricted(),
                        ..naga::back::msl::Options::default()
                    };
                    let pipeline = naga::back::msl::PipelineOptions::default();
                    naga::back::msl::write_string(module, info, &options, &pipeline)
                        .map(drop)
                        .map_err(|error| failure(&error))
                }
                Self::Hlsl => {
                    let (options, mut out) = (naga::back::hlsl::Options::default(), String::new());
                    let pipeline = naga::back::hlsl::PipelineOptions::default();
                    naga::back::hlsl::Writer::new(&mut out, &options, &pipeline)
                        .write(module, info, None)
                        .map(drop)
                        .map_err(|error| failure(&error))
                }
            };
            written.map_err(|(cause, message)| PrepareError::Write {
                backend: self.name(),
                cause,
                message,
            })
        })
    }
}

/// The bounds checks wgpu asks for when a device does not make out-of-bounds access safe.
fn restricted() -> naga::proc::BoundsCheckPolicies {
    use naga::proc::BoundsCheckPolicy::Restrict;
    naga::proc::BoundsCheckPolicies {
        index: Restrict,
        buffer: Restrict,
        image_load: Restrict,
        ..naga::proc::BoundsCheckPolicies::default()
    }
}

/// SPIR-V options as wgpu sets them, apart from the version and bindings: wgpu leaves the
/// coordinate space and fragment depth alone.
fn spv_options() -> naga::back::spv::Options<'static> {
    use naga::back::spv::WriterFlags;
    let mut options = naga::back::spv::Options {
        bounds_check_policies: restricted(),
        ..naga::back::spv::Options::default()
    };
    options
        .flags
        .remove(WriterFlags::ADJUST_COORDINATE_SPACE | WriterFlags::CLAMP_FRAG_DEPTH);
    options
}

/// A writer error's variant name and message.
fn failure(error: &(impl std::fmt::Debug + std::fmt::Display)) -> (String, String) {
    (variant_name(error), error.to_string())
}
