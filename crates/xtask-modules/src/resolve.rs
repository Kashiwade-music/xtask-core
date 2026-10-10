//! Role: 宣言と再公開の名前解決を収束させ、参照と循環を解析する。
use crate::{
    AnalysisOptions, ModuleGraph, components::components, loader::Source, symbols::*,
    visitor::References,
};
use std::collections::{BTreeMap, BTreeSet};
use syn::{Item, visit::Visit};
pub(crate) fn analyze(
    mut sources: Vec<Source>,
    options: &AnalysisOptions,
) -> Result<ModuleGraph, String> {
    sources.sort_by(|a, b| a.module.id.cmp(&b.module.id));
    let mut tables = Tables::new();
    let mut uses = BTreeMap::<String, Vec<Import>>::new();
    let mut strong = BTreeMap::<String, BTreeSet<String>>::new();
    let active_modules = sources
        .iter()
        .map(|s| s.module.id.clone())
        .collect::<BTreeSet<_>>();
    for source in &sources {
        let owner = &source.module.id;
        let table = tables.entry(owner.clone()).or_default();
        for item in &source.items {
            let inactive = matches!(item, Item::Mod(module) if !active_modules.contains(&format!("{owner}::{}", module.ident)));
            if inactive {
                continue;
            }
            if let Some((name, target)) = declaration(item, owner) {
                strong
                    .entry(owner.clone())
                    .or_default()
                    .insert(name.clone());
                table.entry(name).or_default().insert(target);
            }
            if let Item::Use(item) = item {
                imports(
                    &item.tree,
                    if item.leading_colon.is_some() {
                        vec![String::new()]
                    } else {
                        Vec::new()
                    },
                    uses.entry(owner.clone()).or_default(),
                );
            }
        }
    }
    let externs = sources
        .iter()
        .find(|s| s.module.id == "crate")
        .map(|source| {
            source
                .items
                .iter()
                .filter_map(|i| {
                    if matches!(i, Item::ExternCrate(_)) {
                        declaration(i, "crate")
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for table in tables.values_mut() {
        for (name, target) in &externs {
            table
                .entry(name.clone())
                .or_insert_with(|| BTreeSet::from([target.clone()]));
        }
    }
    for (owner, imports) in &uses {
        for import in imports {
            if let Some(alias) = &import.alias {
                strong
                    .entry(owner.clone())
                    .or_default()
                    .insert(alias.clone());
            }
        }
    }
    // Finite monotone fixed point handles chained reexports and glob imports.
    for external_fallback in [false, true] {
        loop {
            let previous = tables.clone();
            for (owner, imports) in &uses {
                for import in imports {
                    apply_import(
                        import,
                        owner,
                        &previous,
                        tables.entry(owner.clone()).or_default(),
                        strong.get(owner).unwrap_or(&BTreeSet::new()),
                        external_fallback,
                    );
                }
            }
            if previous == tables {
                break;
            }
        }
    }
    let mut edges = BTreeSet::new();
    let mut diagnostics = BTreeSet::new();
    for source in &sources {
        diagnostics.extend(source.role_diagnostics.iter().cloned());
        let mut visitor = References {
            module: &source.module.id,
            tables: &tables,
            options,
            scopes: Vec::new(),
            namespace: Namespace::Type,
            edges: &mut edges,
            diagnostics: &mut diagnostics,
        };
        visitor.enabled(&source.attrs);
        for item in &source.items {
            visitor.visit_item(item);
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
