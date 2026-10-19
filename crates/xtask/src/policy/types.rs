use syn::visit::{self, Visit};

use super::config::{Config, Role};
use super::source::Source;
use super::type_defs::{self, Definition};
use super::use_tree::use_leaves;

pub fn check(config: &Config, files: &[Source], errors: &mut Vec<String>) {
    for entry in &config.types {
        let Some(file) = files.iter().find(|f| f.relative == entry.file) else {
            errors.push(format!("registered source was not scanned: {}", entry.file));
            continue;
        };
        let definitions = type_defs::find_definitions(&file.production, &entry.name);
        if definitions.len() != 1 {
            errors.push(format!(
                "{}: registered type {} must have exactly one definition, found {}",
                entry.file,
                entry.name,
                definitions.len()
            ));
        }
        if entry.role == Role::Dto {
            for definition in &definitions {
                check_dto(definition, &entry.file, errors);
            }
        }
        for source in files {
            check_default(&source.production, &entry.name, &source.relative, errors);
        }
    }
    let names: Vec<_> = config
        .types
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    for source in files {
        aliases(&source.production, &names, &source.relative, errors);
    }
}

fn check_dto(definition: &Definition<'_>, file: &str, errors: &mut Vec<String>) {
    let name = definition.name();
    let container = type_defs::serde_keys(definition.attrs());
    let unconditional = definition
        .attrs()
        .iter()
        .filter(|attr| attr.path().is_ident("serde"))
        .cloned()
        .collect::<Vec<_>>();
    if definition.has_named_content()
        && !type_defs::serde_keys(&unconditional)
            .iter()
            .any(|key| key == "deny_unknown_fields")
    {
        errors.push(format!(
            "{file}: {name}: DTO requires serde deny_unknown_fields"
        ));
    }
    if container.iter().any(|key| key == "default") {
        errors.push(format!("{file}: {name}: DTO cannot have serde default"));
    }
    for (variant, attrs) in definition.variant_attrs() {
        if type_defs::serde_keys(attrs)
            .iter()
            .any(|key| key == "default")
        {
            errors.push(format!(
                "{file}: {name}.{variant}: DTO cannot have serde default"
            ));
        }
    }
    for (label, field, attrs) in definition.fields() {
        let keys = type_defs::serde_keys(attrs);
        if keys
            .iter()
            .any(|key| ["default", "flatten", "skip", "skip_deserializing"].contains(&key.as_str()))
        {
            errors.push(format!(
                "{file}: {name}.{label}: DTO field cannot default, flatten, or skip deserialization"
            ));
        }
        if type_defs::mentions_option(&field.ty) {
            errors.push(format!("{file}: {name}.{label}: DTO cannot use Option"));
        }
    }
}

fn check_default(file: &syn::File, name: &str, path: &str, errors: &mut Vec<String>) {
    let names = type_defs::default_names(&use_leaves(file));
    for definition in type_defs::find_definitions(file, name) {
        if type_defs::derives_default(definition.attrs(), &names) {
            errors.push(format!(
                "{path}: {name}: registered type cannot derive Default"
            ));
        }
    }
    for block in type_defs::find_impls(file, name) {
        let trait_default = block.trait_.as_ref().is_some_and(|(_, path, _)| {
            path.segments
                .last()
                .is_some_and(|s| names.contains(&s.ident.to_string()))
        });
        let inherent_default = block.trait_.is_none()
            && block
                .items
                .iter()
                .any(|item| matches!(item, syn::ImplItem::Fn(f) if f.sig.ident == "default"));
        if trait_default || inherent_default {
            errors.push(format!(
                "{path}: {name}: registered type cannot implement Default or fn default"
            ));
        }
    }
}

fn aliases(file: &syn::File, names: &[&str], path: &str, errors: &mut Vec<String>) {
    for leaf in use_leaves(file) {
        if let (Some(name), Some(rename)) = (leaf.path.last(), &leaf.rename)
            && names.contains(&name.as_str())
            && name != rename
        {
            errors.push(format!(
                "{path}: registered type rename {name} as {rename} is forbidden"
            ));
        }
        if leaf.rename.is_some() && leaf.path.last().is_some_and(|name| name == "Option") {
            errors.push(format!("{path}: cannot shadow Option in policy source"));
        }
    }
    let mut finder = Aliases {
        names,
        found: Vec::new(),
    };
    finder.visit_file(file);
    for name in finder.found {
        errors.push(format!(
            "{path}: registered type/Option alias {name} is forbidden"
        ));
    }
}

struct Aliases<'a> {
    names: &'a [&'a str],
    found: Vec<String>,
}
impl<'ast> Visit<'ast> for Aliases<'_> {
    fn visit_item_type(&mut self, node: &'ast syn::ItemType) {
        let mut guarded = self.names.to_vec();
        guarded.push("Option");
        if guarded.contains(&node.ident.to_string().as_str())
            || type_defs::mentions_ident(&node.ty, &guarded)
        {
            self.found.push(node.ident.to_string());
        }
        visit::visit_item_type(self, node);
    }
}
