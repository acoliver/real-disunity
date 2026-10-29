use std::path::Path;

use crate::{layers, metadata};

mod config;
mod coverage;
mod environment;
mod literals;
mod source;
pub use source::rust_files;
mod type_defs;
mod types;
mod use_tree;

pub fn check(root: &Path) -> Result<(), String> {
    let data = metadata::read(root, &["--locked", "--all-features"])?;
    layers::check(&data)?;
    layers::check_facade(root)?;
    check_sources(root, &data)?;
    crate::csharp::check(&data)?;
    println!(
        "policy: explicit layers, transitive CPU/no-default facade graphs, registered types, literal ownership, consumer index, CPU environment lookups, and C# consumer coverage checked"
    );
    Ok(())
}

fn check_sources(root: &Path, data: &metadata::Metadata) -> Result<(), String> {
    let root = metadata::canonical(root)?;
    let layers = layers::workspace_layers(data)?;
    let mut paths = Vec::new();
    let mut cpu_roots = Vec::new();
    for package in data
        .packages
        .iter()
        .filter(|p| data.workspace_members.contains(&p.id))
    {
        let dir = metadata::canonical(
            &package
                .manifest_path
                .parent()
                .ok_or("package manifest has no parent")?
                .join("src"),
        )?;
        let start = paths.len();
        source::rust_files(&dir, &mut paths)?;
        if paths.len() == start {
            return Err(format!("no Rust sources in {}", dir.display()));
        }
        if layers.get(&package.name).is_some_and(|layer| layer.cpu()) {
            cpu_roots.push(dir);
        }
    }
    let files = source::load(&root, paths)?;
    let mut errors = Vec::new();
    for file in &files {
        if cpu_roots
            .iter()
            .any(|dir| root.join(&file.relative).starts_with(dir))
        {
            environment::check(&file.production, &file.relative, &mut errors);
        }
    }
    check_explicit(&root, &files, &mut errors)?;
    crate::finish("explicit policy", &errors)
}

fn check_explicit(
    root: &Path,
    files: &[source::Source],
    errors: &mut Vec<String>,
) -> Result<(), String> {
    let config = config::read(root)?;
    types::check(&config, files, errors);
    literals::check(&config, files, errors);
    coverage::check(root, &config, files, errors);
    Ok(())
}

#[cfg(test)]
mod tests;
