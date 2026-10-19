use std::path::{Path, PathBuf};

const WARN: usize = 750;
const HARD: usize = 1000;

fn visit(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries =
        std::fs::read_dir(dir).map_err(|e| format!("cannot enumerate {}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("cannot enumerate {}: {e}", dir.display()))?;
        let path = entry.path();
        let kind = entry
            .file_type()
            .map_err(|e| format!("cannot inspect {}: {e}", path.display()))?;
        if kind.is_symlink() {
            return Err(format!("symlink in source-size scan: {}", path.display()));
        }
        if kind.is_dir() {
            visit(&path, files)?;
        } else if kind.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
    Ok(())
}

pub fn check(root: &Path) -> Result<(), String> {
    let crates = root.join("crates");
    if !crates.is_dir() {
        return Err(format!("missing source scan root {}", crates.display()));
    }
    let mut files = Vec::new();
    visit(&crates, &mut files)?;
    if files.is_empty() {
        return Err("source-size found no Rust sources; scan is incomplete".into());
    }
    files.sort();
    let mut hard = Vec::new();
    for file in files {
        let content = std::fs::read_to_string(&file)
            .map_err(|e| format!("cannot read {}: {e}", file.display()))?;
        let count = content.lines().count();
        let path = file.strip_prefix(root).unwrap_or(&file).display();
        if count > HARD {
            hard.push(format!("{path}: {count} lines (maximum {HARD})"));
        } else if count > WARN {
            eprintln!("warning: {path}: {count} lines (recommended maximum {WARN})");
        }
    }
    if hard.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "source-size hard limit exceeded:\n{}",
            hard.join("\n")
        ))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn line_count_includes_an_unterminated_final_line() {
        let count = |value: &str| value.lines().count();
        assert_eq!(count(""), 0);
        assert_eq!(count("a\n"), 1);
        assert_eq!(count("a\nb"), 2);
    }
}

#[cfg(test)]
mod fixture_tests {
    use super::*;
    use crate::scratch::Directory;

    #[test]
    fn hard_limit_counts_unterminated_lines_and_empty_scans_fail() {
        let dir = Directory::new("source-size-test").unwrap();
        dir.write("crates/sample/src/lib.rs", &vec!["line"; HARD].join("\n"))
            .unwrap();
        check(dir.path()).unwrap();
        dir.write(
            "crates/sample/src/lib.rs",
            &vec!["line"; HARD + 1].join("\n"),
        )
        .unwrap();
        assert!(check(dir.path()).unwrap_err().contains("1001 lines"));
        let empty = Directory::new("source-size-empty").unwrap();
        std::fs::create_dir(empty.path().join("crates")).unwrap();
        assert!(check(empty.path()).unwrap_err().contains("no Rust sources"));
    }
}
