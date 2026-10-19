use proc_macro2::{Delimiter, TokenTree};
use syn::punctuated::Punctuated;
use syn::visit::{self, Visit};
use syn::{Attribute, Fields, Token};

use super::use_tree::UseLeaf;

pub(crate) enum Definition<'a> {
    Struct(&'a syn::ItemStruct),
    Enum(&'a syn::ItemEnum),
}

impl Definition<'_> {
    pub(crate) fn attrs(&self) -> &[Attribute] {
        match self {
            Self::Struct(item) => &item.attrs,
            Self::Enum(item) => &item.attrs,
        }
    }

    pub(crate) fn name(&self) -> String {
        match self {
            Self::Struct(item) => item.ident.to_string(),
            Self::Enum(item) => item.ident.to_string(),
        }
    }

    pub(crate) fn has_named_content(&self) -> bool {
        match self {
            Self::Struct(_) => true,
            Self::Enum(item) => item
                .variants
                .iter()
                .any(|variant| matches!(variant.fields, Fields::Named(_))),
        }
    }

    pub(crate) fn fields(&self) -> Vec<(String, &syn::Field, &[Attribute])> {
        match self {
            Self::Struct(item) => labelled(&item.fields, "")
                .into_iter()
                .map(|(label, field)| (label, field, &field.attrs[..]))
                .collect(),
            Self::Enum(item) => item
                .variants
                .iter()
                .flat_map(|variant| {
                    labelled(&variant.fields, &format!("{}.", variant.ident))
                        .into_iter()
                        .map(|(label, field)| (label, field, &field.attrs[..]))
                })
                .collect(),
        }
    }

    pub(crate) fn variant_attrs(&self) -> Vec<(String, &[Attribute])> {
        match self {
            Self::Struct(_) => Vec::new(),
            Self::Enum(item) => item
                .variants
                .iter()
                .map(|variant| (variant.ident.to_string(), &variant.attrs[..]))
                .collect(),
        }
    }
}

fn labelled<'a>(fields: &'a Fields, prefix: &str) -> Vec<(String, &'a syn::Field)> {
    fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let name = field
                .ident
                .as_ref()
                .map_or_else(|| index.to_string(), ToString::to_string);
            (format!("{prefix}{name}"), field)
        })
        .collect()
}

struct Finder<'a, 'ast> {
    name: &'a str,
    found: Vec<Definition<'ast>>,
}

impl<'ast> Visit<'ast> for Finder<'_, 'ast> {
    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        if node.ident == self.name {
            self.found.push(Definition::Struct(node));
        }
        visit::visit_item_struct(self, node);
    }

    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        if node.ident == self.name {
            self.found.push(Definition::Enum(node));
        }
        visit::visit_item_enum(self, node);
    }
}

pub(crate) fn find_definitions<'ast>(file: &'ast syn::File, name: &str) -> Vec<Definition<'ast>> {
    let mut finder = Finder {
        name,
        found: Vec::new(),
    };
    finder.visit_file(file);
    finder.found
}

pub(crate) fn serde_keys(attrs: &[Attribute]) -> Vec<String> {
    let mut keys = Vec::new();
    for attr in attrs {
        let tokens = match &attr.meta {
            syn::Meta::List(list) => list.tokens.clone(),
            _ => continue,
        };
        if attr.path().is_ident("serde") {
            keys.extend(list_keys(tokens));
        } else if attr.path().is_ident("cfg_attr") {
            keys.extend(
                nested_lists(tokens, "serde")
                    .into_iter()
                    .flat_map(list_keys),
            );
        }
    }
    keys
}

pub(crate) fn nested_lists(
    tokens: proc_macro2::TokenStream,
    ident: &str,
) -> Vec<proc_macro2::TokenStream> {
    let mut lists = Vec::new();
    let mut previous_is_target = false;
    for tree in tokens {
        match tree {
            TokenTree::Ident(found) => previous_is_target = found == ident,
            TokenTree::Group(group) => {
                if previous_is_target && group.delimiter() == Delimiter::Parenthesis {
                    lists.push(group.stream());
                } else {
                    lists.extend(nested_lists(group.stream(), ident));
                }
                previous_is_target = false;
            }
            _ => previous_is_target = false,
        }
    }
    lists
}

fn list_keys(tokens: proc_macro2::TokenStream) -> Vec<String> {
    let mut keys = Vec::new();
    let mut at_item_start = true;
    for tree in tokens {
        match tree {
            TokenTree::Ident(ident) if at_item_start => {
                keys.push(ident.to_string());
                at_item_start = false;
            }
            TokenTree::Punct(punct) if punct.as_char() == ',' => at_item_start = true,
            _ => at_item_start = false,
        }
    }
    keys
}

pub(crate) fn mentions_option(ty: &syn::Type) -> bool {
    mentions_ident(ty, &["Option"])
}

pub(crate) fn mentions_ident(ty: &syn::Type, names: &[&str]) -> bool {
    let mut collector = SegmentCollector { names: Vec::new() };
    collector.visit_type(ty);
    collector
        .names
        .iter()
        .any(|name| names.contains(&name.as_str()))
}

struct SegmentCollector {
    names: Vec<String>,
}

impl<'ast> Visit<'ast> for SegmentCollector {
    fn visit_path_segment(&mut self, segment: &'ast syn::PathSegment) {
        self.names.push(segment.ident.to_string());
        visit::visit_path_segment(self, segment);
    }
}

pub(crate) fn default_names(leaves: &[UseLeaf]) -> Vec<String> {
    let mut names = vec!["Default".to_owned()];
    for leaf in leaves {
        if leaf.path.last().is_some_and(|last| last == "Default")
            && let Some(rename) = &leaf.rename
        {
            names.push(rename.clone());
        }
    }
    names
}

pub(crate) fn derives_default(attrs: &[Attribute], default_names: &[String]) -> bool {
    attrs.iter().any(|attr| {
        let syn::Meta::List(list) = &attr.meta else {
            return false;
        };
        if attr.path().is_ident("derive") {
            return derive_lists_default(list.tokens.clone(), default_names);
        }
        if !attr.path().is_ident("cfg_attr") || starts_with_test(&list.tokens) {
            return false;
        }
        nested_lists(list.tokens.clone(), "derive")
            .into_iter()
            .any(|tokens| derive_lists_default(tokens, default_names))
    })
}

fn starts_with_test(tokens: &proc_macro2::TokenStream) -> bool {
    matches!(tokens.clone().into_iter().next(), Some(TokenTree::Ident(ident)) if ident == "test")
}

fn derive_lists_default(tokens: proc_macro2::TokenStream, default_names: &[String]) -> bool {
    let parser = Punctuated::<syn::Path, Token![,]>::parse_terminated;
    syn::parse::Parser::parse2(parser, tokens).is_ok_and(|paths| {
        paths.iter().any(|path| {
            path.segments
                .last()
                .is_some_and(|segment| default_names.contains(&segment.ident.to_string()))
        })
    })
}

pub(crate) fn find_impls<'ast>(file: &'ast syn::File, name: &str) -> Vec<&'ast syn::ItemImpl> {
    struct ImplFinder<'a, 'ast> {
        name: &'a str,
        found: Vec<&'ast syn::ItemImpl>,
    }
    impl<'ast> Visit<'ast> for ImplFinder<'_, 'ast> {
        fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
            if last_segment(&node.self_ty).is_some_and(|last| last == self.name) {
                self.found.push(node);
            }
            visit::visit_item_impl(self, node);
        }
    }
    let mut finder = ImplFinder {
        name,
        found: Vec::new(),
    };
    finder.visit_file(file);
    finder.found
}

pub(crate) fn last_segment(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Path(path) if path.qself.is_none() => path
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string()),
        _ => None,
    }
}
