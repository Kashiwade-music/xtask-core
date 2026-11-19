//! Role: cfgとcfg_attrを評価し、解析できない属性展開を診断する。
use crate::model::Env;
use syn::{Attribute, Meta, Token, punctuated::Punctuated};

/// One predicate of a cfg: the literal `true` or `false`, or a name, `key = "value"`,
/// `all(..)`, `any(..)`, or `not(..)`.
enum Predicate {
    Bool(bool),
    Meta(Box<Meta>),
}

impl syn::parse::Parse for Predicate {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        if input.peek(syn::LitBool) {
            return Ok(Self::Bool(input.parse::<syn::LitBool>()?.value));
        }
        Ok(Self::Meta(Box::new(input.parse()?)))
    }
}

/// Whether the attributes are enabled. Module items are evaluated by the loader on their
/// expanded attributes (`enabled_metas`); this is for the attributes of other nodes.
pub(crate) fn enabled(attrs: &[Attribute], env: Env) -> Result<bool, String> {
    enabled_metas(&effective(attrs, env)?, env)
}

/// Whether the cfg predicates of expanded attributes hold.
pub(crate) fn enabled_metas(metas: &[Meta], env: Env) -> Result<bool, String> {
    for meta in metas {
        if meta.path().is_ident("cfg") {
            let Meta::List(list) = meta else {
                return Err("invalid cfg attribute".into());
            };
            let predicate =
                syn::parse2::<Predicate>(list.tokens.clone()).map_err(|e| e.to_string())?;
            if !evaluate(&predicate, env)? {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

/// Whether an expanded attribute named `name` is present.
pub(crate) fn contains(metas: &[Meta], name: &str) -> bool {
    metas.iter().any(|meta| meta.path().is_ident(name))
}

pub(crate) fn effective(attrs: &[Attribute], env: Env) -> Result<Vec<Meta>, String> {
    let mut result = Vec::new();
    for attr in attrs {
        expand(attr.meta.clone(), env, &mut result)?;
    }
    Ok(result)
}

fn expand(meta: Meta, env: Env, result: &mut Vec<Meta>) -> Result<(), String> {
    if meta.path().is_ident("cfg_attr") {
        let Meta::List(list) = meta else {
            return Err("invalid cfg_attr".into());
        };
        let args = list
            .parse_args_with(Punctuated::<Predicate, Token![,]>::parse_terminated)
            .map_err(|e| e.to_string())?;
        let mut args = args.into_iter();
        let predicate = args.next().ok_or("empty cfg_attr")?;
        if evaluate(&predicate, env)? {
            for attr in args {
                let Predicate::Meta(attr) = attr else {
                    return Err("invalid cfg_attr".into());
                };
                expand(*attr, env, result)?;
            }
        }
    } else {
        result.push(meta);
    }
    Ok(())
}

fn evaluate(predicate: &Predicate, env: Env) -> Result<bool, String> {
    match predicate {
        Predicate::Bool(value) => Ok(*value),
        Predicate::Meta(meta) => evaluate_meta(meta, env),
    }
}

fn evaluate_meta(meta: &Meta, env: Env) -> Result<bool, String> {
    match meta {
        Meta::Path(path) => match path.get_ident() {
            Some(name) => Ok(env.cfg.contains(&name.to_string())),
            None => Err("unsupported cfg predicate".into()),
        },
        Meta::NameValue(value) => {
            let key = value
                .path
                .get_ident()
                .ok_or("unsupported cfg key")?
                .to_string();
            let syn::Expr::Lit(lit) = &value.value else {
                return Err("nonliteral cfg value".into());
            };
            let syn::Lit::Str(value) = &lit.lit else {
                return Err("nonstring cfg value".into());
            };
            let value = value.value();
            Ok(if key == "feature" {
                env.settings.features.contains(&value)
            } else {
                env.cfg.contains(&format!("{key}=\"{value}\""))
            })
        }
        Meta::List(list) => {
            let args = list
                .parse_args_with(Punctuated::<Predicate, Token![,]>::parse_terminated)
                .map_err(|e| e.to_string())?;
            let values = args
                .iter()
                .map(|p| evaluate(p, env))
                .collect::<Result<Vec<_>, _>>()?;
            if list.path.is_ident("all") {
                Ok(values.into_iter().all(|v| v))
            } else if list.path.is_ident("any") {
                Ok(values.into_iter().any(|v| v))
            } else if list.path.is_ident("not") && values.len() == 1 {
                Ok(!values[0])
            } else {
                Err("unsupported cfg predicate".into())
            }
        }
    }
}

/// The expansions that the analysis does not model, among expanded attributes.
pub(crate) fn expansion_diagnostics(
    metas: &[Meta],
    env: Env,
    intrinsic_derives: &std::collections::BTreeSet<String>,
) -> Result<Vec<String>, String> {
    let mut diagnostics = Vec::new();
    for meta in metas {
        let name = meta
            .path()
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect::<Vec<_>>()
            .join("::");
        if name == "derive" {
            let Meta::List(list) = meta else {
                return Err("invalid derive attribute".into());
            };
            let derives = list
                .parse_args_with(Punctuated::<syn::Path, Token![,]>::parse_terminated)
                .map_err(|e| e.to_string())?;
            for derive in derives {
                let path = derive
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect::<Vec<_>>();
                let derive = path.join("::");
                if !intrinsic_derives.contains(&derive)
                    && !crate::intrinsic::derive(&path)
                    && !env.settings.trusted_derives.contains(&derive)
                {
                    diagnostics.push(format!("derive macro expansion is unsupported: {derive}"));
                }
            }
        } else if !matches!(
            name.as_str(),
            "cfg"
                | "path"
                | "doc"
                | "allow"
                | "warn"
                | "deny"
                | "forbid"
                | "expect"
                | "deprecated"
                | "must_use"
                | "repr"
                | "inline"
                | "cold"
                | "track_caller"
                | "test"
                | "ignore"
                | "should_panic"
                | "no_mangle"
                | "export_name"
                | "link_name"
                | "link"
                | "link_section"
                | "used"
                | "non_exhaustive"
                | "no_implicit_prelude"
                | "macro_export"
                | "macro_use"
                | "automatically_derived"
                | "unsafe"
                | "no_std"
                | "no_main"
                | "recursion_limit"
                | "type_length_limit"
                | "crate_name"
                | "crate_type"
                | "feature"
                | "instruction_set"
                | "target_feature"
                | "default"
                | "global_allocator"
                | "windows_subsystem"
                | "panic_handler"
                | "proc_macro"
                | "proc_macro_derive"
                | "proc_macro_attribute"
                | "debugger_visualizer"
                | "collapse_debuginfo"
                | "no_builtins"
                | "naked"
        ) && !is_tool_attribute(meta.path())
            && !env.settings.trusted_attributes.contains(&name)
        {
            diagnostics.push(format!("attribute macro expansion is unsupported: {name}"));
        }
    }
    Ok(diagnostics)
}

/// Tool attributes (`rustfmt::`, `clippy::`, `diagnostic::`, `rust_analyzer::`)
/// are inert for module analysis and need no expansion.
fn is_tool_attribute(path: &syn::Path) -> bool {
    path.segments.len() > 1
        && path.segments.first().is_some_and(|segment| {
            matches!(
                segment.ident.to_string().as_str(),
                "rustfmt" | "clippy" | "diagnostic" | "rust_analyzer"
            )
        })
}
