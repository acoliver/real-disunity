use std::collections::BTreeSet;
use std::path::Path;

use proc_macro2::{TokenStream, TokenTree};
use quote::ToTokens;
use syn::visit::{self, Visit};

use super::config::{Config, Role};
use super::source::{self, Source};
use super::type_defs;

pub fn check(root: &Path, config: &Config, files: &[Source], errors: &mut Vec<String>) {
    let leaves = leaves(config, files);
    let mut indexed = BTreeSet::new();
    for entry in &config.coverage {
        let key = (entry.dto.clone(), entry.field.clone());
        if !indexed.insert(key.clone()) {
            errors.push(format!(
                "duplicate consumer coverage {}.{}",
                entry.dto, entry.field
            ));
        }
        if !leaves.contains(&key) {
            errors.push(format!(
                "stale consumer coverage {}.{}",
                entry.dto, entry.field
            ));
        }
        if let Err(e) = check_test(root, &entry.test, &entry.field) {
            errors.push(format!("{}.{}: {e}", entry.dto, entry.field));
        }
    }
    for (dto, field) in leaves.difference(&indexed) {
        errors.push(format!("missing consumer coverage for {dto}.{field}"));
    }
}

fn leaves(config: &Config, files: &[Source]) -> BTreeSet<(String, String)> {
    let names: Vec<_> = config
        .types
        .iter()
        .filter(|t| t.role == Role::Dto)
        .map(|t| t.name.as_str())
        .collect();
    let mut leaves = BTreeSet::new();
    for entry in config.types.iter().filter(|t| t.role == Role::Dto) {
        if let Some(file) = files.iter().find(|f| f.relative == entry.file) {
            for definition in type_defs::find_definitions(&file.production, &entry.name) {
                for (label, field, _) in definition.fields() {
                    if !type_defs::mentions_ident(&field.ty, &names) {
                        leaves.insert((entry.name.clone(), label));
                    }
                }
            }
        }
    }
    leaves
}

fn check_test(root: &Path, reference: &str, field: &str) -> Result<(), String> {
    let (path, name) = reference
        .split_once("::")
        .ok_or("consumer test must be path.rs::function")?;
    let path = source::contained(root, path)?;
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let syntax =
        syn::parse_file(&text).map_err(|e| format!("invalid consumer test source: {e}"))?;
    let mut finder = Tests {
        name,
        found: Vec::new(),
    };
    finder.visit_file(&syntax);
    if finder.found.len() != 1 {
        return Err(format!(
            "consumer test {reference} must identify exactly one #[test] function"
        ));
    }
    let function = finder.found[0];
    if function
        .attrs
        .iter()
        .any(|attr| attr.path().is_ident("ignore"))
    {
        return Err(format!("consumer test {reference} must not be ignored"));
    }
    let tokens = flatten(function.block.to_token_stream());
    let field = field.rsplit('.').next().unwrap_or(field);
    let reads_field = tokens
        .windows(2)
        .any(|pair| pair[0] == "." && pair[1] == field);
    if !reads_field {
        return Err(format!(
            "consumer test {reference} must access field {field} (comments/strings do not count)"
        ));
    }
    Ok(())
}

fn flatten(tokens: TokenStream) -> Vec<String> {
    tokens
        .into_iter()
        .flat_map(|token| match token {
            TokenTree::Group(group) => flatten(group.stream()),
            TokenTree::Literal(_) => vec!["<literal>".to_owned()],
            other => vec![other.to_string()],
        })
        .collect()
}

struct Tests<'a, 'ast> {
    name: &'a str,
    found: Vec<&'ast syn::ItemFn>,
}
impl<'ast> Visit<'ast> for Tests<'_, 'ast> {
    fn visit_item_fn(&mut self, function: &'ast syn::ItemFn) {
        if function.sig.ident == self.name
            && function.attrs.iter().any(|a| a.path().is_ident("test"))
        {
            self.found.push(function);
        }
        visit::visit_item_fn(self, function);
    }
}
