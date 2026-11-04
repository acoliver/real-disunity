//! Naga parsing and backend writing for prepared Unity Vulkan stage modules.

mod backends;

pub use backends::Backend;
pub use worldrunner_unity_serialized::spirv_prep::{
    PreparedModule, PreparedStages, ResourceBinding, prepare, prepare_stages,
};

use std::panic::{AssertUnwindSafe, catch_unwind};

use worldrunner_unity_serialized::spirv_prep;

/// Why a prepared module could not be checked or written with naga.
#[derive(Debug, thiserror::Error)]
pub enum PrepareError {
    #[error(transparent)]
    Prepare(#[from] spirv_prep::PrepareError),
    #[error("naga cannot parse the module: {cause}: {message}")]
    Parse { cause: String, message: String },
    #[error("naga rejects the module: {cause}: {message}")]
    Validate { cause: String, message: String },
    #[error("naga cannot write the module as {backend}: {cause}: {message}")]
    Write {
        backend: &'static str,
        cause: String,
        message: String,
    },
    #[error("{stage} panicked: {message}")]
    Panicked {
        stage: &'static str,
        message: String,
    },
}

impl PrepareError {
    /// A short name for the failure, for counting failures by cause.
    #[must_use]
    pub fn cause(&self) -> String {
        match self {
            Self::Prepare(error) => error.cause(),
            Self::Parse { cause, .. } => format!("parse {cause}"),
            Self::Validate { cause, .. } => format!("validate {cause}"),
            Self::Write { backend, cause, .. } => format!("{backend} {cause}"),
            Self::Panicked { stage, .. } => format!("{stage} panic"),
        }
    }
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic without a message".to_owned())
}

fn guarded<T>(
    stage: &'static str,
    run: impl FnOnce() -> Result<T, PrepareError>,
) -> Result<T, PrepareError> {
    catch_unwind(AssertUnwindSafe(run)).unwrap_or_else(|payload| {
        Err(PrepareError::Panicked {
            stage,
            message: panic_message(payload.as_ref()),
        })
    })
}

/// The name of an error's variant, from its `Debug` form.
fn variant_name(error: &impl std::fmt::Debug) -> String {
    let text = format!("{error:?}");
    let end = text.find(['(', ' ', '{']).unwrap_or(text.len());
    text[..end].to_owned()
}

/// The innermost cause of an error.
fn innermost<'e>(
    error: &'e (dyn std::error::Error + 'static),
) -> &'e (dyn std::error::Error + 'static) {
    let mut current = error;
    while let Some(source) = current.source() {
        current = source;
    }
    current
}

/// A module naga has parsed and validated.
pub struct NagaModule {
    pub module: naga::Module,
    pub info: naga::valid::ModuleInfo,
}

/// Parses and validates a prepared module with naga, with every validation capability
/// enabled.
///
/// # Errors
/// Fails with naga's parse or validation error, named by its innermost cause, or when naga
/// panics.
pub fn check_with_naga(words: &[u32]) -> Result<NagaModule, PrepareError> {
    guarded("naga", || {
        let bytes: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
        let options = naga::front::spv::Options::default();
        let module = naga::front::spv::parse_u8_slice(&bytes, &options).map_err(|error| {
            PrepareError::Parse {
                cause: variant_name(&error),
                message: error.to_string(),
            }
        })?;
        let flags = naga::valid::ValidationFlags::all();
        let capabilities = naga::valid::Capabilities::all();
        let info = naga::valid::Validator::new(flags, capabilities)
            .validate(&module)
            .map_err(|error| {
                let error = error.into_inner();
                PrepareError::Validate {
                    cause: variant_name(&innermost(&error)),
                    message: format!("{error:?}"),
                }
            })?;
        Ok(NagaModule { module, info })
    })
}
