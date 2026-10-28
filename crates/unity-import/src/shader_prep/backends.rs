//! Whether naga can write a checked module for each backend wgpu uses on desktop.

use super::{NagaModule, PrepareError, guarded, variant_name};

/// A naga writer for one of wgpu's desktop backends.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Backend {
    /// SPIR-V 1.0, for Vulkan.
    SpirV,
    /// Metal Shading Language 2.4, for Metal.
    Msl,
    /// HLSL shader model 5.1, for DX12 through FXC.
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

    /// Writes a module for this backend with naga's default options, apart from the MSL
    /// version, and discards the output. Bindings the module does not map are given
    /// placeholders, as naga does by default.
    ///
    /// # Errors
    /// Fails with the writer's error, named by its variant, or when the writer panics.
    pub fn write(self, naga: &NagaModule) -> Result<(), PrepareError> {
        guarded(self.name(), || {
            let (module, info) = (&naga.module, &naga.info);
            let written = match self {
                Self::SpirV => {
                    let options = naga::back::spv::Options::default();
                    naga::back::spv::write_vec(module, info, &options, None)
                        .map(drop)
                        .map_err(|error| failure(&error))
                }
                Self::Msl => {
                    let options = naga::back::msl::Options {
                        lang_version: (2, 4),
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

/// A writer error's variant name and message.
fn failure(error: &(impl std::fmt::Debug + std::fmt::Display)) -> (String, String) {
    (variant_name(error), error.to_string())
}
