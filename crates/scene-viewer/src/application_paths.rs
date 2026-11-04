use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ApplicationPathError {
    #[error("application identifier must be a valid lowercase ASCII name (see application_root)")]
    InvalidIdentifier,
    #[error("application directory environment variable {0} is missing or empty")]
    MissingEnvironment(&'static str),
    #[error("platform {0} has no application directory convention")]
    UnsupportedPlatform(String),
}

/// Select a path without creating it, using the identifier as the directory name.
/// Identifiers contain 1..=64 bytes of lowercase ASCII letters, digits, '.', '-' or '_',
/// start with a letter or digit, contain no ".." and do not end in '.'.
/// Lowercase-only names keep distinct identifiers separate on case-insensitive volumes.
/// Invalid identifiers return [`ApplicationPathError::InvalidIdentifier`].
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
    if identifier.len() > 64
        || !identifier
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        || !identifier.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b'_')
        })
        || identifier.contains("..")
        || identifier.ends_with('.')
    {
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
    Ok(base.join(identifier))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    #[test]
    fn invalid_identifiers_are_rejected() {
        let too_long = "a".repeat(65);
        for identifier in [
            "", &too_long, "App", "a/b", "a\\b", "..", "a..b", ".app", "app.", "a b", "café",
            "-app", "_app", "a:b",
        ] {
            assert!(
                matches!(
                    application_root_with(identifier, "linux", |_| None),
                    Err(ApplicationPathError::InvalidIdentifier)
                ),
                "accepted invalid identifier {identifier:?}"
            );
        }
    }

    #[test]
    fn valid_identifiers_are_used_as_directory_names() {
        let longest = "a".repeat(64);
        for identifier in [
            "a",
            "0",
            "org.acoliver.chess3d",
            "a-b_c.0",
            "a-",
            "a_",
            &longest,
        ] {
            assert_eq!(
                application_root_with(identifier, "linux", |_| {
                    Some(OsString::from("/synthetic-base"))
                })
                .unwrap(),
                PathBuf::from("/synthetic-base").join(identifier)
            );
        }
    }

    #[test]
    fn distinct_applications_have_distinct_roots_on_every_platform() {
        for platform in ["macos", "linux", "windows"] {
            let env = |_: &str| Some(OsString::from("/synthetic-home"));
            let first = application_root_with("org.acoliver.chess3d", platform, env).unwrap();
            let second = application_root_with("org.acoliver.chess2d", platform, env).unwrap();
            assert_ne!(first, second);
            assert_ne!(
                first.to_string_lossy().to_lowercase(),
                second.to_string_lossy().to_lowercase()
            );
        }
    }

    #[test]
    fn path_layouts_are_explicit() {
        let env = |key: &str| match key {
            "HOME" => Some(OsString::from("/home/test")),
            "APPDATA" => Some(OsString::from("C:\\data")),
            _ => None,
        };
        assert_eq!(
            application_root_with("org.acoliver.chess3d", "macos", env).unwrap(),
            PathBuf::from("/home/test/Library/Application Support/org.acoliver.chess3d")
        );
        assert_eq!(
            application_root_with("org.acoliver.chess3d", "linux", env).unwrap(),
            PathBuf::from("/home/test/.local/share/org.acoliver.chess3d")
        );
        assert_eq!(
            application_root_with("org.acoliver.chess3d", "windows", env).unwrap(),
            PathBuf::from("C:\\data").join("org.acoliver.chess3d")
        );
    }

    #[test]
    fn linux_prefers_nonempty_xdg_data_home() {
        for (xdg, expected) in [
            (Some("/data"), "/data/a"),
            (Some(""), "/home/test/.local/share/a"),
            (None, "/home/test/.local/share/a"),
        ] {
            let env = |key: &str| match key {
                "HOME" => Some(OsString::from("/home/test")),
                "XDG_DATA_HOME" => xdg.map(OsString::from),
                _ => None,
            };
            assert_eq!(
                application_root_with("a", "linux", env).unwrap(),
                PathBuf::from(expected)
            );
        }
    }

    #[test]
    fn missing_environment_and_unsupported_platform_fail_explicitly() {
        for (platform, variable) in [("macos", "HOME"), ("linux", "HOME"), ("windows", "APPDATA")] {
            for value in [None, Some(OsString::new())] {
                assert!(matches!(
                    application_root_with("a", platform, |_| value.clone()),
                    Err(ApplicationPathError::MissingEnvironment(name)) if name == variable
                ));
            }
        }
        assert!(matches!(
            application_root_with("a", "other", |_| None),
            Err(ApplicationPathError::UnsupportedPlatform(platform)) if platform == "other"
        ));
    }
}
