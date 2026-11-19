//! Role: 名前空間ごとの宣言・名前付きの use・glob の表と、パスの解決を扱う。
use crate::{loader, model::Settings};
use std::collections::{BTreeMap, BTreeSet};
use syn::{Item, UseTree};

/// The namespaces of Rust names. A tuple or unit struct is declared in the type and value ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Namespace {
    Type,
    Value,
    Macro,
}

pub(crate) const NAMESPACES: [Namespace; 3] = [Namespace::Type, Namespace::Value, Namespace::Macro];

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Target {
    /// A crate-local module, by its index in the module list.
    Module(usize),
    /// A crate-local item in one namespace, by the index of the module that declares it.
    Item(usize, Namespace),
    /// A path outside the crate, as written.
    External(Vec<String>),
}

impl Target {
    /// Whether the target can be named in the namespace.
    pub(crate) fn matches(&self, namespace: Namespace) -> bool {
        match self {
            Target::Module(_) => namespace == Namespace::Type,
            Target::Item(_, own) => *own == namespace,
            Target::External(_) => true,
        }
    }

    /// The module that declares the target, when it is crate-local.
    pub(crate) fn owner(&self) -> Option<usize> {
        match self {
            Target::Module(owner) | Target::Item(owner, _) => Some(*owner),
            Target::External(_) => None,
        }
    }
}

/// The names that one module or one block brings into scope.
#[derive(Default)]
pub(crate) struct Scope {
    /// Declarations by namespace: a module's own items, and the root's extern crates.
    pub(crate) declared: BTreeMap<(Namespace, String), BTreeSet<Target>>,
    /// Named imports by alias. A name here hides a glob import of the same name in every
    /// namespace, since the namespaces of a named import are known only after resolution.
    pub(crate) explicit: BTreeMap<String, BTreeSet<Target>>,
    /// Names that glob imports provide, by namespace.
    pub(crate) globbed: BTreeMap<(Namespace, String), BTreeSet<Target>>,
    /// Unknown external glob imports, including those that other modules glob-imported into this one.
    pub(crate) external_globs: BTreeSet<Vec<String>>,
}

impl Scope {
    /// The targets of a name in one namespace, or `None` when this scope does not bind the name there.
    pub(crate) fn binding(&self, namespace: Namespace, name: &str) -> Option<BTreeSet<Target>> {
        let key = (namespace, name.to_owned());
        if self.declared.contains_key(&key) || self.explicit.contains_key(name) {
            let mut targets = self.declared.get(&key).cloned().unwrap_or_default();
            if let Some(explicit) = self.explicit.get(name) {
                targets.extend(
                    explicit
                        .iter()
                        .filter(|target| target.matches(namespace))
                        .cloned(),
                );
            }
            Some(targets)
        } else {
            self.globbed.get(&key).cloned()
        }
    }

    /// Whether the scope binds the name in some namespace, including through a glob import.
    pub(crate) fn binds_any(&self, name: &str) -> bool {
        self.explicit.contains_key(name)
            || NAMESPACES.iter().any(|namespace| {
                let key = (*namespace, name.to_owned());
                self.declared.contains_key(&key) || self.globbed.contains_key(&key)
            })
    }

    /// Adds what a glob import provides. Returns whether anything new was added.
    pub(crate) fn add_glob(&mut self, glob: Glob) -> bool {
        let mut changed = false;
        for (key, targets) in glob.names {
            let entry = self.globbed.entry(key).or_default();
            let before = entry.len();
            entry.extend(targets);
            changed |= entry.len() != before;
        }
        let before = self.external_globs.len();
        self.external_globs.extend(glob.external);
        changed | (self.external_globs.len() != before)
    }
}

/// What a glob import provides, computed from the scopes it names before anything is added.
#[derive(Default)]
pub(crate) struct Glob {
    names: Vec<((Namespace, String), BTreeSet<Target>)>,
    external: BTreeSet<Vec<String>>,
}

/// A local name of a block: a generic parameter, a pattern binding, or an item of the block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LocalBinding {
    Generic,
    Value,
    Item(Target),
}

/// The names of one block, or of the generic parameters of one item.
#[derive(Default)]
pub(crate) struct Block {
    pub(crate) locals: BTreeMap<(Namespace, String), LocalBinding>,
    pub(crate) items: Scope,
}

/// Module-level data that does not change during resolution.
pub(crate) struct Layout {
    /// The module ids, sorted; the index of a module is its position here.
    pub(crate) ids: Vec<String>,
    pub(crate) parents: Vec<Option<usize>>,
    /// The `use` items of each module, as imports.
    pub(crate) uses: Vec<Vec<Import>>,
}

pub(crate) type Textual = Vec<BTreeMap<String, BTreeSet<Target>>>;

#[derive(Clone)]
pub(crate) struct Import {
    pub(crate) path: Vec<String>,
    pub(crate) alias: Option<String>,
    pub(crate) glob: bool,
}

/// The module tables, read only: the layout and the scope of each module.
#[derive(Clone, Copy)]
pub(crate) struct Modules<'a> {
    pub(crate) layout: &'a Layout,
    pub(crate) scopes: &'a [Scope],
}

impl Modules<'_> {
    pub(crate) fn scope(&self, module: usize) -> Option<&Scope> {
        self.scopes.get(module)
    }

    /// The targets of a name in one namespace, seen from a module and the blocks inside it.
    pub(crate) fn lookup(
        &self,
        module: usize,
        blocks: &[Block],
        namespace: Namespace,
        name: &str,
    ) -> BTreeSet<Target> {
        let key = (namespace, name.to_owned());
        for block in blocks.iter().rev() {
            match block.locals.get(&key) {
                Some(LocalBinding::Item(target)) => return BTreeSet::from([target.clone()]),
                Some(LocalBinding::Generic | LocalBinding::Value) => return BTreeSet::new(),
                None => {}
            }
            if let Some(found) = block.items.binding(namespace, name) {
                return found;
            }
        }
        self.scope(module)
            .and_then(|scope| scope.binding(namespace, name))
            .unwrap_or_default()
    }

    fn head(
        &self,
        module: usize,
        blocks: &[Block],
        name: &str,
        namespaces: &[Namespace],
    ) -> BTreeSet<Target> {
        namespaces
            .iter()
            .flat_map(|namespace| self.lookup(module, blocks, *namespace, name))
            .collect()
    }

    /// The members of a module, in the namespace of the last segment or in any namespace.
    fn member(&self, owner: usize, name: &str, namespace: Option<Namespace>) -> BTreeSet<Target> {
        let Some(scope) = self.scope(owner) else {
            return BTreeSet::new();
        };
        NAMESPACES
            .into_iter()
            .filter(|candidate| namespace.is_none_or(|wanted| wanted == *candidate))
            .filter_map(|candidate| scope.binding(candidate, name))
            .flatten()
            .collect()
    }

    /// Resolves a path written in a module (and in the blocks inside it).
    pub(crate) fn resolve(
        &self,
        module: usize,
        blocks: &[Block],
        path: &[String],
        namespace: Option<Namespace>,
    ) -> BTreeSet<Target> {
        let Some(first) = path.first() else {
            return BTreeSet::new();
        };
        let (mut targets, mut offset) = match first.as_str() {
            "" => (BTreeSet::from([Target::External(Vec::new())]), 1),
            "crate" => (BTreeSet::from([Target::Module(0)]), 1),
            "self" => (BTreeSet::from([Target::Module(module)]), 1),
            "super" => {
                let mut owner = module;
                let mut count = 0;
                while path.get(count).is_some_and(|part| part == "super") {
                    let Some(parent) = self.layout.parents.get(owner).copied().flatten() else {
                        return BTreeSet::new();
                    };
                    owner = parent;
                    count += 1;
                }
                (BTreeSet::from([Target::Module(owner)]), count)
            }
            name => {
                let namespaces = if path.len() > 1 {
                    vec![Namespace::Type]
                } else {
                    namespace.map_or(NAMESPACES.to_vec(), |only| vec![only])
                };
                (self.head(module, blocks, name, &namespaces), 1)
            }
        };
        while offset < path.len() {
            let last = offset + 1 == path.len();
            let name = &path[offset];
            let mut next = BTreeSet::new();
            for target in targets {
                match target {
                    Target::Module(owner) => {
                        next.extend(self.member(owner, name, if last { namespace } else { None }));
                    }
                    // Associated functions and variants belong to the item's module.
                    Target::Item(owner, own) => {
                        next.insert(Target::Item(owner, own));
                    }
                    Target::External(mut external) => {
                        external.push(name.clone());
                        next.insert(Target::External(external));
                    }
                }
            }
            targets = next;
            offset += 1;
        }
        targets
    }

    fn bound_anywhere(&self, module: usize, blocks: &[Block], name: &str) -> bool {
        blocks.iter().any(|block| {
            block.items.binds_any(name)
                || NAMESPACES
                    .iter()
                    .any(|namespace| block.locals.contains_key(&(*namespace, name.to_owned())))
        }) || self
            .scope(module)
            .is_some_and(|scope| scope.binds_any(name))
    }

    /// The targets of an import. An unresolved name that no scope binds is an external path,
    /// when `external` is set.
    pub(crate) fn import_targets(
        &self,
        module: usize,
        blocks: &[Block],
        import: &Import,
        external: bool,
    ) -> BTreeSet<Target> {
        let mut targets = self.resolve(module, blocks, &import.path, None);
        let fallback = external
            && import.path.first().is_some_and(|first| {
                !matches!(first.as_str(), "crate" | "self" | "super")
                    && !self.bound_anywhere(module, blocks, first)
            });
        if targets.is_empty() && fallback {
            targets.insert(Target::External(import.path.clone()));
        }
        targets
    }

    /// What a glob import of these targets provides.
    pub(crate) fn glob(&self, targets: &BTreeSet<Target>, settings: &Settings) -> Glob {
        let mut glob = Glob::default();
        for target in targets {
            match target {
                Target::Module(owner) => {
                    let Some(scope) = self.scope(*owner) else {
                        continue;
                    };
                    glob.external.extend(scope.external_globs.iter().cloned());
                    for (namespace, name) in candidates(scope) {
                        let found = scope.binding(namespace, &name).unwrap_or_default();
                        if !found.is_empty() {
                            glob.names.push(((namespace, name), found));
                        }
                    }
                }
                Target::External(path) if !is_trusted_glob(path, settings) => {
                    glob.external.insert(path.clone());
                }
                Target::External(_) | Target::Item(..) => {}
            }
        }
        glob
    }
}

/// The names a scope binds, in the namespaces they can be bound in.
fn candidates(scope: &Scope) -> BTreeSet<(Namespace, String)> {
    let mut keys: BTreeSet<(Namespace, String)> = scope.declared.keys().cloned().collect();
    keys.extend(scope.globbed.keys().cloned());
    for name in scope.explicit.keys() {
        keys.extend(
            NAMESPACES
                .iter()
                .map(|namespace| (*namespace, name.clone())),
        );
    }
    keys
}

/// Whether a glob import is known to provide no macros, derives or attributes: the standard
/// libraries, and the globs that the policy trusts.
fn is_trusted_glob(path: &[String], settings: &Settings) -> bool {
    path.first()
        .is_some_and(|first| matches!(first.as_str(), "std" | "core" | "alloc"))
        || settings.trusted_globs.contains(&path.join("::"))
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

/// The imports of a `use` item. A leading `::` is the empty first segment.
pub(crate) fn use_imports(item: &syn::ItemUse) -> Vec<Import> {
    let mut list = Vec::new();
    let prefix = if item.leading_colon.is_some() {
        vec![String::new()]
    } else {
        Vec::new()
    };
    imports(&item.tree, prefix, &mut list);
    list
}

/// The index of a child module, when it is one of the modules.
pub(crate) fn child_index(
    ids: &[String],
    owner: usize,
    name: &impl std::fmt::Display,
) -> Option<usize> {
    let child = loader::child_id(&ids[owner], name);
    ids.binary_search(&child).ok()
}

/// The names that a module-level item declares: the namespace, the name and the target.
pub(crate) fn declarations(
    item: &Item,
    owner: usize,
    ids: &[String],
) -> Vec<(Namespace, String, Target)> {
    let local = |namespace| Target::Item(owner, namespace);
    match item {
        Item::Const(i) => vec![(
            Namespace::Value,
            i.ident.to_string(),
            local(Namespace::Value),
        )],
        Item::Fn(i) => vec![(
            Namespace::Value,
            i.sig.ident.to_string(),
            local(Namespace::Value),
        )],
        Item::Static(i) => vec![(
            Namespace::Value,
            i.ident.to_string(),
            local(Namespace::Value),
        )],
        Item::Struct(i) if matches!(i.fields, syn::Fields::Named(_)) => {
            vec![(Namespace::Type, i.ident.to_string(), local(Namespace::Type))]
        }
        Item::Struct(i) => vec![
            (Namespace::Type, i.ident.to_string(), local(Namespace::Type)),
            (
                Namespace::Value,
                i.ident.to_string(),
                local(Namespace::Value),
            ),
        ],
        Item::Enum(i) => vec![(Namespace::Type, i.ident.to_string(), local(Namespace::Type))],
        Item::Trait(i) => vec![(Namespace::Type, i.ident.to_string(), local(Namespace::Type))],
        Item::TraitAlias(i) => vec![(Namespace::Type, i.ident.to_string(), local(Namespace::Type))],
        Item::Type(i) => vec![(Namespace::Type, i.ident.to_string(), local(Namespace::Type))],
        Item::Union(i) => vec![(Namespace::Type, i.ident.to_string(), local(Namespace::Type))],
        Item::Macro(i) if i.mac.path.is_ident("macro_rules") => i
            .ident
            .as_ref()
            .map(|ident| (Namespace::Macro, ident.to_string(), local(Namespace::Macro)))
            .into_iter()
            .collect(),
        Item::Mod(i) => child_index(ids, owner, &i.ident)
            .map(|index| (Namespace::Type, i.ident.to_string(), Target::Module(index)))
            .into_iter()
            .collect(),
        Item::ExternCrate(i) => {
            let name = i.rename.as_ref().map_or(&i.ident, |(_, i)| i).to_string();
            let target = if i.ident == "self" {
                Target::Module(0)
            } else {
                Target::External(vec![i.ident.to_string()])
            };
            vec![(Namespace::Type, name, target)]
        }
        _ => Vec::new(),
    }
}
