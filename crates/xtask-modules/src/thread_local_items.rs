//! Role: Index static names declared by the standard thread_local macro.
use crate::{
    AnalysisOptions, cfg,
    loader::Source,
    symbols::{Namespace, Tables, Target},
    visitor::local_macro_targets,
};
use std::collections::{BTreeMap, BTreeSet};
use syn::{Item, Token, parse::Parse, parse::ParseStream};

struct Statics(Vec<syn::ItemStatic>);

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
            let bare = item.mac.path.is_ident("thread_local")
                && !strong
                    .get(&source.module.id)
                    .is_some_and(|names| names.contains("thread_local"));
            let qualified = parts.iter().map(String::as_str).eq(["std", "thread_local"])
                && (item.mac.path.leading_colon.is_some()
                    || !tables
                        .get(&source.module.id)
                        .is_some_and(|table| table.contains_key("std"))
                        && !strong
                            .get(&source.module.id)
                            .is_some_and(|names| names.contains("std")));
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
