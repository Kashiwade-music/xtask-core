//! Role: 構文・外部ツールのスタイル検査を統合し、理由付き例外と診断を管理する。
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use syn::spanned::Spanned;
use xtask_workspace::{AnalysisSettings, StylePolicy, StyleRule};

use crate::{Analysis, Config};

pub(crate) fn check(
    config: &Config,
    settings: &AnalysisSettings,
    policy: &StylePolicy,
    analysis: &mut Analysis,
) {
    if !policy.enabled {
        return;
    }
    if let Err(error) = policy.validate() {
        analysis
            .report
            .errors
            .push(format!("style policy: {error}"));
        return;
    }
    if policy.is_enabled(StyleRule::NonExhaustive) {
        for name in &policy.api_packages {
            let package = analysis
                .workspace
                .packages
                .iter()
                .find(|package| &package.name == name);
            if !package.is_some_and(|package| {
                package.targets.iter().any(|target| {
                    target.kind.iter().any(|kind| {
                        matches!(
                            kind.as_str(),
                            "lib" | "rlib" | "staticlib" | "cdylib" | "dylib" | "proc-macro"
                        )
                    })
                })
            }) {
                analysis.report.errors.push(format!(
                    "style api_packages `{name}` must name a workspace package with a library target"
                ));
            }
        }
    }
    let root = std::fs::canonicalize(&config.root).unwrap_or_else(|_| config.root.clone());
    let mut files = BTreeSet::new();
    for package in &analysis.workspace.packages {
        files.extend(package.targets.iter().map(|target| target.src_path.clone()));
        if let Some(directory) = package.manifest_path.parent() {
            match xtask_source::rust_files(directory) {
                Ok(discovered) => files.extend(discovered),
                Err(error) => analysis
                    .report
                    .errors
                    .push(format!("style discovery: {error}")),
            }
        }
    }
    for target in &analysis.targets {
        files.extend(
            target
                .graph
                .modules
                .iter()
                .map(|module| module.file.clone()),
        );
    }
    let files: Vec<_> = files.into_iter().collect();
    let mut diagnostics = Vec::new();
    match crate::style_source::check(&root, &analysis.workspace, &files, policy) {
        Ok(found) => diagnostics.extend(found),
        Err(error) => analysis
            .report
            .errors
            .push(format!("style source check: {error}")),
    }
    let tools = crate::style_tools::check(&root, &analysis.workspace, settings, policy);
    diagnostics.extend(tools.diagnostics);
    analysis.report.errors.extend(tools.errors);
    analysis.report.notes.extend(tools.notes);
    diagnostics.sort();
    diagnostics.dedup();
    let mut item_cache = BTreeMap::new();
    let mut used_exceptions = BTreeSet::new();
    for mut diagnostic in diagnostics {
        if !diagnostic.path.is_absolute() {
            diagnostic.path = root.join(&diagnostic.path);
        }
        diagnostic.path =
            std::fs::canonicalize(&diagnostic.path).unwrap_or_else(|_| diagnostic.path.clone());
        let relative = diagnostic
            .path
            .strip_prefix(&root)
            .unwrap_or(&diagnostic.path)
            .to_string_lossy()
            .replace('\\', "/");
        // Explicit selectors include the package, while policy exception items
        // consistently name the item within its module hierarchy.
        if let Some(item) = &diagnostic.item {
            if let Some((package, local)) = item.split_once("::") {
                if analysis
                    .workspace
                    .packages
                    .iter()
                    .any(|candidate| candidate.name == package)
                {
                    diagnostic.item = Some(local.to_owned());
                }
            }
        }
        if diagnostic.item.is_none() {
            let items = item_cache
                .entry(diagnostic.path.clone())
                .or_insert_with(|| source_items(analysis, &diagnostic.path));
            diagnostic.item = items
                .iter()
                .filter(|item| item.contains(diagnostic.line, diagnostic.column))
                .min_by_key(|item| {
                    (
                        item.end - item.start,
                        item.end_column.saturating_sub(item.start_column),
                    )
                })
                .map(|item| item.name.clone());
        }
        let matched = policy.exceptions.iter().enumerate().find(|(_, exception)| {
            exception.matches(diagnostic.rule, &relative, diagnostic.item.as_deref())
        });
        if let Some((index, exception)) = matched {
            used_exceptions.insert(index);
            analysis.report.notes.push(format!(
                "style exception [{}] {}{}: {}",
                diagnostic.rule,
                relative,
                diagnostic
                    .item
                    .as_ref()
                    .map_or(String::new(), |item| format!("::{item}")),
                exception.reason
            ));
        } else {
            analysis.report.errors.push(diagnostic.to_string());
            analysis.report.style_diagnostics.push(diagnostic);
        }
    }
    for (index, exception) in policy.exceptions.iter().enumerate() {
        if !used_exceptions.contains(&index) {
            analysis.report.notes.push(format!(
                "unused style exception [{}] {}{}",
                exception.rule,
                exception.path,
                exception
                    .item
                    .as_ref()
                    .map_or(String::new(), |item| format!("::{item}"))
            ));
        }
    }
    analysis.report.notes.push(format!(
        "checked {} style rules; source checks include inactive source syntax, compiler checks cover the configured Cargo target/features",
        policy.rules().len()
    ));
}

struct SourceItem {
    start: usize,
    end: usize,
    start_column: usize,
    end_column: usize,
    name: String,
}

impl SourceItem {
    fn contains(&self, line: usize, column: usize) -> bool {
        (self.start, self.start_column) <= (line, column)
            && (line, column) <= (self.end, self.end_column)
    }
}

fn source_items(analysis: &Analysis, file: &Path) -> Vec<SourceItem> {
    let Ok(text) = std::fs::read_to_string(file) else {
        return Vec::new();
    };
    let Ok(parsed) = syn::parse_file(&text) else {
        return Vec::new();
    };
    // Inline modules share their parent's source file. The shortest module path
    // identifies the file root; inline nesting is supplied by the AST below.
    let mut bases = BTreeSet::new();
    for target in &analysis.targets {
        if let Some(module) = target
            .graph
            .modules
            .iter()
            .filter(|module| std::fs::canonicalize(&module.file).ok().as_deref() == Some(file))
            .min_by_key(|module| module.id.len())
        {
            bases.insert(
                module
                    .id
                    .strip_prefix("crate::")
                    .unwrap_or(if module.id == "crate" { "" } else { &module.id })
                    .to_owned(),
            );
        }
    }
    if bases.is_empty() {
        bases.insert(String::new());
    }
    let mut found = Vec::new();
    for base in bases {
        index_items(&parsed.items, &base, &mut found);
    }
    found
}

fn item_name(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.into()
    } else {
        format!("{prefix}::{name}")
    }
}

fn record(span: proc_macro2::Span, name: String, found: &mut Vec<SourceItem>) {
    found.push(SourceItem {
        start: span.start().line,
        end: span.end().line,
        start_column: span.start().column + 1,
        end_column: span.end().column + 1,
        name,
    });
}

fn index_fields(fields: &syn::Fields, prefix: &str, found: &mut Vec<SourceItem>) {
    for field in fields {
        if let Some(ident) = &field.ident {
            record(field.span(), item_name(prefix, &ident.to_string()), found);
        }
    }
}

fn index_items(items: &[syn::Item], prefix: &str, found: &mut Vec<SourceItem>) {
    for item in items {
        let ident = match item {
            syn::Item::Const(item) => Some(&item.ident),
            syn::Item::Enum(item) => Some(&item.ident),
            syn::Item::Fn(item) => Some(&item.sig.ident),
            syn::Item::Mod(item) => Some(&item.ident),
            syn::Item::Static(item) => Some(&item.ident),
            syn::Item::Struct(item) => Some(&item.ident),
            syn::Item::Trait(item) => Some(&item.ident),
            syn::Item::Type(item) => Some(&item.ident),
            syn::Item::Union(item) => Some(&item.ident),
            _ => None,
        };
        let name = ident.map(|ident| item_name(prefix, &ident.to_string()));
        if let Some(name) = &name {
            record(item.span(), name.clone(), found);
        }
        match item {
            syn::Item::Mod(module) => {
                if let Some((_, children)) = &module.content {
                    index_items(children, name.as_deref().unwrap_or(prefix), found);
                }
            }
            syn::Item::Enum(item) => {
                for variant in &item.variants {
                    let variant_name = item_name(
                        name.as_deref().unwrap_or(prefix),
                        &variant.ident.to_string(),
                    );
                    record(variant.span(), variant_name.clone(), found);
                    index_fields(&variant.fields, &variant_name, found);
                }
            }
            syn::Item::Struct(item) => {
                index_fields(&item.fields, name.as_deref().unwrap_or(prefix), found);
            }
            syn::Item::Impl(item) => {
                let owner = match &*item.self_ty {
                    syn::Type::Path(path) => path
                        .path
                        .segments
                        .last()
                        .map(|part| item_name(prefix, &part.ident.to_string())),
                    _ => None,
                };
                for member in &item.items {
                    let ident = match member {
                        syn::ImplItem::Fn(item) => Some(&item.sig.ident),
                        syn::ImplItem::Const(item) => Some(&item.ident),
                        syn::ImplItem::Type(item) => Some(&item.ident),
                        _ => None,
                    };
                    if let Some(ident) = ident {
                        record(
                            member.span(),
                            item_name(owner.as_deref().unwrap_or(prefix), &ident.to_string()),
                            found,
                        );
                    }
                }
            }
            syn::Item::Trait(item) => {
                for member in &item.items {
                    let ident = match member {
                        syn::TraitItem::Fn(item) => Some(&item.sig.ident),
                        syn::TraitItem::Const(item) => Some(&item.ident),
                        syn::TraitItem::Type(item) => Some(&item.ident),
                        _ => None,
                    };
                    if let Some(ident) = ident {
                        record(
                            member.span(),
                            item_name(name.as_deref().unwrap_or(prefix), &ident.to_string()),
                            found,
                        );
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    //! Role: ソース位置から例外対象の正確な項目名を解決する。
    use super::*;

    #[test]
    fn nested_items_and_methods_have_exact_paths() {
        let parsed = syn::parse_file("mod api {\nstruct Options;\nimpl Options {\nfn build() {}\n}\ntrait Ext { fn run(); }\n}").unwrap();
        let mut items = Vec::new();
        index_items(&parsed.items, "", &mut items);
        assert!(
            items
                .iter()
                .any(|item| item.name == "api::Options::build" && item.start == 4)
        );
        assert!(items.iter().any(|item| item.name == "api::Ext::run"));
    }

    #[test]
    fn item_exception_does_not_expand_to_a_sibling_on_the_same_line() {
        let text = "fn approved() {} fn unapproved() {}";
        let parsed = syn::parse_file(text).unwrap();
        let mut items = Vec::new();
        index_items(&parsed.items, "", &mut items);
        let column = text.find("unapproved").unwrap() + 1;
        let matching: Vec<_> = items
            .iter()
            .filter(|item| item.contains(1, column))
            .map(|item| item.name.as_str())
            .collect();
        assert_eq!(matching, ["unapproved"]);
    }
}
