use crate::{
    files::{self, Result, io},
    fixture,
};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct GitIdentity {
    head: String,
    refs: String,
    files: BTreeMap<String, Option<String>>,
}
impl GitIdentity {
    pub(crate) fn read(source: &Path) -> Result<Self> {
        if !fixture::git(source, &["status", "--porcelain", "--untracked-files=all"])?.is_empty() {
            return Err(format!(
                "dirty source before adoption: {}",
                source.display()
            ));
        }
        let mut files = BTreeMap::new();
        for name in ["HEAD", "FETCH_HEAD", "config", "packed-refs"] {
            let path = PathBuf::from(fixture::git(
                source,
                &["rev-parse", "--path-format=absolute", "--git-path", name],
            )?);
            files.insert(
                name.into(),
                match fs::symlink_metadata(&path) {
                    Ok(meta) if meta.is_file() => Some(files::hash(&path)?),
                    Ok(_) => {
                        return Err(format!("nonregular local Git identity: {}", path.display()));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                    Err(e) => return Err(io(e)),
                },
            );
        }
        Ok(Self {
            head: fixture::git(source, &["rev-parse", "--verify", "HEAD^{commit}"])?,
            refs: fixture::git(
                source,
                &["for-each-ref", "--format=%(refname) %(objectname)"],
            )?,
            files,
        })
    }
    pub(crate) fn verify(&self, source: &Path) -> Result<()> {
        if &Self::read(source)? != self {
            return Err(format!(
                "local source ref/revision identity changed before adoption: {}",
                source.display()
            ));
        }
        Ok(())
    }
}

#[derive(Serialize)]
pub(crate) struct LocalReference {
    source: PathBuf,
    requested: PathBuf,
    reference: String,
    revision: String,
}
impl LocalReference {
    pub(crate) fn observe(remote: &str, branch: &str, candidate: &Path) -> Result<Option<Self>> {
        let remote = remote.strip_prefix("file://").unwrap_or(remote);
        let path = candidate.join(remote);
        if !path.try_exists().map_err(io)? {
            return Ok(None);
        }
        let source = path.canonicalize().map_err(io)?;
        let reference = if branch.starts_with("refs/") {
            branch.into()
        } else {
            format!("refs/heads/{branch}")
        };
        let revision = fixture::git(
            &source,
            &["rev-parse", "--verify", &format!("{reference}^{{commit}}")],
        )?;
        Ok(Some(Self {
            source,
            requested: path,
            reference,
            revision,
        }))
    }
    pub(crate) fn verify(&self, candidate_revision: &str) -> Result<()> {
        let current = fixture::git(
            &self.source,
            &[
                "rev-parse",
                "--verify",
                &format!("{}^{{commit}}", self.reference),
            ],
        )?;
        if current != self.revision
            || candidate_revision != self.revision
            || self.requested.canonicalize().map_err(io)? != self.source
        {
            return Err(format!(
                "local upstream reference changed before adoption: {} {}",
                self.source.display(),
                self.reference
            ));
        }
        Ok(())
    }
}
