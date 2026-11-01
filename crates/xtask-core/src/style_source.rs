//! Role: Rust構文からimport・属性・命名などの機械的なスタイル違反を検出する。
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use proc_macro2::Span;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{Attribute, Item, ItemMod, ItemUse, Meta, UseTree, Visibility};
use xtask_workspace::{StylePolicy, StyleRule, Workspace};

use crate::model::StyleDiagnostic;

/// Structural checks inspect source, including inactive cfg branches. Macro
/// token trees are deliberately not interpreted as Rust syntax.
pub(crate) fn check(
    root: &Path,
    workspace: &Workspace,
    files: &[PathBuf],
    policy: &StylePolicy,
) -> Result<Vec<StyleDiagnostic>, String> {
    if ![
        StyleRule::HexLiterals,
        StyleRule::ImportGranularity,
        StyleRule::GlobImports,
        StyleRule::NonLeafImplementation,
        StyleRule::RustfmtSkip,
        StyleRule::ExtensionTraitNames,
    ]
    .into_iter()
    .any(|rule| policy.is_enabled(rule))
    {
        return Ok(Vec::new());
    }
    let mut parsed = BTreeMap::new();
    for file in files {
        let identity =
            fs::canonicalize(file).map_err(|error| format!("{}: {error}", file.display()))?;
        if parsed.contains_key(&identity) {
            continue;
        }
        let text =
            fs::read_to_string(file).map_err(|error| format!("{}: {error}", file.display()))?;
        let ast = syn::parse_file(&text).map_err(|error| format!("{}: {error}", file.display()))?;
        parsed.insert(identity, (file.clone(), ast));
    }
    let mut diagnostics = Vec::new();
    for (file, ast) in parsed.values() {
        let mut visitor = Checker {
            policy,
            file,
            inline: false,
            module_items: &ast.items,
            diagnostics: &mut diagnostics,
        };
        visitor.check_module(&ast.items);
        visitor.visit_file(ast);
    }
    if policy.is_enabled(StyleRule::ExtensionTraitNames) {
        check_extension_names(root, workspace, &parsed, policy, &mut diagnostics)?;
    }
    Ok(diagnostics)
}

struct Checker<'a, 'b> {
    policy: &'a StylePolicy,
    file: &'a Path,
    inline: bool,
    module_items: &'a [Item],
    diagnostics: &'b mut Vec<StyleDiagnostic>,
}

impl Checker<'_, '_> {
    fn emit(&mut self, rule: StyleRule, span: Span, message: impl Into<String>) {
        if self.policy.is_enabled(rule) {
            let start = span.start();
            self.diagnostics.push(StyleDiagnostic {
                rule,
                path: self.file.to_path_buf(),
                line: start.line,
                column: start.column + 1,
                item: None,
                message: message.into(),
            });
        }
    }

    fn check_module(&mut self, items: &[Item]) {
        if !non_leaf(items) {
            return;
        }
        for item in items {
            let allowed = match item {
                Item::Mod(module) => module.content.is_none() || exempt_module(module),
                Item::Use(import) => !matches!(import.vis, Visibility::Inherited),
                _ => false,
            };
            if !allowed {
                self.emit(
                    StyleRule::NonLeafImplementation,
                    item.span(),
                    "non-leaf modules contain only module declarations and re-exports; declare a reasoned exception for glue code",
                );
            }
        }
    }
}

impl<'ast> Visit<'ast> for Checker<'ast, '_> {
    fn visit_item_mod(&mut self, module: &'ast ItemMod) {
        for attr in &module.attrs {
            self.visit_attribute(attr);
        }
        if let Some((_, items)) = &module.content {
            let saved_inline = self.inline;
            let saved_items = self.module_items;
            self.inline = true;
            self.module_items = items;
            self.check_module(items);
            for item in items {
                self.visit_item(item);
            }
            self.inline = saved_inline;
            self.module_items = saved_items;
        }
    }

    fn visit_item_use(&mut self, import: &'ast ItemUse) {
        if nested_import_paths(&import.tree) {
            self.emit(
                StyleRule::ImportGranularity,
                import.span(),
                "import items from one module per use statement; split paths nested inside braces",
            );
        }
        let mut globs = Vec::new();
        glob_paths(&import.tree, &mut Vec::new(), &mut globs);
        for (path, span) in globs {
            let inline_super = self.inline && path == ["super"];
            let child_path = match path.as_slice() {
                [first, rest @ ..] if first == "self" => rest,
                rest => rest,
            };
            let child_reexport = !matches!(import.vis, Visibility::Inherited)
                && non_leaf(self.module_items)
                && child_path.len() == 1
                && self
                    .module_items
                    .iter()
                    .any(|item| matches!(item, Item::Mod(module) if module.ident == child_path[0]));
            if !inline_super && !child_reexport {
                self.emit(
                    StyleRule::GlobImports,
                    span,
                    "glob import requires an explicit exception outside inline super imports and non-leaf child re-exports",
                );
            }
        }
        visit::visit_item_use(self, import);
    }

    fn visit_lit_int(&mut self, literal: &'ast syn::LitInt) {
        let text = literal.to_string();
        if let Some(hex) = text.strip_prefix("0x") {
            let digits = hex.strip_suffix(literal.suffix()).unwrap_or(hex);
            if digits.bytes().any(|byte| matches!(byte, b'A'..=b'F')) {
                self.emit(
                    StyleRule::HexLiterals,
                    literal.span(),
                    "use lowercase hexadecimal digits",
                );
            }
        }
        visit::visit_lit_int(self, literal);
    }

    fn visit_attribute(&mut self, attr: &'ast Attribute) {
        if contains_rustfmt_skip(&attr.meta) {
            self.emit(
                StyleRule::RustfmtSkip,
                attr.span(),
                "rustfmt::skip requires a reasoned exception for manual alignment or generated code",
            );
        }
        visit::visit_attribute(self, attr);
    }
}

fn nested_import_paths(tree: &UseTree) -> bool {
    match tree {
        UseTree::Path(path) => nested_import_paths(&path.tree),
        UseTree::Group(group) => group
            .items
            .iter()
            .any(|item| matches!(item, UseTree::Path(_)) || nested_import_paths(item)),
        _ => false,
    }
}

fn glob_paths(tree: &UseTree, prefix: &mut Vec<String>, output: &mut Vec<(Vec<String>, Span)>) {
    match tree {
        UseTree::Path(path) => {
            prefix.push(path.ident.to_string());
            glob_paths(&path.tree, prefix, output);
            prefix.pop();
        }
        UseTree::Group(group) => {
            for item in &group.items {
                glob_paths(item, prefix, output);
            }
        }
        UseTree::Glob(glob) => output.push((prefix.clone(), glob.span())),
        _ => {}
    }
}

fn exempt_module(module: &ItemMod) -> bool {
    matches!(module.ident.to_string().as_str(), "test" | "tests")
        || module.attrs.iter().any(|attr| {
            let mut segments = attr.path().segments.iter();
            matches!((segments.next(), segments.next(), segments.next()),
                (Some(a), Some(b), None) if a.ident == "cxx" && b.ident == "bridge")
        })
}

fn non_leaf(items: &[Item]) -> bool {
    items
        .iter()
        .any(|item| matches!(item, Item::Mod(module) if !exempt_module(module)))
}

fn contains_rustfmt_skip(meta: &Meta) -> bool {
    let path = meta.path();
    let mut segments = path.segments.iter();
    if matches!((segments.next(), segments.next(), segments.next()),
        (Some(a), Some(b), None) if a.ident == "rustfmt" && b.ident == "skip")
    {
        return true;
    }
    if path.is_ident("cfg_attr") {
        if let Meta::List(list) = meta {
            if let Ok(parts) = list.parse_args_with(
                syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated,
            ) {
                return parts.iter().skip(1).any(contains_rustfmt_skip);
            }
        }
    }
    false
}

type ParsedFiles = BTreeMap<PathBuf, (PathBuf, syn::File)>;

fn check_extension_names(
    root: &Path,
    workspace: &Workspace,
    files: &ParsedFiles,
    policy: &StylePolicy,
    diagnostics: &mut Vec<StyleDiagnostic>,
) -> Result<(), String> {
    let mut found = BTreeSet::new();
    for package in &workspace.packages {
        for target in &package.targets {
            let Ok(identity) = fs::canonicalize(&target.src_path) else {
                continue;
            };
            let Some((path, ast)) = files.get(&identity) else {
                continue;
            };
            let base = path.parent().unwrap_or(root);
            let mut context = SelectorVisitor {
                files,
                requested: &policy.extension_traits,
                found: &mut found,
                diagnostics,
                stack: BTreeSet::new(),
            };
            context.file(path, &package.name, base, ast);
        }
    }
    let unmatched: Vec<_> = policy
        .extension_traits
        .iter()
        .filter(|selector| !found.contains(*selector))
        .map(|selector| format!("`{selector}`"))
        .collect();
    if !unmatched.is_empty() {
        return Err(format!(
            "style.extension_traits selectors matched no source trait: {}",
            unmatched.join(", ")
        ));
    }
    Ok(())
}

struct SelectorVisitor<'a, 'b> {
    files: &'a ParsedFiles,
    requested: &'a [String],
    found: &'b mut BTreeSet<String>,
    diagnostics: &'b mut Vec<StyleDiagnostic>,
    stack: BTreeSet<PathBuf>,
}

impl SelectorVisitor<'_, '_> {
    fn file(&mut self, path: &Path, module: &str, base: &Path, ast: &syn::File) {
        let Ok(identity) = fs::canonicalize(path) else {
            return;
        };
        if !self.stack.insert(identity.clone()) {
            return;
        }
        self.items(path, module, base, false, &ast.items);
        self.stack.remove(&identity);
    }

    fn items(&mut self, path: &Path, module: &str, base: &Path, inline: bool, items: &[Item]) {
        for item in items {
            match item {
                Item::Trait(trait_) => {
                    let selector = format!("{module}::{}", trait_.ident);
                    if self.requested.contains(&selector) {
                        self.found.insert(selector.clone());
                        if !trait_.ident.to_string().ends_with("Ext") {
                            let start = trait_.ident.span().start();
                            self.diagnostics.push(StyleDiagnostic {
                                rule: StyleRule::ExtensionTraitNames,
                                path: path.to_path_buf(),
                                line: start.line,
                                column: start.column + 1,
                                item: Some(selector.clone()),
                                message: format!("extension trait `{selector}` must end in Ext"),
                            });
                        }
                    }
                }
                Item::Mod(child) => {
                    let child_module = format!("{module}::{}", child.ident);
                    let explicit: Vec<_> = child.attrs.iter().flat_map(path_attributes).collect();
                    let unconditional_path =
                        child.attrs.iter().any(|attr| attr.path().is_ident("path"));
                    let attribute_base = if inline {
                        base
                    } else {
                        path.parent().unwrap_or(base)
                    };
                    if let Some((_, contents)) = &child.content {
                        let mut bases: Vec<_> = explicit
                            .iter()
                            .map(|value| attribute_base.join(value))
                            .collect();
                        if !unconditional_path {
                            bases.push(base.join(child.ident.to_string()));
                        }
                        for child_base in bases {
                            self.items(path, &child_module, &child_base, true, contents);
                        }
                    } else {
                        let mut candidates: Vec<_> = explicit
                            .iter()
                            .map(|value| (attribute_base.join(value), true))
                            .collect();
                        if !unconditional_path {
                            candidates.extend([
                                (base.join(format!("{}.rs", child.ident)), false),
                                (base.join(child.ident.to_string()).join("mod.rs"), false),
                            ]);
                        }
                        for (candidate, explicit_path) in candidates {
                            let Ok(identity) = fs::canonicalize(&candidate) else {
                                continue;
                            };
                            if let Some((file, ast)) = self.files.get(&identity) {
                                let child_base = if explicit_path
                                    || file.file_name().is_some_and(|name| name == "mod.rs")
                                {
                                    file.parent().unwrap_or(base).to_path_buf()
                                } else {
                                    file.with_extension("")
                                };
                                self.file(file, &child_module, &child_base, ast);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

fn path_attributes(attribute: &Attribute) -> Vec<String> {
    fn values(meta: &Meta, output: &mut Vec<String>) {
        if meta.path().is_ident("path") {
            if let Meta::NameValue(value) = meta {
                if let syn::Expr::Lit(literal) = &value.value {
                    if let syn::Lit::Str(value) = &literal.lit {
                        output.push(value.value());
                    }
                }
            }
        } else if meta.path().is_ident("cfg_attr") {
            if let Meta::List(list) = meta {
                if let Ok(parts) = list.parse_args_with(
                    syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated,
                ) {
                    for part in parts.iter().skip(1) {
                        values(part, output);
                    }
                }
            }
        }
    }
    let mut result = Vec::new();
    values(&attribute.meta, &mut result);
    result
}

#[cfg(test)]
mod tests;
