//! The crate's own source, read as a syntax tree.
//!
//! `syn` rather than line matching: a `pub use` group can wrap, nest and
//! rename, and an attribute can sit on its own line or not, so only a parser
//! sees every export the compiler does however `rustfmt` lays the list out.

use std::{fs, path::PathBuf};

use syn::{Attribute, Item, Meta, UseTree, Visibility};

/// One source file of pgorm-query, parsed.
pub struct Source {
    pub path: PathBuf,
    items: Vec<Item>,
}

/// A name a file makes public: the leaf of a `pub use`, or an item declared
/// `pub` in place.
#[derive(Debug)]
pub struct Export {
    pub name: String,
    /// Behind `#[doc(hidden)]`: reachable, but not rendered as API.
    pub hidden: bool,
    /// A `pub use path::*`, which exports whatever the module happens to hold.
    pub glob: bool,
}

/// A `pub mod`, by its path below the file it was read from.
#[derive(Debug)]
pub struct PublicModule {
    pub path: String,
    pub hidden: bool,
    /// The `#[cfg(..)]` predicates gating it, as written.
    pub cfg: Vec<String>,
}

impl Source {
    /// Read and parse a file under `pgorm-query/`.
    pub fn read(relative: &str) -> Self {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("reading {}: {error}", path.display()));
        let file = syn::parse_file(&text)
            .unwrap_or_else(|error| panic!("parsing {}: {error}", path.display()));
        Self {
            path,
            items: file.items,
        }
    }

    /// Every name the file makes public at its own level, in source order.
    pub fn exports(&self) -> Vec<Export> {
        let mut exports = Vec::new();
        for item in &self.items {
            match item {
                Item::Use(item) if is_pub(&item.vis) => {
                    let hidden = is_doc_hidden(&item.attrs);
                    use_leaves(&item.tree, &mut |name, glob| {
                        exports.push(Export { name, hidden, glob });
                    });
                }
                _ => {
                    if let Some((name, attrs)) = public_declaration(item) {
                        exports.push(Export {
                            name,
                            hidden: is_doc_hidden(attrs),
                            glob: false,
                        });
                    }
                }
            }
        }
        exports
    }

    /// The names of [`exports`](Self::exports), without their attributes.
    pub fn exported_names(&self) -> Vec<String> {
        self.exports()
            .into_iter()
            .map(|export| export.name)
            .collect()
    }

    /// Every `pub mod` reachable from the file through public modules alone,
    /// nested inline modules included.
    pub fn public_modules(&self) -> Vec<PublicModule> {
        let mut found = Vec::new();
        collect_public_modules(&self.items, "", &mut found);
        found
    }
}

fn is_pub(vis: &Visibility) -> bool {
    matches!(vis, Visibility::Public(_))
}

/// The argument tokens of every `#[<name>(..)]` among `attrs`, as text.
fn attribute_arguments(attrs: &[Attribute], name: &str) -> Vec<String> {
    attrs
        .iter()
        .filter_map(|attr| match &attr.meta {
            Meta::List(list) if list.path.is_ident(name) => Some(list.tokens.to_string()),
            _ => None,
        })
        .collect()
}

fn is_doc_hidden(attrs: &[Attribute]) -> bool {
    attribute_arguments(attrs, "doc")
        .iter()
        .any(|arguments| arguments == "hidden")
}

/// Call `leaf` with the name each leaf of a use tree binds, and whether it is
/// a glob.
fn use_leaves(tree: &UseTree, leaf: &mut impl FnMut(String, bool)) {
    match tree {
        UseTree::Path(path) => use_leaves(&path.tree, leaf),
        UseTree::Name(name) => leaf(name.ident.to_string(), false),
        UseTree::Rename(rename) => leaf(rename.rename.to_string(), false),
        UseTree::Glob(_) => leaf("*".to_owned(), true),
        UseTree::Group(group) => {
            for tree in &group.items {
                use_leaves(tree, leaf);
            }
        }
    }
}

/// The name and attributes of an item declared `pub` in place. Modules are
/// [`Source::modules`]' business, and `use` items are the caller's.
fn public_declaration(item: &Item) -> Option<(String, &[Attribute])> {
    let (vis, ident, attrs) = match item {
        Item::Const(item) => (&item.vis, &item.ident, &item.attrs),
        Item::Enum(item) => (&item.vis, &item.ident, &item.attrs),
        Item::Fn(item) => (&item.vis, &item.sig.ident, &item.attrs),
        Item::Static(item) => (&item.vis, &item.ident, &item.attrs),
        Item::Struct(item) => (&item.vis, &item.ident, &item.attrs),
        Item::Trait(item) => (&item.vis, &item.ident, &item.attrs),
        Item::TraitAlias(item) => (&item.vis, &item.ident, &item.attrs),
        Item::Type(item) => (&item.vis, &item.ident, &item.attrs),
        Item::Union(item) => (&item.vis, &item.ident, &item.attrs),
        _ => return None,
    };
    is_pub(vis).then(|| (ident.to_string(), attrs.as_slice()))
}

fn collect_public_modules(items: &[Item], prefix: &str, found: &mut Vec<PublicModule>) {
    for item in items {
        let Item::Mod(module) = item else { continue };
        if !is_pub(&module.vis) {
            continue;
        }
        let path = format!("{prefix}{}", module.ident);
        if let Some((_, content)) = &module.content {
            collect_public_modules(content, &format!("{path}::"), found);
        }
        found.push(PublicModule {
            hidden: is_doc_hidden(&module.attrs),
            cfg: attribute_arguments(&module.attrs, "cfg"),
            path,
        });
    }
}
