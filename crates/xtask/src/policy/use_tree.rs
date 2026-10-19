use syn::spanned::Spanned;
use syn::visit::{self, Visit};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UseLeaf {
    pub(crate) path: Vec<String>,
    pub(crate) rename: Option<String>,
    pub(crate) glob: bool,
    pub(crate) line: usize,
}

pub(crate) fn use_leaves(file: &syn::File) -> Vec<UseLeaf> {
    let mut collector = Collector { leaves: Vec::new() };
    collector.visit_file(file);
    collector.leaves
}

struct Collector {
    leaves: Vec<UseLeaf>,
}

impl<'ast> Visit<'ast> for Collector {
    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        let line = node.span().start().line;
        walk(&node.tree, &mut Vec::new(), line, &mut self.leaves);
        visit::visit_item_use(self, node);
    }
}

fn walk(tree: &syn::UseTree, prefix: &mut Vec<String>, line: usize, out: &mut Vec<UseLeaf>) {
    match tree {
        syn::UseTree::Path(path) => {
            prefix.push(path.ident.to_string());
            walk(&path.tree, prefix, line, out);
            prefix.pop();
        }
        syn::UseTree::Name(name) => out.push(leaf(prefix, &name.ident, None, false, line)),
        syn::UseTree::Rename(rename) => out.push(leaf(
            prefix,
            &rename.ident,
            Some(rename.rename.to_string()),
            false,
            line,
        )),
        syn::UseTree::Glob(_) => out.push(UseLeaf {
            path: prefix.clone(),
            rename: None,
            glob: true,
            line,
        }),
        syn::UseTree::Group(group) => {
            for item in &group.items {
                walk(item, prefix, line, out);
            }
        }
    }
}

fn leaf(
    prefix: &[String],
    ident: &syn::Ident,
    rename: Option<String>,
    glob: bool,
    line: usize,
) -> UseLeaf {
    let mut path = prefix.to_vec();
    if ident != "self" {
        path.push(ident.to_string());
    }
    UseLeaf {
        path,
        rename,
        glob,
        line,
    }
}
