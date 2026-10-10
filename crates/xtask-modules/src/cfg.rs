//! Role: cfgとcfg_attrを評価し、解析できない属性展開を診断する。
use crate::AnalysisOptions;
use syn::{Attribute, Meta, Token, punctuated::Punctuated};

pub(crate) fn enabled(attrs: &[Attribute], options: &AnalysisOptions) -> Result<bool, String> {
    for attr in effective(attrs, options)? {
        if attr.path().is_ident("cfg") {
            let Meta::List(list) = attr else {
                return Err("invalid cfg attribute".into());
            };
            if !evaluate(
                &syn::parse2(list.tokens).map_err(|e| e.to_string())?,
                options,
            )? {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

pub(crate) fn effective(
    attrs: &[Attribute],
    options: &AnalysisOptions,
) -> Result<Vec<Meta>, String> {
    let mut result = Vec::new();
    for attr in attrs {
        expand(attr.meta.clone(), options, &mut result)?;
    }
    Ok(result)
}

fn expand(meta: Meta, options: &AnalysisOptions, result: &mut Vec<Meta>) -> Result<(), String> {
    if meta.path().is_ident("cfg_attr") {
        let Meta::List(list) = meta else {
            return Err("invalid cfg_attr".into());
        };
        let args = list
            .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
            .map_err(|e| e.to_string())?;
        let mut args = args.into_iter();
        let predicate = args.next().ok_or("empty cfg_attr")?;
        if evaluate(&predicate, options)? {
            for attr in args {
                expand(attr, options, result)?;
            }
        }
    } else {
        result.push(meta);
    }
    Ok(())
}

fn evaluate(meta: &Meta, options: &AnalysisOptions) -> Result<bool, String> {
    match meta {
        Meta::Path(path) => Ok(path
            .get_ident()
            .is_some_and(|i| options.cfg.contains(&i.to_string()))),
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
                options.features.contains(&value)
            } else {
                options.cfg.contains(&format!("{key}=\"{value}\""))
            })
        }
        Meta::List(list) => {
            let args = list
                .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
                .map_err(|e| e.to_string())?;
            let values = args
                .iter()
                .map(|m| evaluate(m, options))
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

pub(crate) fn expansion_diagnostics(
    attrs: &[Attribute],
    options: &AnalysisOptions,
    intrinsic_derives: &std::collections::BTreeSet<String>,
) -> Result<Vec<String>, String> {
    let mut diagnostics = Vec::new();
    for meta in effective(attrs, options)? {
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
                let derive = derive
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::");
                let path = derive.split("::").map(str::to_owned).collect::<Vec<_>>();
                if !intrinsic_derives.contains(&derive)
                    && !crate::intrinsic::derive(&path)
                    && !options.trusted_derives.contains(&derive)
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
                | "rustfmt::skip"
        ) && !options.trusted_attributes.contains(&name)
        {
            diagnostics.push(format!("attribute macro expansion is unsupported: {name}"));
        }
    }
    Ok(diagnostics)
}
