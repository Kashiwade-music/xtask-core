//! Role: Index static names declared by the standard thread_local macro.
use crate::{
    AnalysisOptions, cfg,
    loader::Source,
    symbols::{Namespace, Tables, Target, imports, resolve_path},
    visitor::local_macro_targets,
};
use std::collections::{BTreeMap, BTreeSet};
use syn::{Item, Token, parse::Parse, parse::ParseStream};

struct Statics(Vec<syn::ItemStatic>);

fn uncertain_binding(
    source: &Source,
    name: &str,
    tables: &Tables,
    sources: &[Source],
    visited: &mut BTreeSet<String>,
) -> bool {
    if !visited.insert(source.module.id.clone()) {
        return false;
    }
    for item in &source.items {
        let Item::Use(item) = item else { continue };
        let mut bindings = Vec::new();
        imports(
            &item.tree,
            if item.leading_colon.is_some() {
                vec![String::new()]
            } else {
                Vec::new()
            },
            &mut bindings,
        );
        for binding in bindings {
            if !binding.glob && binding.alias.as_deref() != Some(name) {
                continue;
            }
            let targets = resolve_path(&source.module.id, &binding.path, tables, None, None);
            if targets.is_empty() {
                return true;
            }
            if binding.glob && targets.iter().any(|target| {
                matches!(target, Target::Module(owner) if sources.iter()
                    .find(|source| source.module.id == *owner)
                    .is_some_and(|source| uncertain_binding(source, name, tables, sources, visited)))
            }) {
                return true;
            }
        }
    }
    false
}

impl Parse for Statics {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut result = Vec::new();
        while !input.is_empty() {
            let attrs = input.call(syn::Attribute::parse_outer)?;
            let vis = input.parse()?;
            let static_token = input.parse()?;
            let ident = input.parse()?;
            let colon_token = input.parse()?;
            let ty = input.parse()?;
            let eq_token = input.parse()?;
            let expr = input.parse()?;
            let semi_token = if input.is_empty() {
                Token![;](proc_macro2::Span::call_site())
            } else {
                input.parse()?
            };
            result.push(syn::ItemStatic {
                attrs,
                vis,
                static_token,
                mutability: syn::StaticMutability::None,
                ident,
                colon_token,
                ty,
                eq_token,
                expr,
                semi_token,
            });
        }
        Ok(Self(result))
    }
}

pub(crate) fn register(
    sources: &[Source],
    tables: &mut Tables,
    textual: &Tables,
    strong: &mut BTreeMap<String, BTreeSet<String>>,
    options: &AnalysisOptions,
) -> Result<(), String> {
    let mut declarations = Vec::new();
    let unknown_macro_use = sources.iter().any(|source| source.items.iter().any(|item| {
        matches!(item, Item::ExternCrate(item) if item.attrs.iter().any(|attr| attr.path().is_ident("macro_use")))
    }));
    for source in sources {
        for item in &source.items {
            let Item::Macro(item) = item else { continue };
            let targets =
                local_macro_targets(&source.module.id, &item.mac.path, tables, textual, None);
            let parts = item
                .mac
                .path
                .segments
                .iter()
                .map(|part| part.ident.to_string())
                .collect::<Vec<_>>();
            let unknown_glob = tables
                .get(&source.module.id)
                .is_some_and(|table| table.contains_key("$external_globs"));
            let bare = item.mac.path.is_ident("thread_local")
                && !unknown_glob
                && !unknown_macro_use
                && !uncertain_binding(
                    source,
                    "thread_local",
                    tables,
                    sources,
                    &mut BTreeSet::new(),
                );
            let qualified = parts.iter().map(String::as_str).eq(["std", "thread_local"])
                && (item.mac.path.leading_colon.is_some()
                    || !unknown_glob
                        && !uncertain_binding(
                            source,
                            "std",
                            tables,
                            sources,
                            &mut BTreeSet::new(),
                        )
                        && resolve_path(
                            &source.module.id,
                            &["std".into()],
                            tables,
                            None,
                            Some(Namespace::Type),
                        )
                        .is_empty());
            let standard = if targets.is_empty() {
                bare || qualified
            } else {
                targets.iter().all(|target| {
                    matches!(target, Target::External(path)
                        if path.iter().map(String::as_str).eq(["std", "thread_local"]))
                })
            };
            if !standard || !cfg::enabled(&item.attrs, options)? {
                continue;
            }
            let statics = syn::parse2::<Statics>(item.mac.tokens.clone()).map_err(|error| {
                format!(
                    "{}: invalid thread_local declaration: {error}",
                    source.module.id
                )
            })?;
            for item in statics.0 {
                if cfg::enabled(&item.attrs, options)? {
                    declarations.push((source.module.id.clone(), item.ident.to_string()));
                }
            }
        }
    }
    for (owner, name) in declarations {
        strong
            .entry(owner.clone())
            .or_default()
            .insert(name.clone());
        tables
            .entry(owner.clone())
            .or_default()
            .entry(name)
            .or_default()
            .insert(Target::Item(owner, Namespace::Value));
    }
    Ok(())
}
