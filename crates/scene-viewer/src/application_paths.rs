use std::ffi::OsString;
use std::fmt::Write;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ApplicationPathError {
    #[error("application identifier must contain 1..=64 UTF-8 bytes")]
    InvalidIdentifier,
    #[error("application directory environment variable {0} is missing or empty")]
    MissingEnvironment(&'static str),
    #[error("platform {0} has no application directory convention")]
    UnsupportedPlatform(String),
}

/// Select a path without creating it. Identifiers are case-sensitive UTF-8 strings.
/// Hex encoding keeps distinct identifiers separate even on case-insensitive volumes.
pub fn application_root(identifier: &str) -> Result<PathBuf, ApplicationPathError> {
    application_root_with(identifier, std::env::consts::OS, |name| {
        std::env::var_os(name)
    })
}

fn application_root_with(
    identifier: &str,
    platform: &str,
    environment: impl Fn(&str) -> Option<OsString>,
) -> Result<PathBuf, ApplicationPathError> {
    if identifier.is_empty() || identifier.len() > 64 {
        return Err(ApplicationPathError::InvalidIdentifier);
    }
    let required = |name| {
        environment(name)
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .ok_or(ApplicationPathError::MissingEnvironment(name))
    };
    let base = match platform {
        "macos" => required("HOME")?.join("Library/Application Support"),
        "linux" => match environment("XDG_DATA_HOME").filter(|s| !s.is_empty()) {
            Some(path) => PathBuf::from(path),
            None => required("HOME")?.join(".local/share"),
        },
        "windows" => required("APPDATA")?,
        other => return Err(ApplicationPathError::UnsupportedPlatform(other.to_owned())),
    };
    let mut name = String::from("app-");
    for byte in identifier.bytes() {
        write!(name, "{byte:02x}").expect("writing to String cannot fail");
    }
    Ok(base.join(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    #[test]
    fn distinct_applications_have_distinct_roots_on_every_platform() {
        for platform in ["macos", "linux", "windows"] {
            let env = |_: &str| Some(OsString::from("/synthetic-home"));
            let first = application_root_with("App", platform, env).unwrap();
            let second = application_root_with("app", platform, env).unwrap();
            assert_ne!(first, second);
            assert_ne!(
                first.to_string_lossy().to_lowercase(),
                second.to_string_lossy().to_lowercase()
            );
        }
    }

    #[test]
    fn path_layouts_and_missing_environment_are_explicit() {
        let env = |key: &str| match key {
            "HOME" => Some(OsString::from("/home/test")),
            "APPDATA" => Some(OsString::from("C:\\data")),
            _ => None,
        };
        assert_eq!(
            application_root_with("a", "macos", env).unwrap(),
            PathBuf::from("/home/test/Library/Application Support/app-61")
        );
        assert_eq!(
            application_root_with("a", "linux", env).unwrap(),
            PathBuf::from("/home/test/.local/share/app-61")
        );
        assert_eq!(
            application_root_with("a", "windows", env).unwrap(),
            PathBuf::from("C:\\data").join("app-61")
        );
        assert!(application_root_with("", "linux", env).is_err());
        assert!(application_root_with("a", "windows", |_| None).is_err());
        assert!(application_root_with("a", "other", env).is_err());
    }
}
