//! Role: 宣言と再公開の名前解決を収束させ、参照と循環を解析する。
use crate::{
    ModuleGraph, cfg,
    components::components,
    loader::{Loaded, Source},
    model::{Env, ModuleKind},
    symbols::{
        Fallback, Glob, Import, Layout, Modules, Namespace, Scope, Target, Textual, child_index,
        declarations, use_imports,
    },
    thread_local_items,
    visitor::References,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use syn::Item;

pub(crate) fn analyze(loaded: Loaded, env: Env) -> Result<ModuleGraph, String> {
    let Loaded {
        sources,
        diagnostics: loading,
    } = loaded;
    let ids = sources
        .iter()
        .map(|s| s.module.id.clone())
        .collect::<Vec<_>>();
    let parents = sources.iter().map(|s| s.module.parent).collect::<Vec<_>>();
    let layout = Layout {
        uses: sources.iter().map(module_uses).collect(),
        prelude: extern_prelude(&sources, &ids),
        macro_use: thread_local_items::macro_use_visible(&sources, &parents),
        ids,
        parents,
    };
    let textual = textual_macros(&sources, &layout);
    // The names are settled twice, each time from empty tables. The first pass has no statics of
    // the standard thread_local macro. Each call is judged on it, and the second pass declares
    // the statics of the standard calls.
    let first = tables(&sources, &layout, &[], env);
    let thread_local_items::Standard {
        declared,
        calls,
        diagnostics: thread_local_diagnostics,
    } = thread_local_items::standard_statics(&sources, &layout, &first, &textual, env)?;
    let scopes = tables(&sources, &layout, &declared, env);
    let modules = Modules {
        layout: &layout,
        scopes: &scopes,
    };
    let mut diagnostics = loading.into_iter().collect::<BTreeSet<_>>();
    diagnostics.extend(thread_local_diagnostics);
    let opaque = crate::visitor::opaque_modules(&sources, &modules, &textual, env);
    let mut edges = BTreeSet::new();
    for (module, source) in sources.iter().enumerate() {
        let mut visitor = References {
            module,
            modules,
            textual: &textual,
            file: &source.module.file,
            env,
            opaque: &opaque,
            blocks: Vec::new(),
            calls: &calls,
            edges: &mut edges,
            diagnostics: &mut diagnostics,
        };
        // An inline module's attributes are the expanded attributes of its `mod` item, which
        // its parent reports among the items. Only a file module reports its own.
        if source.module.kind == ModuleKind::File {
            visitor.expansions(source.attributes());
        }
        for (index, (item, attributes)) in source.items().into_iter().enumerate() {
            visitor.expansions(attributes);
            visitor.module_item(index, item);
        }
    }
    let modules = sources.into_iter().map(|s| s.module).collect::<Vec<_>>();
    let edges = edges.into_iter().collect::<Vec<_>>();
    let cycles = components(modules.iter().map(|m| m.id.clone()).collect(), &edges);
    Ok(ModuleGraph {
        modules,
        edges,
        cycles,
        diagnostics: diagnostics.into_iter().collect(),
    })
}

/// The imports of a module's `use` items.
fn module_uses(source: &Source) -> Vec<Import> {
    source
        .items()
        .into_iter()
        .filter_map(|(item, _)| match item {
            Item::Use(item) => Some(use_imports(item)),
            _ => None,
        })
        .flatten()
        .collect()
}

/// The names of every module, settled from empty tables: the declarations, the statics of the
/// given standard thread_local calls, and the extern crates of the root. The local paths settle
/// first, so that an external path is only a fallback.
fn tables(
    sources: &[Source],
    layout: &Layout,
    statics: &[(usize, String)],
    env: Env,
) -> Vec<Scope> {
    let mut scopes = sources.iter().map(|_| Scope::default()).collect::<Vec<_>>();
    declare(sources, layout, &mut scopes);
    for (owner, name) in statics {
        scopes[*owner]
            .declared
            .entry((Namespace::Value, name.clone()))
            .or_default()
            .insert(Target::Item(*owner, Namespace::Value));
    }
    settle(&mut scopes, layout, env, Fallback::LocalOnly);
    settle(&mut scopes, layout, env, Fallback::External);
    scopes
}

/// Declares the items of every module, and the alias of each named import. The extern crates of
/// the root are not declared here: they are the extern prelude.
fn declare(sources: &[Source], layout: &Layout, scopes: &mut [Scope]) {
    let ids = &layout.ids;
    for (owner, source) in sources.iter().enumerate() {
        for (item, attributes) in source.items() {
            if matches!(item, Item::ExternCrate(_)) && ids[owner] == "crate" {
                continue;
            }
            // A #[macro_export] macro is also declared at the crate root, wherever it is defined.
            let exported = matches!(item, Item::Macro(mac) if mac.mac.path.is_ident("macro_rules"))
                && cfg::contains(attributes, "macro_export");
            for (namespace, name, target) in declarations(item, owner, ids) {
                if exported {
                    scopes[0]
                        .declared
                        .entry((namespace, name.clone()))
                        .or_default()
                        .insert(target.clone());
                }
                scopes[owner]
                    .declared
                    .entry((namespace, name))
                    .or_default()
                    .insert(target);
            }
        }
    }
    for (owner, imports) in layout.uses.iter().enumerate() {
        for alias in imports.iter().filter_map(|import| import.alias.as_ref()) {
            scopes[owner].explicit.entry(alias.clone()).or_default();
        }
    }
}

/// The extern crates of the root, by name: the extern prelude. It is visible in every module, in
/// the type namespace, after the module's own scope.
fn extern_prelude(sources: &[Source], ids: &[String]) -> BTreeMap<String, BTreeSet<Target>> {
    let mut prelude = BTreeMap::<String, BTreeSet<Target>>::new();
    let Some(root) = sources.iter().position(|s| s.module.id == "crate") else {
        return prelude;
    };
    for (item, _) in sources[root].items() {
        if !matches!(item, Item::ExternCrate(_)) {
            continue;
        }
        for (_, name, target) in declarations(item, root, ids) {
            prelude.entry(name).or_default().insert(target);
        }
    }
    prelude
}

/// Applies the imports of every module until no set grows. A module whose names grew
/// requeues every module that has imports; the sets only grow, so this ends. `fallback`
/// decides whether an unresolved path of an unbound name is an external path.
fn settle(scopes: &mut [Scope], layout: &Layout, env: Env, fallback: Fallback) {
    let count = scopes.len();
    let mut queue = (0..count).collect::<VecDeque<_>>();
    let mut queued = vec![true; count];
    while let Some(module) = queue.pop_front() {
        queued[module] = false;
        let mut changed = false;
        for import in &layout.uses[module] {
            changed |= apply(scopes, layout, module, import, env, fallback);
        }
        if changed {
            for (dependant, imports) in layout.uses.iter().enumerate() {
                if !imports.is_empty() && !queued[dependant] {
                    queued[dependant] = true;
                    queue.push_back(dependant);
                }
            }
        }
    }
}

/// Applies one import of a module. Returns whether its scope grew.
fn apply(
    scopes: &mut [Scope],
    layout: &Layout,
    module: usize,
    import: &Import,
    env: Env,
    fallback: Fallback,
) -> bool {
    let (targets, glob) = {
        let view = Modules {
            layout,
            scopes: &*scopes,
        };
        let targets = view.import_targets(module, &[], import, fallback);
        let glob = if import.glob {
            view.glob(&targets, env.settings)
        } else {
            Glob::default()
        };
        (targets, glob)
    };
    let scope = &mut scopes[module];
    if import.glob {
        return scope.add_glob(glob);
    }
    let Some(alias) = &import.alias else {
        return false;
    };
    let entry = scope.explicit.entry(alias.clone()).or_default();
    let before = entry.len();
    entry.extend(targets);
    entry.len() != before
}

/// `macro_rules!` names in textual scope per module: own definitions plus those
/// that `#[macro_use] mod child;` adds to its parent. Declaration order is not modeled.
fn textual_macros(sources: &[Source], layout: &Layout) -> Textual {
    let mut textual: Textual = sources.iter().map(|_| BTreeMap::new()).collect();
    let mut inherited = Vec::<(usize, usize)>::new();
    for (owner, source) in sources.iter().enumerate() {
        for (item, attributes) in source.items() {
            match item {
                Item::Macro(mac) if mac.mac.path.is_ident("macro_rules") => {
                    for (namespace, name, target) in declarations(item, owner, &layout.ids) {
                        if namespace == Namespace::Macro {
                            textual[owner].entry(name).or_default().insert(target);
                        }
                    }
                }
                Item::Mod(module) if cfg::contains(attributes, "macro_use") => {
                    if let Some(child) = child_index(&layout.ids, owner, &module.ident) {
                        inherited.push((owner, child));
                    }
                }
                _ => {}
            }
        }
    }
    // Finite monotone fixed point: macros flow through chains of #[macro_use] modules.
    loop {
        let mut changed = false;
        for &(parent, child) in &inherited {
            let additions = textual[child].clone();
            for (name, targets) in additions {
                let entry = textual[parent].entry(name).or_default();
                let before = entry.len();
                entry.extend(targets);
                changed |= entry.len() != before;
            }
        }
        if !changed {
            break;
        }
    }
    textual
}
