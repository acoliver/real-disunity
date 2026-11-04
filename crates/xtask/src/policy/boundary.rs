//! A package's adapter module is the only code that names the crate of a foreign format.
//!
//! `worldrunner-render-program` executes engine-owned descriptions; `src/unity/**` converts
//! Unity's serialized records into them. The dependency-layer check cannot see this boundary,
//! because Graphics may depend on Foundation. This check reads the package's source instead:
//! outside the adapter directory, no path, import (renamed or not), re-export, macro input or
//! `extern crate` may name the adapter's crate, and no module may name the adapter module, so a
//! root module can neither expose an adapter type nor re-export the adapter.

use std::path::Path;

use quote::ToTokens;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};

use super::source::rust_files;
use super::use_tree::use_leaves;
use crate::metadata::{self, Dependency, Metadata};

/// One package whose source is split into a neutral part and a format adapter.
pub struct Boundary {
    package: &'static str,
    /// The adapter's directory and module name, under the package's `src`.
    adapter: &'static str,
    /// The cargo feature that gates the adapter module.
    feature: &'static str,
    /// The format crate only the adapter may name.
    foreign_package: &'static str,
    foreign_crate: &'static str,
}

pub const BOUNDARIES: &[Boundary] = &[Boundary {
    package: "worldrunner-render-program",
    adapter: "unity",
    feature: "unity",
    foreign_package: "worldrunner-unity-serialized",
    foreign_crate: "worldrunner_unity_serialized",
}];

pub fn check(data: &Metadata) -> Result<(), String> {
    let root = metadata::canonical(&data.workspace_root)?;
    let mut errors = Vec::new();
    for boundary in BOUNDARIES {
        let Some(package) = data
            .packages
            .iter()
            .find(|p| p.name == boundary.package && data.workspace_members.contains(&p.id))
        else {
            continue;
        };
        dependency_errors(boundary, &package.dependencies, &mut errors);
        let src = metadata::canonical(
            &package
                .manifest_path
                .parent()
                .ok_or("package manifest has no parent")?
                .join("src"),
        )?;
        source_errors(boundary, &root, &src, &mut errors)?;
    }
    crate::finish("adapter boundary", &errors)
}

/// A renamed dependency would let source name the format crate under another identifier.
fn dependency_errors(boundary: &Boundary, dependencies: &[Dependency], errors: &mut Vec<String>) {
    for dependency in dependencies {
        if dependency.name == boundary.foreign_package && dependency.rename.is_some() {
            errors.push(format!(
                "{}: dependency {} is renamed, which hides it from the adapter boundary check",
                boundary.package, boundary.foreign_package
            ));
        }
    }
}

fn source_errors(
    boundary: &Boundary,
    root: &Path,
    src: &Path,
    errors: &mut Vec<String>,
) -> Result<(), String> {
    let mut files = Vec::new();
    rust_files(src, &mut files)?;
    files.sort();
    for path in files {
        let path = metadata::canonical(&path)?;
        let inside = path
            .strip_prefix(src)
            .map_err(|_| format!("{} is outside {}", path.display(), src.display()))?;
        if inside.starts_with(boundary.adapter) {
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let file =
            syn::parse_file(&text).map_err(|e| format!("cannot parse {}: {e}", path.display()))?;
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let crate_root = inside == Path::new("lib.rs");
        errors.extend(violations(boundary, &relative, crate_root, &file));
    }
    Ok(())
}

/// What a file outside the adapter does that crosses the boundary.
fn violations(
    boundary: &Boundary,
    relative: &str,
    crate_root: bool,
    file: &syn::File,
) -> Vec<String> {
    let mut visitor = Crossings {
        boundary,
        relative,
        crate_root,
        found: Vec::new(),
    };
    for leaf in use_leaves(file) {
        let at = leaf.line;
        visitor.path(&leaf.path, at);
    }
    visitor.visit_file(file);
    visitor.found
}

struct Crossings<'a> {
    boundary: &'a Boundary,
    relative: &'a str,
    crate_root: bool,
    found: Vec<String>,
}

impl Crossings<'_> {
    fn report(&mut self, line: usize, what: &str) {
        self.found.push(format!("{}:{line}: {what}", self.relative));
    }

    /// Reports a path that names the format crate or the adapter module.
    fn path(&mut self, segments: &[String], line: usize) {
        let joined = segments.join("::");
        if segments
            .iter()
            .any(|s| s.contains("unity_serialized") || *s == self.boundary.foreign_crate)
        {
            self.report(
                line,
                &format!(
                    "`{joined}` names the {} crate outside the adapter",
                    self.boundary.foreign_package
                ),
            );
        } else if self.names_adapter(segments) {
            self.report(
                line,
                &format!(
                    "`{joined}` names the `{}` adapter module outside it",
                    self.boundary.adapter
                ),
            );
        }
    }

    fn names_adapter(&self, segments: &[String]) -> bool {
        let adapter = self.boundary.adapter;
        match segments {
            [first, second, ..] if matches!(first.as_str(), "crate" | "self" | "super") => {
                second == adapter
            }
            [first, ..] => self.crate_root && first == adapter,
            [] => false,
        }
    }

    fn gated(&self, attrs: &[syn::Attribute]) -> bool {
        let wanted = format!("cfg(feature=\"{}\")", self.boundary.feature);
        attrs.iter().any(|attr| {
            attr.path().is_ident("cfg")
                && attr.meta.to_token_stream().to_string().replace(' ', "") == wanted
        })
    }

    /// Reports the paths written in macro input, which no parser has resolved.
    fn tokens(&mut self, stream: proc_macro2::TokenStream, line: usize) {
        let mut current: Vec<String> = Vec::new();
        let mut after_colon = false;
        for token in stream {
            match token {
                proc_macro2::TokenTree::Ident(ident) => {
                    if !after_colon {
                        self.flush(&mut current, line);
                    }
                    current.push(ident.to_string());
                    after_colon = false;
                }
                proc_macro2::TokenTree::Punct(punct) if punct.as_char() == ':' => {
                    after_colon = true;
                }
                proc_macro2::TokenTree::Group(group) => {
                    self.flush(&mut current, line);
                    self.tokens(group.stream(), line);
                    after_colon = false;
                }
                _ => {
                    self.flush(&mut current, line);
                    after_colon = false;
                }
            }
        }
        self.flush(&mut current, line);
    }

    fn flush(&mut self, current: &mut Vec<String>, line: usize) {
        if !current.is_empty() {
            self.path(current, line);
            current.clear();
        }
    }
}

impl<'ast> Visit<'ast> for Crossings<'_> {
    fn visit_path(&mut self, path: &'ast syn::Path) {
        let segments: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
        self.path(&segments, path.span().start().line);
        visit::visit_path(self, path);
    }

    fn visit_item_extern_crate(&mut self, item: &'ast syn::ItemExternCrate) {
        let name = item.ident.to_string();
        self.path(&[name], item.span().start().line);
    }

    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        let line = item.span().start().line;
        if item.ident == self.boundary.adapter {
            if !self.crate_root {
                self.report(line, "declares the adapter module outside the crate root");
            } else if !self.gated(&item.attrs) {
                self.report(
                    line,
                    &format!(
                        "declares the adapter module without #[cfg(feature = \"{}\")]",
                        self.boundary.feature
                    ),
                );
            }
        }
        for attr in &item.attrs {
            let text = attr.meta.to_token_stream().to_string();
            if attr.path().is_ident("path") && text.contains(self.boundary.adapter) {
                self.report(line, "points a module at the adapter's files");
            }
        }
        visit::visit_item_mod(self, item);
    }

    fn visit_attribute(&mut self, attr: &'ast syn::Attribute) {
        if let syn::Meta::List(list) = &attr.meta {
            self.tokens(list.tokens.clone(), attr.span().start().line);
        }
        visit::visit_attribute(self, attr);
    }

    fn visit_macro(&mut self, item: &'ast syn::Macro) {
        self.tokens(item.tokens.clone(), item.span().start().line);
        visit::visit_macro(self, item);
    }
}

#[cfg(test)]
mod tests;
