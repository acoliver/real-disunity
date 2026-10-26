use crate::{Error, Game, checkout_path};
use std::{
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    process::Command,
};

pub(super) fn checkout(workspace: &Path, game: Game) -> Result<PathBuf, Error> {
    let path = checkout_path(workspace, game);
    let parent = path.parent().expect("cache directory");
    fs::create_dir_all(parent)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(parent.join(format!("{}.lock", game.name())))?;
    lock.lock()?;
    // A directory without its own repository is build-cache residue, never an original:
    // git would resolve it through an enclosing repository instead.
    if path.exists() && !is_checkout_root(&path)? {
        fs::remove_dir_all(&path)?;
    }
    if !path.exists() {
        let staging = tempfile::Builder::new()
            .prefix(game.name())
            .tempdir_in(parent)?;
        git(staging.path(), &["init", "--quiet"])?;
        git(staging.path(), &["remote", "add", "origin", game.url()])?;
        git(
            staging.path(),
            &["fetch", "--depth", "1", "origin", game.rev()],
        )?;
        git(
            staging.path(),
            &["checkout", "--quiet", "--detach", "FETCH_HEAD"],
        )?;
        verify(staging.path(), game.rev())?;
        fs::rename(staging.path(), &path)?;
    }
    verify(&path, game.rev())?;
    Ok(path)
}

/// Whether `path` is the top level of its own git work tree.
pub(super) fn is_checkout_root(path: &Path) -> Result<bool, Error> {
    let Ok(top) = git(path, &["rev-parse", "--show-toplevel"]) else {
        return Ok(false);
    };
    Ok(Path::new(top.trim()).canonicalize()? == path.canonicalize()?)
}

pub(super) fn verify(path: &Path, expected: &str) -> Result<(), Error> {
    if !is_checkout_root(path)? {
        return Err(Error::NotCheckout(path.into()));
    }
    let head = git(path, &["rev-parse", "HEAD"])?;
    if head.trim() != expected {
        return Err(Error::Revision {
            path: path.into(),
            actual: head.trim().into(),
            expected: expected.into(),
        });
    }
    if !git(path, &["status", "--porcelain", "--untracked-files=all"])?.is_empty() {
        return Err(Error::Modified(path.into()));
    }
    Ok(())
}

pub(super) fn git(path: &Path, args: &[&str]) -> Result<String, Error> {
    let output = Command::new("git")
        .arg("--no-optional-locks")
        .args(args)
        .current_dir(path)
        .output()?;
    if !output.status.success() {
        return Err(Error::Git {
            operation: args.join(" "),
            path: path.into(),
            message: String::from_utf8_lossy(&output.stderr).into(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into())
}
