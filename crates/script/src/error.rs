//! Errors crossing the script boundary. Script faults must surface as data,
//! never as panics across the Lua/Rust boundary.

use std::fmt;

/// Every failure mode of the script host, distinguished so callers can react
/// to budget exhaustion differently from a parse error in a shipped chunk.
#[derive(Debug)]
pub enum ScriptError {
    /// The chunk failed to compile; `line` is the Lua-side line number.
    Parse {
        chunk: String,
        line: i32,
        message: String,
    },
    /// The chunk ran and raised a Lua error.
    Runtime {
        chunk: String,
        line: i32,
        message: String,
    },
    /// A hook exceeded its per-fire instruction budget and was halted.
    BudgetExceeded { chunk: String, budget: u32 },
    /// A module reported an API version the host refuses to install.
    ApiVersion {
        module: String,
        expected: String,
        found: String,
    },
    /// Script code reached for something the sandbox contract withholds.
    Sandbox { message: String },
    /// A `UserData` handle was used after the world it named is gone.
    HandleInvalid { handle: String },
    /// An original script's patches were refused; nothing was loaded.
    Patch(crate::patches::PatchError),
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse {
                chunk,
                line,
                message,
            } => {
                write!(f, "{chunk}:{line}: parse error: {message}")
            }
            Self::Runtime {
                chunk,
                line,
                message,
            } => {
                write!(f, "{chunk}:{line}: runtime error: {message}")
            }
            Self::BudgetExceeded { chunk, budget } => {
                write!(f, "{chunk}: instruction budget of {budget} exceeded")
            }
            Self::ApiVersion {
                module,
                expected,
                found,
            } => write!(
                f,
                "module {module} reports api version {found}, host requires {expected}"
            ),
            Self::Sandbox { message } => write!(f, "sandbox violation: {message}"),
            Self::HandleInvalid { handle } => {
                write!(f, "handle {handle} is invalid (world unloaded)")
            }
            Self::Patch(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for ScriptError {}

impl From<crate::patches::PatchError> for ScriptError {
    fn from(error: crate::patches::PatchError) -> Self {
        Self::Patch(error)
    }
}

impl From<mlua::Error> for ScriptError {
    fn from(error: mlua::Error) -> Self {
        match error {
            mlua::Error::SyntaxError { message, .. } => Self::Parse {
                chunk: "<chunk>".into(),
                line: 0,
                message: message.to_string(),
            },
            mlua::Error::RuntimeError(message) => Self::Runtime {
                chunk: "<chunk>".into(),
                line: 0,
                message,
            },
            other => Self::Runtime {
                chunk: "<chunk>".into(),
                line: 0,
                message: other.to_string(),
            },
        }
    }
}
