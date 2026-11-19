//! Role: 宣言と再公開の名前解決を収束させ、参照と循環を解析する。
use crate::{
    ModuleGraph, cfg,
    components::components,
    loader::{Loaded, Source},
    model::{Env, ModuleKind},
    symbols::{
        Glob, Import, Layout, Modules, NAMESPACES, Namespace, Scope, Textual, child_index,
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
    let mut scopes = sources.iter().map(|_| Scope::default()).collect::<Vec<_>>();
    let uses = collect(&sources, &ids, &mut scopes);
    let layout = Layout {
        ids,
        parents: sources.iter().map(|s| s.module.parent).collect(),
        uses,
    };
    let registrations = thread_local_items::register(&sources, &layout, &mut scopes, env)?;
    add_externs(&sources, &layout.ids, &mut scopes);
    // The local paths settle first, so that an external path is only a fallback.
    settle(&mut scopes, &layout, env, false);
    settle(&mut scopes, &layout, env, true);
    let modules = Modules {
        layout: &layout,
        scopes: &scopes,
    };
    let mut diagnostics = loading.into_iter().collect::<BTreeSet<_>>();
    diagnostics.extend(thread_local_items::confirm(
        &modules,
        &sources,
        &registrations,
    ));
    let textual = textual_macros(&sources, &layout);
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
            edges: &mut edges,
            diagnostics: &mut diagnostics,
        };
        // An inline module's attributes are the expanded attributes of its `mod` item, which
        // its parent reports among the items. Only a file module reports its own.
        if source.module.kind == ModuleKind::File {
            visitor.expansions(source.attributes());
        }
        for (item, attributes) in source.items() {
            visitor.expansions(attributes);
            visitor.module_item(item);
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

/// Declares the items of every module, and collects its `use` items. Returns the imports of
/// each module.
fn collect(sources: &[Source], ids: &[String], scopes: &mut [Scope]) -> Vec<Vec<Import>> {
    let mut uses = sources.iter().map(|_| Vec::new()).collect::<Vec<_>>();
    for (owner, source) in sources.iter().enumerate() {
        for (item, attributes) in source.items() {
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
            if let Item::Use(item) = item {
                for import in use_imports(item) {
                    if let Some(alias) = &import.alias {
                        scopes[owner].explicit.entry(alias.clone()).or_default();
                    }
                    uses[owner].push(import);
                }
            }
        }
    }
    uses
}

/// The extern crates of the root are visible in every module, unless the module declares the name.
fn add_externs(sources: &[Source], ids: &[String], scopes: &mut [Scope]) {
    let Some(root) = sources.iter().position(|s| s.module.id == "crate") else {
        return;
    };
    let externs = sources[root]
        .items()
        .into_iter()
        .filter(|(item, _)| matches!(item, Item::ExternCrate(_)))
        .flat_map(|(item, _)| declarations(item, root, ids))
        .collect::<Vec<_>>();
    for scope in scopes.iter_mut() {
        for (_, name, target) in &externs {
            if NAMESPACES
                .iter()
                .any(|namespace| scope.declared.contains_key(&(*namespace, name.clone())))
            {
                continue;
            }
            for namespace in NAMESPACES {
                scope
                    .declared
                    .entry((namespace, name.clone()))
                    .or_default()
                    .insert(target.clone());
            }
        }
    }
}

/// Applies the imports of every module until no set grows. A module whose names grew
/// requeues every module that has imports; the sets only grow, so this ends. `external`
/// makes an unresolved path of an unbound name an external path.
fn settle(scopes: &mut [Scope], layout: &Layout, env: Env, external: bool) {
    let count = scopes.len();
    let mut queue = (0..count).collect::<VecDeque<_>>();
    let mut queued = vec![true; count];
    while let Some(module) = queue.pop_front() {
        queued[module] = false;
        let mut changed = false;
        for import in &layout.uses[module] {
            changed |= apply(scopes, layout, module, import, env, external);
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
    external: bool,
) -> bool {
    let (targets, glob) = {
        let view = Modules {
            layout,
            scopes: &*scopes,
        };
        let targets = view.import_targets(module, &[], import, external);
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
