use proc_macro2::{TokenStream, TokenTree};
use syn::visit::{self, Visit};

use super::use_tree::use_leaves;

pub fn check(file: &syn::File, path: &str, errors: &mut Vec<String>) {
    for leaf in use_leaves(file) {
        let std = leaf.path.first().is_some_and(|s| s == "std");
        if std
            && (leaf.path.iter().any(|s| s == "env")
                || leaf.glob
                || leaf.path.len() == 1 && leaf.rename.is_some())
        {
            errors.push(format!(
                "{path}: environment-capable std import; pass configuration explicitly"
            ));
        }
    }
    let mut visitor = Environment { found: false };
    visitor.visit_file(file);
    if visitor.found {
        errors.push(format!(
            "{path}: process environment lookup; pass configuration explicitly"
        ));
    }
}

struct Environment {
    found: bool,
}
impl<'ast> Visit<'ast> for Environment {
    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let syn::Expr::Path(path) = call.func.as_ref() {
            let names: Vec<_> = path
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect();
            self.found |= names
                .windows(2)
                .any(|pair| pair[0] == "env" && lookup(&pair[1]));
        }
        visit::visit_expr_call(self, call);
    }
    fn visit_macro(&mut self, item: &'ast syn::Macro) {
        self.found |= macro_environment(item.tokens.clone());
    }
    fn visit_item_extern_crate(&mut self, item: &'ast syn::ItemExternCrate) {
        self.found |= item.ident == "std" && item.rename.is_some();
    }
}

fn lookup(name: &str) -> bool {
    ["var", "var_os", "vars", "vars_os"].contains(&name)
}

fn macro_environment(tokens: TokenStream) -> bool {
    let mut words = Vec::new();
    for token in tokens {
        match token {
            TokenTree::Group(group) if macro_environment(group.stream()) => return true,
            TokenTree::Ident(ident) => words.push(ident.to_string()),
            TokenTree::Punct(punct) if punct.as_char() == ':' => {}
            _ => words.clear(),
        }
        if words
            .windows(2)
            .any(|pair| pair[0] == "env" && lookup(&pair[1]))
        {
            return true;
        }
    }
    false
}
