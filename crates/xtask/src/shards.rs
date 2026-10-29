//! Splits the `ci-gpu` test run across parallel runners.
//!
//! `ci-gpu` runs `cargo test --all-targets -- --include-ignored --test-threads=1` over the GPU
//! package selection. A shard reproduces the same set of tests in the same single cargo
//! invocation shape (feature unification stays identical) and runs one slice of it. The slices
//! are computed from the listed test names, so the union over all shards is exactly the test set
//! that the serial run executes.

use std::collections::BTreeSet;
use std::path::Path;

use crate::commands::{self, Profile};
use crate::metadata::{self, Metadata};

const MAX_SHARDS: usize = 256;
const SECTIONS: [(&str, Kind); 5] = [
    ("lib", Kind::Lib),
    ("bin", Kind::Bin),
    ("test", Kind::Test),
    ("bench", Kind::Bench),
    ("example", Kind::Example),
];
const LIST_ARGS: [&str; 4] = ["--list", "--format", "terse", "--include-ignored"];
const RUN_ARGS: [&str; 2] = ["--include-ignored", "--test-threads=1"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Lib,
    Bin,
    Test,
    Bench,
    Example,
}

impl Kind {
    fn flag(self) -> &'static str {
        match self {
            Self::Lib => "--lib",
            Self::Bin => "--bin",
            Self::Test => "--test",
            Self::Bench => "--bench",
            Self::Example => "--example",
        }
    }

    /// Maps cargo metadata `kind` values. `None` marks build scripts, which `cargo test` never
    /// runs as tests.
    fn from_cargo(kinds: &[String]) -> Result<Option<Self>, String> {
        let has = |name: &str| kinds.iter().any(|kind| kind == name);
        if has("custom-build") {
            return Ok(None);
        }
        for (name, kind) in [
            ("bin", Self::Bin),
            ("test", Self::Test),
            ("bench", Self::Bench),
            ("example", Self::Example),
        ] {
            if has(name) {
                return Ok(Some(kind));
            }
        }
        let library = ["lib", "rlib", "dylib", "cdylib", "staticlib", "proc-macro"];
        if kinds.iter().any(|kind| library.contains(&kind.as_str())) {
            return Ok(Some(Self::Lib));
        }
        Err(format!("unrecognised cargo target kind {kinds:?}"))
    }
}

/// A named cargo target other than the library, or the library itself under [`Kind::Lib`].
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Unit {
    pub kind: Kind,
    pub name: String,
}

impl Unit {
    fn flags(&self) -> [String; 2] {
        [self.kind.flag().to_owned(), self.name.clone()]
    }
}

/// Targets of the selected packages, split by how `cargo test` runs them.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Targets {
    has_lib: bool,
    libtest: BTreeSet<Unit>,
    whole: BTreeSet<Unit>,
}

impl Targets {
    /// Target flags that select every libtest binary in one cargo invocation.
    pub fn libtest_flags(&self) -> Vec<String> {
        let mut flags = Vec::new();
        if self.has_lib {
            flags.push(Kind::Lib.flag().to_owned());
        }
        for unit in &self.libtest {
            flags.extend(unit.flags());
        }
        flags
    }

    pub fn whole(&self) -> &BTreeSet<Unit> {
        &self.whole
    }
}

/// Names of `harness = false` targets declared in a manifest. These binaries own their `main`,
/// so they cannot be listed or split by name and run as whole units.
pub fn harness_free(manifest: &str) -> Result<BTreeSet<Unit>, String> {
    let document: toml::Table = manifest
        .parse()
        .map_err(|error| format!("invalid manifest: {error}"))?;
    let mut found = BTreeSet::new();
    for (section, kind) in SECTIONS {
        let entries = match document.get(section) {
            None => Vec::new(),
            Some(toml::Value::Array(items)) => items.iter().collect(),
            Some(table @ toml::Value::Table(_)) => vec![table],
            Some(_) => {
                return Err(format!(
                    "manifest section `{section}` has an unexpected type"
                ));
            }
        };
        for entry in entries {
            if entry.get("harness").and_then(toml::Value::as_bool) != Some(false) {
                continue;
            }
            let name = entry.get("name").and_then(toml::Value::as_str);
            let Some(name) = name.filter(|_| kind != Kind::Lib) else {
                return Err(format!(
                    "a `{section}` target with harness = false and no distinct name cannot be sharded"
                ));
            };
            found.insert(Unit {
                kind,
                name: name.to_owned(),
            });
        }
    }
    Ok(found)
}

/// Every runnable target of one package, tagged with whether it uses the libtest harness.
pub fn classify(targets: &[metadata::Target], manifest: &str) -> Result<Vec<(Unit, bool)>, String> {
    let free = harness_free(manifest)?;
    let mut classified = Vec::new();
    for target in targets {
        let Some(kind) = Kind::from_cargo(&target.kind)? else {
            continue;
        };
        let unit = Unit {
            kind,
            name: target.name.clone(),
        };
        let harness = !free.contains(&unit);
        classified.push((unit, harness));
    }
    Ok(classified)
}

/// Merges per-package targets, failing closed when one target name is ambiguous.
pub fn merge(classified: Vec<(Unit, bool)>) -> Result<Targets, String> {
    let mut targets = Targets::default();
    for (unit, harness) in classified {
        match (harness, unit.kind) {
            (true, Kind::Lib) => targets.has_lib = true,
            (true, _) => {
                targets.libtest.insert(unit);
            }
            (false, _) => {
                targets.whole.insert(unit);
            }
        }
    }
    if let Some(unit) = targets.whole.intersection(&targets.libtest).next() {
        return Err(format!(
            "{} target `{}` has harness = false in one selected package and a libtest harness in another; one cargo invocation selects targets by name, so rename one of them",
            unit.kind.flag().trim_start_matches('-'),
            unit.name
        ));
    }
    if !targets.has_lib && targets.libtest.is_empty() {
        return Err("the selection has no libtest targets to shard".into());
    }
    Ok(targets)
}

/// Test names from `--list --format terse` output; ignored tests are listed like any other.
pub fn parse_names(listing: &str) -> BTreeSet<String> {
    listing
        .lines()
        .filter_map(|line| line.trim_end().strip_suffix(": test"))
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Assignment {
    pub units: Vec<Unit>,
    pub names: Vec<String>,
}

fn parse_number(text: &str, what: &str) -> Result<usize, String> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("{what} must be a positive integer, got `{text}`"));
    }
    let value: usize = text
        .parse()
        .map_err(|_| format!("{what} `{text}` is too large"))?;
    if value == 0 || value > MAX_SHARDS {
        return Err(format!(
            "{what} must be between 1 and {MAX_SHARDS}, got {value}"
        ));
    }
    Ok(value)
}

pub fn parse_count(text: &str) -> Result<usize, String> {
    parse_number(text, "shard count")
}

/// Parses `K/N` into a 1-based index and a count.
pub fn parse_shard(text: &str) -> Result<(usize, usize), String> {
    let (index, count) = text
        .split_once('/')
        .ok_or_else(|| format!("shard must look like K/N, got `{text}`"))?;
    let count = parse_count(count)?;
    let index = parse_number(index, "shard index")?;
    if index > count {
        return Err(format!("shard index {index} exceeds shard count {count}"));
    }
    Ok((index, count))
}

/// Deals the sorted whole units and the sorted test names round robin: item `i` goes to shard
/// `i % count`. The result has one entry per shard.
pub fn partition(
    units: &BTreeSet<Unit>,
    names: &BTreeSet<String>,
    count: usize,
) -> Result<Vec<Assignment>, String> {
    if count == 0 {
        return Err("shard count must be at least 1".into());
    }
    let mut shards: Vec<Assignment> = (0..count).map(|_| Assignment::default()).collect();
    for (position, unit) in units.iter().enumerate() {
        shards[position % count].units.push(unit.clone());
    }
    for (position, name) in names.iter().enumerate() {
        shards[position % count].names.push(name.clone());
    }
    Ok(shards)
}

/// Proves that the shards cover the input exactly once.
pub fn verify(
    shards: &[Assignment],
    units: &BTreeSet<Unit>,
    names: &BTreeSet<String>,
) -> Result<(), String> {
    let mut seen_units = BTreeSet::new();
    let mut seen_names = BTreeSet::new();
    for shard in shards {
        for unit in &shard.units {
            if !seen_units.insert(unit.clone()) {
                return Err(format!(
                    "unit {} {} is in two shards",
                    unit.kind.flag(),
                    unit.name
                ));
            }
        }
        for name in &shard.names {
            if !seen_names.insert(name.clone()) {
                return Err(format!("test {name} is in two shards"));
            }
        }
    }
    if &seen_units != units || &seen_names != names {
        return Err("the shards do not cover exactly the listed tests".into());
    }
    Ok(())
}

/// `cargo test` arguments: selection and target flags, then the arguments for the test binaries.
pub fn test_args(selection: &[String], target_flags: &[String], harness: &[String]) -> Vec<String> {
    let mut args: Vec<String> = ["test", "--locked", "--jobs", "2"]
        .map(str::to_owned)
        .into();
    args.extend(selection.iter().cloned());
    args.extend(target_flags.iter().cloned());
    args.push("--".into());
    args.extend(harness.iter().cloned());
    args
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

fn selected_packages(selection: &[String]) -> Vec<&str> {
    selection
        .windows(2)
        .filter(|pair| pair[0] == "--package")
        .map(|pair| pair[1].as_str())
        .collect()
}

fn read_targets(data: &Metadata, selection: &[String]) -> Result<Targets, String> {
    let mut classified = Vec::new();
    for name in selected_packages(selection) {
        let package = data
            .packages
            .iter()
            .find(|package| package.name == name && data.workspace_members.contains(&package.id))
            .ok_or_else(|| format!("selected package {name} is not a workspace member"))?;
        let manifest = std::fs::read_to_string(&package.manifest_path)
            .map_err(|e| format!("cannot read {}: {e}", package.manifest_path.display()))?;
        classified.extend(
            classify(&package.targets, &manifest).map_err(|e| format!("package {name}: {e}"))?,
        );
    }
    merge(classified)
}

struct Inventory {
    selection: Vec<String>,
    targets: Targets,
    names: BTreeSet<String>,
}

fn inventory(root: &Path) -> Result<Inventory, String> {
    let data = metadata::read(root, &["--locked", "--all-features"])?;
    let selection = commands::selection(&data, Profile::Gpu)?;
    let targets = read_targets(&data, &selection)?;
    let args = test_args(&selection, &targets.libtest_flags(), &strings(&LIST_ARGS));
    let listing = crate::capture_command(root, "cargo", &refs(&args))?;
    let names = parse_names(&listing);
    if names.is_empty() {
        return Err("listing found no tests; refusing an empty GPU run".into());
    }
    Ok(Inventory {
        selection,
        targets,
        names,
    })
}

fn refs(args: &[String]) -> Vec<&str> {
    args.iter().map(String::as_str).collect()
}

fn plan_for(inventory: &Inventory, count: usize) -> Result<Vec<Assignment>, String> {
    let shards = partition(inventory.targets.whole(), &inventory.names, count)?;
    verify(&shards, inventory.targets.whole(), &inventory.names)?;
    Ok(shards)
}

fn describe(inventory: &Inventory, shards: &[Assignment]) {
    println!(
        "{} tests and {} whole-binary units across {} shards",
        inventory.names.len(),
        inventory.targets.whole().len(),
        shards.len()
    );
    for (position, shard) in shards.iter().enumerate() {
        let units: Vec<_> = shard
            .units
            .iter()
            .map(|unit| format!("{} {}", unit.kind.flag(), unit.name))
            .collect();
        println!(
            "shard {}/{}: {} tests, {} units {}",
            position + 1,
            shards.len(),
            shard.names.len(),
            shard.units.len(),
            units.join(", ")
        );
    }
}

/// `cargo xtask ci-gpu-plan N`: lists and verifies the partition without running any test.
pub fn plan(root: &Path, count: &str) -> Result<(), String> {
    let count = parse_count(count)?;
    let inventory = inventory(root)?;
    let shards = plan_for(&inventory, count)?;
    describe(&inventory, &shards);
    Ok(())
}

/// `cargo xtask ci-gpu-shard K/N`: runs shard K of N of the `ci-gpu` tests.
pub fn run(root: &Path, spec: &str) -> Result<(), String> {
    let (index, count) = parse_shard(spec)?;
    let inventory = inventory(root)?;
    let shards = plan_for(&inventory, count)?;
    let mine = &shards[index - 1];
    eprintln!(
        "GPU required mode: shard {index}/{count} runs {} of {} tests and {} of {} whole-binary units, including ignored tests; missing adapters are errors",
        mine.names.len(),
        inventory.names.len(),
        mine.units.len(),
        inventory.targets.whole().len()
    );
    if !mine.names.is_empty() {
        let mut harness = strings(&["--exact"]);
        harness.extend(strings(&RUN_ARGS));
        harness.extend(mine.names.iter().cloned());
        let args = test_args(
            &inventory.selection,
            &inventory.targets.libtest_flags(),
            &harness,
        );
        crate::run_command(root, "cargo", &refs(&args))?;
    }
    for unit in &mine.units {
        let args = test_args(&inventory.selection, &unit.flags(), &strings(&RUN_ARGS));
        crate::run_command(root, "cargo", &refs(&args))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
