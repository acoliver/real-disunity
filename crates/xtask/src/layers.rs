use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::metadata::{self, Metadata, Package};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Foundation,
    World,
    Bridge,
    Graphics,
    Facade,
    Host,
    Managed,
    Tool,
    Game,
}

impl Layer {
    pub fn cpu(self) -> bool {
        matches!(self, Self::Foundation | Self::World | Self::Bridge)
    }

    fn allows(self, other: Self) -> bool {
        use Layer::{Bridge, Facade, Foundation, Game, Graphics, Host, Managed, Tool, World};
        matches!(
            (self, other),
            (Foundation, Foundation)
                | (World, Foundation | World)
                | (Bridge, Foundation | World | Bridge)
                | (Graphics, Foundation | Bridge | Graphics)
                | (
                    Facade | Host,
                    Foundation | World | Bridge | Graphics | Host | Managed
                )
                | (Managed, Foundation | Managed)
                | (
                    Tool,
                    Foundation | World | Bridge | Graphics | Facade | Host | Managed | Tool
                )
                | (Game, Facade)
        )
    }
}

const PACKAGES: &[(&str, Layer)] = &[
    ("worldrunner-asset-id", Layer::Foundation),
    ("worldrunner-original-games", Layer::Foundation),
    ("worldrunner-core-types", Layer::Foundation),
    ("worldrunner-catalog", Layer::Foundation),
    ("worldrunner-package", Layer::Foundation),
    ("worldrunner-texture", Layer::Foundation),
    ("worldrunner-mesh", Layer::Foundation),
    ("worldrunner-material", Layer::Foundation),
    ("worldrunner-import-schema", Layer::Foundation),
    ("worldrunner-script", Layer::Foundation),
    ("worldrunner-camera", Layer::Foundation),
    ("worldrunner-disunity-ffi", Layer::Foundation),
    ("worldrunner-world-schema", Layer::World),
    ("worldrunner-world-model", Layer::World),
    ("worldrunner-world-runtime", Layer::World),
    ("worldrunner-world-generation", Layer::World),
    ("worldrunner-save", Layer::World),
    ("worldrunner-render-plan", Layer::Bridge),
    ("worldrunner-render-gpu", Layer::Graphics),
    ("worldrunner-render-scene", Layer::Graphics),
    ("worldrunner-render-shadow", Layer::Graphics),
    ("worldrunner-render-lighting", Layer::Graphics),
    ("worldrunner-render-post", Layer::Graphics),
    ("worldrunner-scene-viewer", Layer::Host),
    ("worldrunner-disunity-runtime", Layer::Managed),
    ("worldrunner-content-tool", Layer::Tool),
    ("worldrunner-compat-content", Layer::World),
    ("compat-kit", Layer::Tool),
    ("compat-player", Layer::Tool),
    ("xtask", Layer::Tool),
    ("worldrunner", Layer::Facade),
];

pub fn classify(root: &Path, package: &Package) -> Result<Layer, String> {
    let manifest = metadata::canonical(&package.manifest_path)?;
    let relative = manifest.strip_prefix(root).map_err(|_| {
        format!(
            "workspace package {} is outside {}",
            package.name,
            root.display()
        )
    })?;
    if relative.starts_with("games") {
        return Ok(Layer::Game);
    }
    PACKAGES
        .iter()
        .find_map(|(name, layer)| (*name == package.name).then_some(*layer))
        .ok_or_else(|| {
            format!(
                "unclassified workspace package `{}`; assign an explicit layer",
                package.name
            )
        })
}

pub fn workspace_layers(data: &Metadata) -> Result<BTreeMap<String, Layer>, String> {
    let root = metadata::canonical(&data.workspace_root)?;
    data.packages
        .iter()
        .filter(|p| data.workspace_members.contains(&p.id))
        .map(|p| classify(&root, p).map(|layer| (p.name.clone(), layer)))
        .collect()
}

pub fn check(data: &Metadata) -> Result<(), String> {
    let root = metadata::canonical(&data.workspace_root)?;
    let layers = workspace_layers(data)?;
    let mut errors = Vec::new();
    for package in &data.packages {
        check_paths(
            &root,
            package,
            layers.get(&package.name).copied(),
            &mut errors,
        )?;
        if let Some(layer) = layers.get(&package.name)
            && data.workspace_members.contains(&package.id)
        {
            check_edges(package, *layer, &layers, &mut errors);
        }
    }
    let roots: Vec<_> = data
        .packages
        .iter()
        .filter(|p| {
            data.workspace_members.contains(&p.id)
                && layers.get(&p.name).is_some_and(|layer| layer.cpu())
        })
        .collect();
    errors.extend(cpu_graph_errors(data, &roots)?);
    crate::finish("dependency/layer", &errors)
}

fn check_paths(
    root: &Path,
    package: &Package,
    layer: Option<Layer>,
    errors: &mut Vec<String>,
) -> Result<(), String> {
    for dependency in &package.dependencies {
        if let Some(path) = &dependency.path {
            let path = metadata::canonical(path)?;
            let shell = path.strip_prefix(root).is_ok_and(|relative| {
                relative.starts_with("games") || relative.starts_with("tools")
            });
            if shell && layer.is_some_and(|layer| !matches!(layer, Layer::Game | Layer::Tool)) {
                errors.push(format!(
                    "engine package {} must not depend on games/tools path {}",
                    package.name,
                    path.display()
                ));
            }
            if !path.starts_with(root) {
                errors.push(format!(
                    "{} -> {}: local dependency escapes workspace: {}",
                    package.name,
                    dependency.name,
                    path.display()
                ));
            }
        }
    }
    Ok(())
}

fn check_edges(
    package: &Package,
    layer: Layer,
    layers: &BTreeMap<String, Layer>,
    errors: &mut Vec<String>,
) {
    for dependency in &package.dependencies {
        if let Some(other) = layers.get(&dependency.name).copied().or_else(|| {
            PACKAGES
                .iter()
                .find_map(|(name, layer)| (*name == dependency.name).then_some(*layer))
        }) && !layer.allows(other)
        {
            errors.push(format!(
                "layer violation: {} ({layer:?}) -> {} ({other:?})",
                package.name, dependency.name
            ));
        }
        if layer.cpu() && forbidden(&dependency.name) {
            errors.push(format!(
                "CPU package {} declares GPU/CLR dependency {}",
                package.name, dependency.name
            ));
        }
    }
}

fn forbidden(name: &str) -> bool {
    [
        "wgpu",
        "winit",
        "naga",
        "glow",
        "ash",
        "vulkano",
        "glium",
        "gfx-hal",
        "netcorehost",
        "nethost",
        "hostfxr",
        "coreclr",
    ]
    .iter()
    .any(|prefix| name == *prefix || name.starts_with(&format!("{prefix}-")))
        || matches!(
            name,
            "worldrunner-render-gpu"
                | "worldrunner-render-scene"
                | "worldrunner-render-shadow"
                | "worldrunner-render-lighting"
                | "worldrunner-render-post"
                | "worldrunner-scene-viewer"
                | "worldrunner-disunity-runtime"
        )
}

pub fn cpu_graph_errors(data: &Metadata, roots: &[&Package]) -> Result<Vec<String>, String> {
    let packages: BTreeMap<_, _> = data.packages.iter().map(|p| (p.id.as_str(), p)).collect();
    let nodes: BTreeMap<_, _> = data
        .resolve
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n))
        .collect();
    let mut errors = Vec::new();
    for root in roots {
        let mut pending = vec![(root.id.as_str(), root.name.clone())];
        let mut seen = BTreeSet::new();
        while let Some((id, chain)) = pending.pop() {
            if !seen.insert(id) {
                continue;
            }
            let package = packages
                .get(id)
                .ok_or_else(|| format!("metadata graph references missing package {id}"))?;
            if forbidden(&package.name) {
                errors.push(format!("CPU graph reaches GPU/CLR: {chain}"));
            }
            let node = nodes
                .get(id)
                .ok_or_else(|| format!("metadata graph lacks node {id}"))?;
            for edge in &node.deps {
                let next = packages.get(edge.pkg.as_str()).ok_or_else(|| {
                    format!("metadata graph references missing package {}", edge.pkg)
                })?;
                pending.push((edge.pkg.as_str(), format!("{chain} -> {}", next.name)));
            }
        }
    }
    Ok(errors)
}

pub fn check_facade(root: &Path) -> Result<(), String> {
    let graph = metadata::facade_graph(root)?;
    let facade: Vec<_> = graph
        .packages
        .iter()
        .filter(|p| p.name == "worldrunner")
        .collect();
    if facade.len() != 1 {
        return Err("facade probe must resolve exactly one worldrunner package".into());
    }
    crate::finish("facade no-default CPU", &cpu_graph_errors(&graph, &facade)?)
}

#[cfg(test)]
mod tests;
