//! Role: Index static names declared by the standard thread_local macro.
use crate::{
    ast, cfg,
    loader::{Source, located},
    model::{Env, Settings},
    symbols::{
        Import, Layout, Modules, Namespace, Scope, Target, Textual, is_local_macro,
        local_macro_targets,
    },
};
use proc_macro2::TokenStream;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
use syn::{
    Item, Meta, Token, ext::IdentExt, parse::Parse, parse::ParseStream, punctuated::Punctuated,
};

struct Statics(Vec<syn::ItemStatic>);

/// A static of a `thread_local!` body whose configuration enables it, with its attributes after
/// cfg_attr expansion. Its cfg is evaluated only by `enabled_statics`.
pub(crate) struct Enabled {
    pub(crate) item: syn::ItemStatic,
    pub(crate) metas: Vec<Meta>,
}

/// The result of judging the standard `thread_local!` calls, and of evaluating the statics of
/// the trusted calls that the visitor visits. Each static's cfg is evaluated once, here.
#[derive(Default)]
pub(crate) struct Standard {
    /// The enabled statics of the standard calls, as (module, name).
    pub(crate) declared: Vec<(usize, String)>,
    /// The enabled statics of each trusted call at module level, by (module, item index).
    pub(crate) calls: BTreeMap<(usize, usize), Vec<Enabled>>,
    /// The cfg diagnostics of the statics. An invalid cfg disables only its own static.
    pub(crate) diagnostics: Vec<String>,
}

/// Evaluates the cfg of each static of one `thread_local!` body. A static whose cfg is invalid
/// is disabled and reported once, at its attribute's line; the other statics are unaffected.
/// Returns `Err` only when the body does not parse as statics.
pub(crate) fn enabled_statics(
    tokens: &TokenStream,
    file: &Path,
    env: Env,
) -> syn::Result<(Vec<Enabled>, Vec<String>)> {
    let mut enabled = Vec::new();
    let mut diagnostics = Vec::new();
    for item in syn::parse2::<Statics>(tokens.clone())?.0 {
        let evaluated = cfg::effective(&item.attrs, env).and_then(|metas| {
            let on = cfg::enabled_metas(&metas, env)?;
            Ok((on, metas))
        });
        match evaluated {
            Ok((true, metas)) => enabled.push(Enabled { item, metas }),
            Ok((false, _)) => {}
            Err(error) => {
                let line = cfg::error_line(&item.attrs);
                diagnostics.push(located(file, line, &error));
            }
        }
    }
    Ok((enabled, diagnostics))
}

/// How a `thread_local!` call is recognized, syntactically. The visitor visits the statics of
/// both kinds; only the calls that resolve to the standard macro declare names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Provider {
    Qualified,
    Alias,
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

/// Judges the module-level `thread_local!` calls on `first`, the first pass of the names, which
/// has no such statics yet. The standard calls declare their enabled statics. The trusted calls
/// keep theirs for the visitor. Each static's cfg is evaluated here and nowhere else.
pub(crate) fn standard_statics(
    sources: &[Source],
    layout: &Layout,
    first: &[Scope],
    textual: &Textual,
    env: Env,
) -> Result<Standard, String> {
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
    let mut result = Standard::default();
    for (module, source) in sources.iter().enumerate() {
        let macro_use_visible = macro_use_visible(layout, &macro_use, module);
        for (index, (item, _)) in source.items().into_iter().enumerate() {
            let Item::Macro(item) = item else { continue };
            let standard = is_standard_call(
                &modules,
                textual,
                module,
                &item.mac.path,
                macro_use_visible,
                env,
            );
            let trusted = is_trusted_call(&modules, textual, module, item, env);
            if !standard && !trusted {
                continue;
            }
            let (enabled, diagnostics) = match (
                standard,
                enabled_statics(&item.mac.tokens, &source.module.file, env),
            ) {
                (_, Ok(evaluated)) => evaluated,
                (true, Err(error)) => {
                    return Err(format!(
                        "{}: invalid thread_local declaration: {error}",
                        source.module.id
                    ));
                }
                // Only the standard call must be a static body; for another trusted call the
                // visitor then visits nothing, as before.
                (false, Err(_)) => continue,
            };
            result.diagnostics.extend(diagnostics);
            if standard {
                result
                    .declared
                    .extend(enabled.iter().map(|s| (module, s.item.ident.to_string())));
            }
            if trusted {
                result.calls.insert((module, index), enabled);
            }
        }
    }
    Ok(result)
}

/// Whether the visitor visits the statics of a module-level macro call: a trusted call of a
/// `thread_local` provider that is not a crate-local macro.
fn is_trusted_call(
    modules: &Modules,
    textual: &Textual,
    module: usize,
    item: &syn::ItemMacro,
    env: Env,
) -> bool {
    !item.mac.path.is_ident("macro_rules")
        && env
            .settings
            .trusted_macros
            .contains(&ast::path_text(&item.mac.path))
        && !is_local_macro(&local_macro_targets(
            modules,
            textual,
            module,
            &item.mac.path,
            &[],
        ))
        && provider(&item.mac.path, &modules.layout.uses[module]).is_some()
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
