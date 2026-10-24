use crate::{
    files::{self, Result, io},
    fixture,
    model::{CommandSpec, Fixture, RefreshSpec, SourceState},
    process::{self, Execution, RunControl},
};
use crate::{
    refresh_identity::{GitIdentity, LocalReference},
    refresh_integrity::ClosureCheck,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

struct Snapshot {
    state: SourceState,
    hashes: BTreeMap<String, String>,
    identity: GitIdentity,
    _lease: Option<std::fs::File>,
}
struct Candidate {
    original: Snapshot,
    upstream: Option<LocalReference>,
    _owned: crate::storage::OwnedDirectory,
    active: Snapshot,
    path: PathBuf,
    revision: String,
    identity: GitIdentity,
    hashes: BTreeMap<String, String>,
}
struct Session<'a> {
    root: &'a Path,
    fixture: &'a Fixture,
    control: &'a RunControl,
    output: &'a Path,
}

pub fn refresh(
    root: &Path,
    fixture: &Fixture,
    accept: bool,
    control: &RunControl,
) -> Result<Value> {
    files::check_budget(root)?;
    let directory = files::work_path(root, Path::new(".work/compat/locks"))?;
    fs::create_dir_all(&directory).map_err(io)?;
    let lock_path = directory.join(format!("{}.refresh.lock", fixture.name));
    let Some(_lock) = crate::lease::refresh(&lock_path)? else {
        return Ok(
            json!({"fixture":fixture.name,"status":"blocked","message":"fixture refresh already locked"}),
        );
    };
    let output = files::work_path(
        root,
        &PathBuf::from(format!(".work/compat/refresh/{}", files::unique_id())),
    )?;
    fs::create_dir_all(&output).map_err(io)?;
    let mut report = json!({"schema_version":1,"fixture":fixture.name,"status":"blocked","accepted":false,"output":output});
    let session = Session {
        root,
        fixture,
        control,
        output: &output,
    };
    if let Err(error) = session.qualify(accept, &directory, &mut report) {
        if report["status"] != "fail" {
            report["status"] = json!(if control.is_cancelled() {
                "skipped"
            } else {
                "blocked"
            });
        }
        report["failure_cause"] = json!({"kind":"infrastructure","stage":report["stage"].as_str().unwrap_or("refresh-configuration"),"message":error});
        report["message"] = json!(error);
    }
    files::write_json(&output.join("refresh.json"), &report)?;
    Ok(report)
}
fn manifest(source: &Path) -> Result<BTreeMap<String, String>> {
    files::source_files(source, 100_000_000_000, 1_000_000)?
        .into_iter()
        .map(|path| {
            Ok((
                path.strip_prefix(source)
                    .map_err(io)?
                    .to_string_lossy()
                    .into_owned(),
                files::hash(&path)?,
            ))
        })
        .collect()
}
fn original(root: &Path, fixture: &Fixture) -> PathBuf {
    if fixture.source_root.is_absolute() {
        fixture.source_root.clone()
    } else {
        root.join(&fixture.source_root)
    }
}
fn clean(path: &Path) -> Result<()> {
    if fixture::git(path, &["status", "--porcelain", "--untracked-files=all"])?.is_empty() {
        Ok(())
    } else {
        Err("dirty original or active source rejected; local edits were preserved".into())
    }
}
impl Session<'_> {
    fn config(&self) -> Result<&RefreshSpec> {
        let spec = self
            .fixture
            .refresh
            .as_ref()
            .ok_or("refresh upstream and qualification gates are not configured")?;
        if spec.gates.is_empty() {
            return Err("explicit compile/test qualification gates are required".into());
        }
        if spec.remote.starts_with('-') || spec.branch.starts_with('-') || spec.branch.is_empty() {
            return Err("invalid refresh remote or branch".into());
        }
        Ok(spec)
    }
    fn qualify(&self, accept: bool, locks: &Path, report: &mut Value) -> Result<()> {
        self.config()?;
        let mut closure = ClosureCheck::new(self.root, self.fixture, self.output).map_err(|error| {
            report["integrity"] = json!({"status":"unsupported","engine_proof":false,"code":"integrity-policy-unavailable","message":error});
            error
        })?;
        report["integrity"] =
            json!({"status":"not-requested","engine_proof":false,"scope":"harness-gates-only"});
        let candidate = self.prepare()?;
        let before = candidate
            .active
            .state
            .revision
            .as_deref()
            .ok_or("source must have a git revision")?;
        report["before"] = serde_json::to_value(&candidate.active.state).map_err(io)?;
        report["candidate"] = json!(candidate.path);
        report["revision"] = json!(candidate.revision);
        report["diff"] = changes(&candidate.path, before, &candidate.revision)?;
        report["stage"] = json!("qualification-gates");
        self.gates(&candidate, report, &mut closure)?;
        report["stage"] = json!("pre-adoption");
        if let Err(error) = self.pre_adoption(&candidate, &mut closure, report) {
            report["status"] = json!("fail");
            return Err(error);
        }
        report["after"] = serde_json::to_value(fixture::inspect(&candidate.path)?).map_err(io)?;
        if self.control.is_cancelled() {
            return Err("refresh cancelled before adoption".into());
        }
        if accept {
            report["stage"] = json!("atomic-adoption");
            self.adopt(&candidate, locks, report, closure.as_ref())?;
        }
        report["status"] = json!("pass");
        report["message"] = json!(if accept {
            "qualified candidate adopted through atomic active-source lock"
        } else {
            "candidate qualified; active source unchanged"
        });
        Ok(())
    }
    fn pre_adoption(
        &self,
        candidate: &Candidate,
        closure: &mut Option<ClosureCheck<'_>>,
        report: &mut Value,
    ) -> Result<()> {
        self.verify(candidate)?;
        if let Some(check) = closure {
            report["integrity"] =
                json!({"status":"checking","mode":"strict-closure","engine_proof":false});
            report["integrity"] = check.check(&candidate.path, self.control)?;
        }
        self.verify(candidate)
    }
    fn adopt(
        &self,
        candidate: &Candidate,
        locks: &Path,
        report: &mut Value,
        closure: Option<&ClosureCheck<'_>>,
    ) -> Result<()> {
        files::write_json(
            &self.output.join("qualification.json"),
            &json!({"status":"pass","revision":candidate.revision,"source_hashes":candidate.hashes,"gates":report["gates"],"integrity":report["integrity"],"local_identity":candidate.identity,"local_upstream":candidate.upstream}),
        )?;
        let lock = json!({"schema_version":1,"fixture":self.fixture.name,"source_root":candidate.path,"revision":candidate.revision,"accepted_ms":files::now_ms(),"qualification":self.output.join("qualification.json"),"source_hashes":candidate.hashes,"integrity":report["integrity"],"local_identity":candidate.identity,"local_upstream":candidate.upstream});
        let bytes = serde_json::to_vec_pretty(&lock).map_err(io)?;
        self.verify(candidate)?;
        if let Some(check) = closure {
            check.verify_watched()?;
        }
        if self.control.is_cancelled() {
            return Err("refresh cancelled before adoption".into());
        }
        files::atomic_write(&locks.join(format!("{}.json", self.fixture.name)), &bytes)?;
        report["accepted"] = json!(true);
        Ok(())
    }
    fn prepare(&self) -> Result<Candidate> {
        let original_path = original(self.root, self.fixture);
        let original_lease = crate::storage::source_lease(self.root, &original_path)?;
        clean(&original_path)?;
        let original = Snapshot {
            state: fixture::inspect(&original_path)?,
            hashes: manifest(&original_path)?,
            identity: GitIdentity::read(&original_path)?,
            _lease: original_lease,
        };
        let active = fixture::source_root(self.root, self.fixture)?;
        let source_lease = crate::storage::source_lease(self.root, &active)?;
        let state = fixture::inspect(&active)?;
        if state.dirty != Some(false) {
            return Err("dirty or unversioned active source rejected".into());
        }
        let active = Snapshot {
            _lease: source_lease,
            state,
            hashes: manifest(&active)?,
            identity: GitIdentity::read(&active)?,
        };
        let owned = crate::storage::allocate(self.root, crate::storage::Kind::Candidate)?;
        let path = owned.path.clone();
        let upstream = LocalReference::observe(&self.remote()?, &self.config()?.branch, &path)?;
        self.fetch(&active.state.root, &path)?;
        Ok(Candidate {
            original,
            upstream,
            _owned: owned,
            active,
            revision: fixture::git(&path, &["rev-parse", "HEAD"])?,
            hashes: manifest(&path)?,
            identity: GitIdentity::read(&path)?,
            path,
        })
    }
    fn environment(&self, candidate: &Path) -> BTreeMap<String, String> {
        BTreeMap::from([
            ("GIT_TERMINAL_PROMPT".into(), "0".into()),
            (
                "COMPAT_ROOT".into(),
                self.root.to_string_lossy().into_owned(),
            ),
            (
                "COMPAT_SOURCE_ROOT".into(),
                candidate.to_string_lossy().into_owned(),
            ),
            (
                "COMPAT_GATE_OUTPUT".into(),
                self.output.to_string_lossy().into_owned(),
            ),
        ])
    }
    fn remote(&self) -> Result<String> {
        let spec = self.config()?;
        Ok(fixture::git(
            &original(self.root, self.fixture),
            &["remote", "get-url", &spec.remote],
        )
        .unwrap_or_else(|_| spec.remote.clone()))
    }
    fn fetch(&self, active: &Path, candidate: &Path) -> Result<()> {
        let spec = self.config()?;
        let steps = fetch_steps(active, candidate, self.remote()?, &spec.branch);
        let env = self.environment(candidate);
        for (index, args) in steps.into_iter().enumerate() {
            let logs = self.output.join(format!("git-{index}"));
            let options = Execution {
                cwd: self.root,
                env: &env,
                logs: &logs,
                artifacts: None,
                limits: &self.fixture.limits,
            };
            let outcome = process::execute(
                &CommandSpec {
                    program: "git".into(),
                    args,
                },
                &options,
                self.control,
            )?;
            if !outcome.success || outcome.interrupted.is_some() {
                return Err(format!(
                    "source update failed: {} {:?}",
                    outcome.exit, outcome.interrupted
                ));
            }
        }
        Ok(())
    }
    fn gates(
        &self,
        candidate: &Candidate,
        report: &mut Value,
        closure: &mut Option<ClosureCheck<'_>>,
    ) -> Result<()> {
        let environment = self.environment(&candidate.path);
        let mut gates = Vec::new();
        for (index, gate) in self.config()?.gates.iter().enumerate() {
            let command = files::expand_command(gate, self.root, &candidate.path, self.output);
            let logs = self.output.join(format!("gate-{index}"));
            let options = Execution {
                cwd: &candidate.path,
                env: &environment,
                logs: &logs,
                artifacts: Some(self.output),
                limits: &self.fixture.limits,
            };
            let outcome = process::execute(&command, &options, self.control)?;
            gates.push(json!({"command":gate,"exit":outcome.exit,"interrupted":outcome.interrupted,"truncated_logs":outcome.truncated_logs}));
            report["gates"] = json!(gates);
            if !outcome.success || outcome.interrupted.is_some() {
                report["status"] = json!("fail");
                return Err(
                    "qualification gate failed; previous active source and lock preserved".into(),
                );
            }
            if let Some(check) = closure
                && let Err(error) = check.observe(&candidate.path)
            {
                report["status"] = json!("fail");
                return Err(error);
            }
        }
        Ok(())
    }
    fn verify(&self, candidate: &Candidate) -> Result<()> {
        candidate.identity.verify(&candidate.path)?;
        if let Some(upstream) = &candidate.upstream {
            upstream.verify(&candidate.revision)?;
        }
        if manifest(&candidate.path)? != candidate.hashes
            || fixture::git(&candidate.path, &["rev-parse", "HEAD"])? != candidate.revision
        {
            return Err("qualification modified candidate source; adoption rejected".into());
        }
        verify_snapshot(&candidate.active)?;
        verify_snapshot(&candidate.original)?;
        clean(&original(self.root, self.fixture))
    }
}
fn verify_snapshot(snapshot: &Snapshot) -> Result<()> {
    snapshot.identity.verify(&snapshot.state.root)?;
    if manifest(&snapshot.state.root)? != snapshot.hashes
        || fixture::git(&snapshot.state.root, &["rev-parse", "HEAD"])?
            != snapshot
                .state
                .revision
                .as_deref()
                .ok_or("revision missing")?
    {
        return Err("source changed concurrently during qualification; adoption rejected".into());
    }
    Ok(())
}

fn is_script(path: &str) -> bool {
    Path::new(path).extension().is_some_and(|ext| ext == "cs")
}
fn changes(candidate: &Path, before: &str, after: &str) -> Result<Value> {
    let paths = fixture::git(candidate, &["diff", "--name-only", before, after, "--"])?;
    let diff = fixture::git(
        candidate,
        &["diff", "--unified=0", before, after, "--", "*.cs"],
    )?;
    let all: Vec<_> = paths.lines().collect();
    Ok(json!({"paths":all,
        "scripts":all.iter().filter(|p|is_script(p)).collect::<Vec<_>>(),
        "packages":all.iter().filter(|p|p.starts_with("Packages/")).collect::<Vec<_>>(),
        "editor":all.iter().filter(|p|p.starts_with("ProjectSettings/") || p.contains("/Editor/")).collect::<Vec<_>>(),
        "assets":all.iter().filter(|p|p.starts_with("Assets/") && !is_script(p)).collect::<Vec<_>>(),
        "api_lines":diff.lines().filter(|l|l.starts_with('+') && !l.starts_with("+++") && (l.contains("public ") || l.contains("using ") || l.contains("UnityEngine"))).take(2000).collect::<Vec<_>>()
    }))
}

fn fetch_steps(active: &Path, candidate: &Path, remote: String, branch: &str) -> [Vec<String>; 4] {
    let destination = candidate.to_string_lossy().into_owned();
    [
        vec![
            "clone".into(),
            "--no-hardlinks".into(),
            "--no-checkout".into(),
            "--".into(),
            active.to_string_lossy().into_owned(),
            destination.clone(),
        ],
        vec![
            "-C".into(),
            destination.clone(),
            "remote".into(),
            "set-url".into(),
            "origin".into(),
            remote,
        ],
        vec![
            "-C".into(),
            destination.clone(),
            "fetch".into(),
            "--no-tags".into(),
            "origin".into(),
            branch.into(),
        ],
        vec![
            "-C".into(),
            destination,
            "checkout".into(),
            "--detach".into(),
            "FETCH_HEAD".into(),
        ],
    ]
}
