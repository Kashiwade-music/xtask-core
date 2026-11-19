//! Role: Index static names declared by the standard thread_local macro.
use crate::{
    cfg,
    loader::Source,
    model::Env,
    symbols::{Import, Layout, Modules, Namespace, Scope, Target},
};
use std::collections::BTreeSet;
use syn::{
    Item, Meta, Token, ext::IdentExt, parse::Parse, parse::ParseStream, punctuated::Punctuated,
};

struct Statics(Vec<syn::ItemStatic>);

/// How a `thread_local!` call is recognized. Both are syntactic. The qualified path is
/// certainly the standard macro; the other names need the provider check after resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Provider {
    Qualified,
    Alias,
}

/// A call of `thread_local!` in a module, whose provider is confirmed after resolution.
pub(crate) struct Registration {
    module: usize,
    provider: Provider,
}

/// The statics of a `thread_local!` body.
pub(crate) fn statics(tokens: proc_macro2::TokenStream) -> syn::Result<Vec<syn::ItemStatic>> {
    syn::parse2::<Statics>(tokens).map(|statics| statics.0)
}

/// Whether a `use` path names the standard `thread_local` macro.
fn is_standard_import(path: &[String]) -> bool {
    matches!(
        path.iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .as_slice(),
        ["std", "thread_local"] | ["", "std", "thread_local"]
    )
}

/// The provider of a macro call that names `thread_local`, or `None` for any other macro.
/// The names are those of the module's own `use` items, not those reached by a reexport.
pub(crate) fn provider(path: &syn::Path, imports: &[Import]) -> Option<Provider> {
    let parts = path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>();
    if parts == ["std", "thread_local"] {
        return Some(Provider::Qualified);
    }
    if path.leading_colon.is_some() || parts.len() != 1 {
        return None;
    }
    let name = parts[0].as_str();
    let aliased = imports
        .iter()
        .any(|import| import.alias.as_deref() == Some(name) && is_standard_import(&import.path));
    (name == "thread_local" || aliased).then_some(Provider::Alias)
}

fn imports_unknown_thread_local(source: &Source) -> bool {
    for (item, attributes) in source.items() {
        if !matches!(item, Item::ExternCrate(_)) {
            continue;
        }
        for attribute in attributes {
            if !attribute.path().is_ident("macro_use") {
                continue;
            }
            match attribute {
                Meta::Path(_) | Meta::NameValue(_) => return true,
                Meta::List(list) => {
                    let names =
                        list.parse_args_with(Punctuated::<syn::Ident, Token![,]>::parse_terminated);
                    if names.is_err()
                        || names.is_ok_and(|names| {
                            names.iter().any(|name| name.unraw() == "thread_local")
                        })
                    {
                        return true;
                    }
                }
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

/// Declares the statics of every `thread_local!` call in the modules, and returns the calls.
/// The declaration does not depend on the resolution; the provider is checked afterwards.
pub(crate) fn register(
    sources: &[Source],
    layout: &Layout,
    scopes: &mut [Scope],
    env: Env,
) -> Result<Vec<Registration>, String> {
    let mut registrations = Vec::new();
    let mut declared = Vec::new();
    for (module, source) in sources.iter().enumerate() {
        for (item, _) in source.items() {
            let Item::Macro(mac) = item else { continue };
            let Some(provider) = provider(&mac.mac.path, &layout.uses[module]) else {
                continue;
            };
            let statics = syn::parse2::<Statics>(mac.mac.tokens.clone()).map_err(|error| {
                format!(
                    "{}: invalid thread_local declaration: {error}",
                    source.module.id
                )
            })?;
            registrations.push(Registration { module, provider });
            for item in statics.0 {
                if cfg::enabled(&item.attrs, env)? {
                    declared.push((module, item.ident.to_string()));
                }
            }
        }
    }
    for (owner, name) in declared {
        scopes[owner]
            .declared
            .entry((Namespace::Value, name))
            .or_default()
            .insert(Target::Item(owner, Namespace::Value));
    }
    Ok(registrations)
}

/// The diagnostics of the calls whose provider cannot be established: a `thread_local` that
/// a local macro, a non-standard import, an unknown glob, or an unknown `macro_use` could provide.
pub(crate) fn confirm(
    modules: &Modules,
    sources: &[Source],
    registrations: &[Registration],
) -> Vec<String> {
    let unknown_macro_use = sources
        .iter()
        .enumerate()
        .filter(|(_, source)| imports_unknown_thread_local(source))
        .map(|(module, _)| module)
        .collect::<BTreeSet<_>>();
    registrations
        .iter()
        .filter(|registration| registration.provider == Provider::Alias)
        .filter(|registration| uncertain(modules, &unknown_macro_use, registration.module))
        .map(|registration| {
            format!(
                "{}: macro provider cannot be established: thread_local",
                modules.layout.ids[registration.module]
            )
        })
        .collect()
}

/// Whether a module's `thread_local` could come from something other than the standard macro.
fn uncertain(modules: &Modules, unknown_macro_use: &BTreeSet<usize>, module: usize) -> bool {
    let mut chain = Vec::new();
    let mut current = Some(module);
    while let Some(index) = current {
        chain.push(index);
        current = modules.layout.parents.get(index).copied().flatten();
    }
    let local_macro = chain.iter().any(|index| {
        modules.scopes[*index]
            .declared
            .contains_key(&(Namespace::Macro, "thread_local".to_owned()))
    });
    let macro_use = chain.iter().any(|index| unknown_macro_use.contains(index));
    let standard = BTreeSet::from([Target::External(vec![
        "std".to_owned(),
        "thread_local".to_owned(),
    ])]);
    let Some(scope) = modules.scope(module) else {
        return true;
    };
    let explicit = scope
        .explicit
        .get("thread_local")
        .is_some_and(|targets| *targets != standard);
    local_macro || macro_use || explicit || !scope.external_globs.is_empty()
}
