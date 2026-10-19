use std::path::{Component, Path, PathBuf};

use syn::visit_mut::{self, VisitMut};

pub struct Source {
    pub relative: String,
    pub production: syn::File,
}

pub fn contained(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(format!(
            "policy path must be workspace-relative without traversal: {relative}"
        ));
    }
    let absolute = crate::metadata::canonical(&root.join(path))?;
    if !absolute.starts_with(crate::metadata::canonical(root)?) {
        return Err(format!("policy path escapes workspace: {relative}"));
    }
    Ok(absolute)
}

pub fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries =
        std::fs::read_dir(dir).map_err(|e| format!("cannot enumerate {}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_symlink() {
            return Err(format!(
                "symlink in source scan is not supported: {}",
                path.display()
            ));
        }
        if kind.is_dir() {
            rust_files(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

pub fn load(root: &Path, paths: Vec<PathBuf>) -> Result<Vec<Source>, String> {
    paths
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(&path)
                .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
            let syntax = syn::parse_file(&text)
                .map_err(|e| format!("cannot parse {}: {e}", path.display()))?;
            let relative = path
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            let mut production = syntax.clone();
            Production.visit_file_mut(&mut production);
            Ok(Source {
                relative,
                production,
            })
        })
        .collect()
}

fn test_only(item: &syn::Item) -> bool {
    let attrs = match item {
        syn::Item::Mod(item) => &item.attrs,
        syn::Item::Fn(item) => &item.attrs,
        syn::Item::Struct(item) => &item.attrs,
        syn::Item::Enum(item) => &item.attrs,
        syn::Item::Impl(item) => &item.attrs,
        _ => return false,
    };
    attrs.iter().any(|a| {
        a.path().is_ident("cfg")
            && a.parse_args::<syn::Path>()
                .is_ok_and(|p| p.is_ident("test"))
    })
}

struct Production;
impl VisitMut for Production {
    fn visit_file_mut(&mut self, file: &mut syn::File) {
        file.items.retain(|item| !test_only(item));
        visit_mut::visit_file_mut(self, file);
    }
    fn visit_item_mod_mut(&mut self, module: &mut syn::ItemMod) {
        if let Some((_, items)) = &mut module.content {
            items.retain(|item| !test_only(item));
        }
        visit_mut::visit_item_mod_mut(self, module);
    }
}
