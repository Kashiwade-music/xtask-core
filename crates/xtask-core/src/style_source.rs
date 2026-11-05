//! Role: Rust構文からimport・属性・リテラルなどの機械的なスタイル違反を検出する。
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use proc_macro2::{Span, TokenStream, TokenTree};
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{Attribute, Item, ItemMod, ItemUse, Meta, UseTree, Visibility};
use xtask_workspace::{StylePolicy, StyleRule};

use crate::model::StyleDiagnostic;

/// Structural checks inspect source, including inactive cfg branches. Macro
/// token trees are deliberately not interpreted as Rust syntax.
pub(crate) fn check(
    files: &[PathBuf],
    policy: &StylePolicy,
) -> Result<Vec<StyleDiagnostic>, String> {
    if ![
        StyleRule::HexLiterals,
        StyleRule::ImportGranularity,
        StyleRule::GlobImports,
        StyleRule::NonLeafImplementation,
        StyleRule::RustfmtSkip,
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
        parsed.insert(identity, (file, ast));
    }
    let mut diagnostics = Vec::new();
    for (file, ast) in parsed.values() {
        let mut visitor = Checker {
            policy,
            file,
            inline: false,
            in_block: false,
            module_items: &ast.items,
            diagnostics: &mut diagnostics,
        };
        visitor.check_module(&ast.items);
        visitor.visit_file(ast);
    }
    Ok(diagnostics)
}

struct Checker<'a, 'b> {
    policy: &'a StylePolicy,
    file: &'a Path,
    inline: bool,
    /// Inside a function or other block body, where enum glob imports are allowed.
    in_block: bool,
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
                // Private imports only serve the module's own items, which are
                // reported themselves; flagging them would duplicate each finding.
                Item::Use(_) | Item::ExternCrate(_) => true,
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

    fn check_hex(&mut self, literal: &syn::LitInt) {
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
    }

    /// Macro input is not parsed as Rust syntax, but its literal tokens are
    /// still source text written by the author.
    fn check_hex_tokens(&mut self, tokens: TokenStream) {
        for token in tokens {
            match token {
                TokenTree::Group(group) => self.check_hex_tokens(group.stream()),
                TokenTree::Literal(literal) => {
                    if let syn::Lit::Int(literal) = syn::Lit::new(literal) {
                        self.check_hex(&literal);
                    }
                }
                TokenTree::Ident(_) | TokenTree::Punct(_) => {}
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
            let saved_block = self.in_block;
            let saved_items = self.module_items;
            self.inline = true;
            self.in_block = false;
            self.module_items = items;
            self.check_module(items);
            for item in items {
                self.visit_item(item);
            }
            self.inline = saved_inline;
            self.in_block = saved_block;
            self.module_items = saved_items;
        }
    }

    fn visit_block(&mut self, block: &'ast syn::Block) {
        let saved = self.in_block;
        self.in_block = true;
        visit::visit_block(self, block);
        self.in_block = saved;
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        self.check_hex_tokens(mac.tokens.clone());
        visit::visit_macro(self, mac);
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
            // `use Enum::*` inside a function body is allowed for enum-focused
            // code. Without name resolution, a type is recognized by its
            // UpperCamelCase name, as modules use snake_case.
            let block_enum = self.in_block
                && path
                    .last()
                    .is_some_and(|segment| segment.starts_with(char::is_uppercase));
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
            let allowed = self
                .policy
                .allowed_globs
                .iter()
                .any(|allowed| *allowed == path.join("::"));
            if !inline_super && !block_enum && !child_reexport && !allowed {
                self.emit(
                    StyleRule::GlobImports,
                    span,
                    "glob import requires an explicit exception outside inline super imports, enum variants in function bodies, non-leaf child re-exports and allowed globs",
                );
            }
        }
        visit::visit_item_use(self, import);
    }

    fn visit_lit_int(&mut self, literal: &'ast syn::LitInt) {
        self.check_hex(literal);
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

/// Test modules, whether named `test`/`tests` or gated by `#[cfg(test)]`,
/// and cross-language bridges may sit beside a non-leaf module's declarations.
fn exempt_module(module: &ItemMod) -> bool {
    matches!(module.ident.to_string().as_str(), "test" | "tests")
        || module.attrs.iter().any(|attr| {
            let mut segments = attr.path().segments.iter();
            let bridge = matches!((segments.next(), segments.next(), segments.next()),
                (Some(a), Some(b), None) if a.ident == "cxx" && b.ident == "bridge");
            let test = attr.path().is_ident("cfg")
                && attr
                    .parse_args::<syn::Ident>()
                    .is_ok_and(|predicate| predicate == "test");
            bridge || test
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
    // Includes the `rustfmt::skip::macros(...)` and `::attributes(...)` forms.
    if matches!((segments.next(), segments.next()),
        (Some(a), Some(b)) if a.ident == "rustfmt" && b.ident == "skip")
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

#[cfg(test)]
mod tests;
