//! Role: 型・値の名前空間で宣言、import、再公開とパス解決を扱う。
use std::collections::{BTreeMap, BTreeSet};
use syn::{Item, UseTree};
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Target {
    Module(String),
    Item(String, Namespace),
    External(Vec<String>),
    LocalValue,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Namespace {
    Type,
    Value,
    Both,
    Macro,
}
impl Target {
    pub(crate) fn matches(&self, namespace: Namespace) -> bool {
        match self {
            Target::Item(_, ns) => {
                *ns == namespace
                    || (*ns == Namespace::Both
                        && matches!(namespace, Namespace::Type | Namespace::Value))
            }
            Target::Module(_) => namespace == Namespace::Type,
            Target::LocalValue => namespace == Namespace::Value,
            Target::External(_) => true,
        }
    }
}
pub(crate) type Symbols = BTreeMap<String, BTreeSet<Target>>;
pub(crate) type Tables = BTreeMap<String, Symbols>;

#[derive(Clone)]
pub(crate) struct Import {
    pub(crate) path: Vec<String>,
    pub(crate) alias: Option<String>,
    pub(crate) glob: bool,
}

pub(crate) fn imports(tree: &UseTree, prefix: Vec<String>, result: &mut Vec<Import>) {
    match tree {
        UseTree::Path(p) => {
            let mut next = prefix;
            next.push(p.ident.to_string());
            imports(&p.tree, next, result);
        }
        UseTree::Name(n) => {
            let mut path = prefix;
            let alias = if n.ident == "self" {
                path.last().cloned()
            } else {
                path.push(n.ident.to_string());
                Some(n.ident.to_string())
            };
            result.push(Import {
                path,
                alias,
                glob: false,
            });
        }
        UseTree::Rename(n) => {
            let mut path = prefix;
            if n.ident != "self" {
                path.push(n.ident.to_string());
            }
            result.push(Import {
                path,
                alias: Some(n.rename.to_string()),
                glob: false,
            });
        }
        UseTree::Glob(_) => result.push(Import {
            path: prefix,
            alias: None,
            glob: true,
        }),
        UseTree::Group(g) => {
            for tree in &g.items {
                imports(tree, prefix.clone(), result);
            }
        }
    }
}

pub(crate) fn declaration(item: &Item, owner: &str) -> Option<(String, Target)> {
    let ident = match item {
        Item::Const(i) => &i.ident,
        Item::Enum(i) => &i.ident,
        Item::Fn(i) => &i.sig.ident,
        Item::Static(i) => &i.ident,
        Item::Struct(i) => &i.ident,
        Item::Trait(i) => &i.ident,
        Item::TraitAlias(i) => &i.ident,
        Item::Type(i) => &i.ident,
        Item::Union(i) => &i.ident,
        Item::Macro(i) if i.mac.path.is_ident("macro_rules") => {
            return i.ident.as_ref().map(|ident| {
                (
                    ident.to_string(),
                    Target::Item(owner.into(), Namespace::Macro),
                )
            });
        }
        Item::Mod(i) => {
            return Some((
                i.ident.to_string(),
                Target::Module(format!("{owner}::{}", i.ident)),
            ));
        }
        Item::ExternCrate(i) => {
            return Some((
                i.rename.as_ref().map_or(&i.ident, |(_, i)| i).to_string(),
                if i.ident == "self" {
                    Target::Module("crate".into())
                } else {
                    Target::External(vec![i.ident.to_string()])
                },
            ));
        }
        _ => return None,
    };
    let namespace = match item {
        Item::Const(_) | Item::Fn(_) | Item::Static(_) => Namespace::Value,
        Item::Struct(i) if !matches!(i.fields, syn::Fields::Named(_)) => Namespace::Both,
        _ => Namespace::Type,
    };
    Some((ident.to_string(), Target::Item(owner.into(), namespace)))
}

pub(crate) fn resolve_path(
    module: &str,
    path: &[String],
    tables: &Tables,
    local: Option<&Symbols>,
    namespace: Option<Namespace>,
) -> BTreeSet<Target> {
    if path.is_empty() {
        return BTreeSet::new();
    }
    let (mut targets, mut offset) = match path[0].as_str() {
        "" => (BTreeSet::from([Target::External(Vec::new())]), 1),
        "crate" => (BTreeSet::from([Target::Module("crate".into())]), 1),
        "self" => (BTreeSet::from([Target::Module(module.into())]), 1),
        "super" => {
            let mut owner = module.to_string();
            let mut n = 0;
            while path.get(n).is_some_and(|p| p == "super") {
                let Some((parent, _)) = owner.rsplit_once("::") else {
                    return BTreeSet::new();
                };
                owner = parent.into();
                n += 1;
            }
            (BTreeSet::from([Target::Module(owner)]), n)
        }
        name => {
            let binding = local.and_then(|s| s.get(name));
            let binding = if path.len() > 1
                && binding.is_some_and(|s| s == &BTreeSet::from([Target::LocalValue]))
            {
                None
            } else {
                binding
            };
            (
                binding
                    .or_else(|| tables.get(module).and_then(|s| s.get(name)))
                    .cloned()
                    .unwrap_or_default(),
                1,
            )
        }
    };
    let first_namespace = if path.len() > 1 {
        Some(Namespace::Type)
    } else {
        namespace
    };
    let filter_namespace = if offset == 1 && !matches!(path[0].as_str(), "crate" | "self" | "super")
    {
        first_namespace
    } else {
        None
    };
    if let Some(namespace) = filter_namespace {
        targets.retain(|target| target.matches(namespace));
    }
    while offset < path.len() {
        let mut next = BTreeSet::new();
        for target in targets {
            match target {
                Target::Module(owner) => {
                    if let Some(found) = tables.get(&owner).and_then(|s| s.get(&path[offset])) {
                        next.extend(
                            found
                                .iter()
                                .filter(|t| {
                                    offset + 1 != path.len()
                                        || namespace.is_none_or(|ns| t.matches(ns))
                                })
                                .cloned(),
                        );
                    }
                }
                // Associated functions/variants belong to the item's module.
                Target::Item(owner, namespace) => {
                    next.insert(Target::Item(owner, namespace));
                }
                Target::External(mut external_path) => {
                    external_path.push(path[offset].clone());
                    next.insert(Target::External(external_path));
                }
                Target::LocalValue => {}
            }
        }
        targets = next;
        offset += 1;
    }
    targets
}

pub(crate) fn apply_import(
    import: &Import,
    module: &str,
    tables: &Tables,
    symbols: &mut Symbols,
    strong: &BTreeSet<String>,
    external_fallback: bool,
) -> BTreeSet<Target> {
    let mut targets = resolve_path(module, &import.path, tables, Some(symbols), None);
    if targets.is_empty()
        && external_fallback
        && import.path.first().is_some_and(|first| {
            !matches!(first.as_str(), "crate" | "self" | "super")
                && !strong.contains(first)
                && !tables
                    .get(module)
                    .is_some_and(|table| table.contains_key(first))
        })
    {
        targets.insert(Target::External(import.path.clone()));
    }
    if targets.is_empty() {
        return targets;
    }
    if import.glob {
        for target in &targets {
            if let Target::External(_) = target {
                symbols
                    .entry("$external_globs".into())
                    .or_default()
                    .insert(target.clone());
            }
        }
        let exports = targets.iter().next().and_then(|t| match t {
            Target::Module(owner) => tables.get(owner),
            _ => None,
        });
        if let Some(exports) = exports {
            for (name, target) in exports {
                if !strong.contains(name) {
                    symbols
                        .entry(name.clone())
                        .or_default()
                        .extend(target.clone());
                }
            }
        }
    } else if let Some(alias) = &import.alias {
        symbols
            .entry(alias.clone())
            .or_default()
            .extend(targets.clone());
    }
    targets
}
