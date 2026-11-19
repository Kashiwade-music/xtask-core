//! Role: Index static names declared by the standard thread_local macro.
use crate::{
    cfg,
    loader::Source,
    model::{Env, Settings},
    symbols::{Import, Layout, Modules, Namespace, Scope, Target, Textual},
    visitor::local_macro_targets,
};
use std::collections::BTreeSet;
use syn::{
    Item, Meta, Token, ext::IdentExt, parse::Parse, parse::ParseStream, punctuated::Punctuated,
};

struct Statics(Vec<syn::ItemStatic>);

/// How a `thread_local!` call is recognized, syntactically. The visitor visits the statics of
/// both kinds; only the calls that resolve to the standard macro declare names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Provider {
    Qualified,
    Alias,
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

/// Whether a module's `#[macro_use] extern crate` may import the `thread_local` macro.
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

/// The statics of the standard `thread_local!` calls, as (module, name). `first` is the first
/// pass of the names, which has no such statics yet; each call is judged on it.
pub(crate) fn standard_statics(
    sources: &[Source],
    layout: &Layout,
    first: &[Scope],
    textual: &Textual,
    env: Env,
) -> Result<Vec<(usize, String)>, String> {
    let modules = Modules {
        layout,
        scopes: first,
    };
    let macro_use = sources
        .iter()
        .enumerate()
        .filter(|(_, source)| imports_unknown_thread_local(source))
        .map(|(module, _)| module)
        .collect::<BTreeSet<_>>();
    let mut statics = Vec::new();
    for (module, source) in sources.iter().enumerate() {
        let macro_use_visible = macro_use_visible(layout, &macro_use, module);
        for (item, _) in source.items() {
            let Item::Macro(item) = item else { continue };
            if !is_standard_call(
                &modules,
                textual,
                module,
                &item.mac.path,
                macro_use_visible,
                env,
            ) {
                continue;
            }
            let parsed = syn::parse2::<Statics>(item.mac.tokens.clone()).map_err(|error| {
                format!(
                    "{}: invalid thread_local declaration: {error}",
                    source.module.id
                )
            })?;
            for declaration in parsed.0 {
                if cfg::enabled(&declaration.attrs, env)? {
                    statics.push((module, declaration.ident.to_string()));
                }
            }
        }
    }
    Ok(statics)
}

/// Whether a module, or one of its ancestors, has a `#[macro_use]` that may import `thread_local`.
fn macro_use_visible(layout: &Layout, macro_use: &BTreeSet<usize>, module: usize) -> bool {
    let mut current = Some(module);
    while let Some(index) = current {
        if macro_use.contains(&index) {
            return true;
        }
        current = layout.parents.get(index).copied().flatten();
    }
    false
}

/// Whether a module-level macro call is the standard `thread_local!`. When the macro namespace
/// resolves the path, the call is standard only if every target is `std::thread_local`.
/// Otherwise it is standard when its path is bare `thread_local` or `std::thread_local`, and
/// nothing in the module could bind that name to another provider.
fn is_standard_call(
    modules: &Modules,
    textual: &Textual,
    module: usize,
    path: &syn::Path,
    macro_use: bool,
    env: Env,
) -> bool {
    let targets = local_macro_targets(modules, textual, module, path, &[]);
    if !targets.is_empty() {
        return targets.iter().all(|target| {
            matches!(target, Target::External(external)
                if external.iter().map(String::as_str).eq(["std", "thread_local"]))
        });
    }
    let unknown_glob = modules
        .scope(module)
        .is_some_and(|scope| !scope.external_globs.is_empty());
    let segments = path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>();
    let settings = env.settings;
    let bare = path.is_ident("thread_local")
        && !unknown_glob
        && !macro_use
        && !uncertain(
            modules,
            module,
            "thread_local",
            &mut BTreeSet::new(),
            settings,
        );
    let qualified = segments
        .iter()
        .map(String::as_str)
        .eq(["std", "thread_local"])
        && (path.leading_colon.is_some()
            || !unknown_glob
                && !uncertain(modules, module, "std", &mut BTreeSet::new(), settings)
                && modules
                    .resolve(module, &[], &["std".to_owned()], Some(Namespace::Type))
                    .is_empty());
    bare || qualified
}

/// Whether a name may be bound by something other than the standard library, through an
/// import of the module, or of a module that it glob-imports, that does not resolve. This
/// reads the imports collected before resolution and the first pass of the names.
fn uncertain(
    modules: &Modules,
    module: usize,
    name: &str,
    visited: &mut BTreeSet<usize>,
    settings: &Settings,
) -> bool {
    if !visited.insert(module) {
        return false;
    }
    for import in &modules.layout.uses[module] {
        if !import.glob && import.alias.as_deref() != Some(name) {
            continue;
        }
        let targets = modules.resolve(module, &[], &import.path, None);
        if targets.is_empty() {
            // A trusted glob is known to provide no macros (see `Settings::trusted_globs`).
            let trusted = settings.trusted_globs.contains(&import.path.join("::"));
            if import.glob && (trusted || is_standard_glob(modules, module, &import.path)) {
                continue;
            }
            return true;
        }
        if import.glob
            && targets.iter().any(|target| {
                matches!(target, Target::Module(owner)
                    if uncertain(modules, *owner, name, visited, settings))
            })
        {
            return true;
        }
    }
    false
}

/// Whether a glob import names the standard library, whose first segment is not a local name.
fn is_standard_glob(modules: &Modules, module: usize, path: &[String]) -> bool {
    path.first().is_some_and(|first| {
        matches!(first.as_str(), "std" | "core" | "alloc")
            && modules
                .resolve(
                    module,
                    &[],
                    std::slice::from_ref(first),
                    Some(Namespace::Type),
                )
                .is_empty()
    })
}
