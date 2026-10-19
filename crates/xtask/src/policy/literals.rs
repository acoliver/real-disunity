use std::collections::BTreeSet;

use proc_macro2::{TokenStream, TokenTree};
use syn::visit::{self, Visit};

use super::config::Config;
use super::source::Source;

pub fn check(config: &Config, files: &[Source], errors: &mut Vec<String>) {
    let mut observed = BTreeSet::new();
    for scope in &config.literal_scopes {
        let Some(file) = files.iter().find(|file| &file.relative == scope) else {
            errors.push(format!("literal scope was not scanned: {scope}"));
            continue;
        };
        let mut visitor = Numbers {
            config,
            file: scope,
            owner: None,
            negative: false,
            observed: &mut observed,
            errors,
        };
        visitor.visit_file(&file.production);
    }
    for owner in &config.owners {
        if !observed.contains(&(owner.file.clone(), owner.symbol.clone())) {
            errors.push(format!(
                "stale literal owner {}::{}",
                owner.file, owner.symbol
            ));
        }
    }
}

struct Numbers<'a> {
    config: &'a Config,
    file: &'a str,
    owner: Option<String>,
    negative: bool,
    observed: &'a mut BTreeSet<(String, String)>,
    errors: &'a mut Vec<String>,
}

impl Numbers<'_> {
    fn number(&mut self, literal: &syn::Lit) {
        let value = match literal {
            syn::Lit::Int(value) => value.base10_digits().parse::<f64>(),
            syn::Lit::Float(value) => value.base10_digits().parse::<f64>(),
            _ => return,
        };
        let allowed = if self.negative {
            &[0.0_f64, 1.0][..]
        } else {
            &[0.0_f64, 0.5, 1.0, 2.0][..]
        };
        if value.is_ok_and(|value| allowed.iter().any(|a| a.to_bits() == value.to_bits())) {
            return;
        }
        if let Some(symbol) = &self.owner
            && self
                .config
                .owners
                .iter()
                .any(|o| o.file == self.file && o.symbol == *symbol)
        {
            self.observed.insert((self.file.to_owned(), symbol.clone()));
            return;
        }
        self.errors.push(format!("{}: unowned policy numeric literal; move policy into authored input or register a justified constant owner", self.file));
    }

    fn tokens(&mut self, tokens: TokenStream) {
        let previous = self.negative;
        self.negative = false;
        for token in tokens {
            if let TokenTree::Punct(punct) = &token
                && punct.as_char() == '-'
            {
                self.negative = true;
                continue;
            }
            match token {
                TokenTree::Group(group) => self.tokens(group.stream()),
                TokenTree::Literal(literal) => self.number(&syn::Lit::new(literal)),
                _ => {}
            }
            self.negative = false;
        }
        self.negative = previous;
    }
}

impl<'ast> Visit<'ast> for Numbers<'_> {
    fn visit_item_const(&mut self, item: &'ast syn::ItemConst) {
        let previous = self.owner.replace(item.ident.to_string());
        visit::visit_item_const(self, item);
        self.owner = previous;
    }
    fn visit_expr_lit(&mut self, expr: &'ast syn::ExprLit) {
        self.number(&expr.lit);
    }
    fn visit_expr_unary(&mut self, expr: &'ast syn::ExprUnary) {
        let previous = self.negative;
        self.negative = matches!(expr.op, syn::UnOp::Neg(_));
        visit::visit_expr_unary(self, expr);
        self.negative = previous;
    }
    fn visit_type_array(&mut self, array: &'ast syn::TypeArray) {
        self.visit_type(&array.elem);
    }
    fn visit_expr_repeat(&mut self, repeat: &'ast syn::ExprRepeat) {
        self.visit_expr(&repeat.expr);
    }
    fn visit_macro(&mut self, item: &'ast syn::Macro) {
        self.tokens(item.tokens.clone());
    }
}
