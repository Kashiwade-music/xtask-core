//! Role: Rust構文からimport・属性・リテラルなどの機械的なスタイル違反を検出する。
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use proc_macro2::{Span, TokenStream, TokenTree};
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{Attribute, Item, ItemMod, ItemUse, Meta, UseTree, Visibility};
use xtask_policy::{StyleRule, StyleSettings};

use crate::model::{StyleDiagnostic, enabled};

/// What one pass over the source files found: violations, read and parse
/// errors, and the item index used to name the item that a line belongs to.
#[derive(Default)]
pub(crate) struct SourceReport {
    pub(crate) diagnostics: Vec<StyleDiagnostic>,
    /// Read and parse errors, as `path: error`. Only reported when a source rule is enabled.
    pub(crate) errors: Vec<String>,
    pub(crate) items: BTreeMap<PathBuf, Vec<ItemSpan>>,
}

/// The extent of an item in one file, in one-based lines and columns (both ends
/// included), and its name relative to the file's module.
#[derive(Clone, Debug)]
pub(crate) struct ItemSpan {
    pub(crate) start_line: usize,
    pub(crate) end_line: usize,
    pub(crate) start_column: usize,
    pub(crate) end_column: usize,
    pub(crate) item: String,
}

impl ItemSpan {
    pub(crate) fn contains(&self, line: usize, column: usize) -> bool {
        (self.start_line, self.start_column) <= (line, column)
            && (line, column) <= (self.end_line, self.end_column)
    }
}

/// The innermost item that contains the position, when several items do.
pub(crate) fn innermost(items: &[ItemSpan], line: usize, column: usize) -> Option<&ItemSpan> {
    items
        .iter()
        .filter(|item| item.contains(line, column))
        .min_by_key(|item| {
            (
                item.end_line - item.start_line,
                item.end_column.saturating_sub(item.start_column),
            )
        })
}

/// Structural checks inspect source, including inactive cfg branches. Macro
/// token trees are deliberately not interpreted as Rust syntax. Every file is
/// read and parsed once, and its AST is dropped before the next file is read.
pub(crate) fn check(files: &[PathBuf], style: &StyleSettings) -> SourceReport {
    let source_rules = [
        StyleRule::HexLiterals,
        StyleRule::ImportGranularity,
        StyleRule::GlobImports,
        StyleRule::NonLeafImplementation,
        StyleRule::RustfmtSkip,
    ]
    .into_iter()
    .any(|rule| enabled(style, rule));
    let mut report = SourceReport::default();
    for file in files {
        index_file(file, style, source_rules, &mut report);
    }
    report
}

fn index_file(file: &Path, style: &StyleSettings, source_rules: bool, report: &mut SourceReport) {
    let parsed = fs::read_to_string(file)
        .map_err(|error| format!("{}: {error}", file.display()))
        .and_then(|text| {
            syn::parse_file(&text).map_err(|error| format!("{}: {error}", file.display()))
        });
    let ast = match parsed {
        Ok(ast) => ast,
        Err(error) => {
            if source_rules {
                report.errors.push(error);
            }
            return;
        }
    };
    let mut items = Vec::new();
    index_items(&ast.items, "", &mut items);
    items.sort_by_key(|item| item.start_line);
    report.items.insert(file.to_path_buf(), items);
    if source_rules {
        let mut diagnostics = Vec::new();
        let mut checker = Checker {
            style,
            file,
            scope: Scope {
                module: ModuleScope::File,
                items: &ast.items,
                in_body: false,
            },
            diagnostics: &mut diagnostics,
        };
        checker.check_module(&ast.items);
        checker.visit_file(&ast);
        report.diagnostics.extend(diagnostics);
    }
}

/// The module and body that the visitor is in. Each nested scope gets its own
/// value, so leaving a scope needs no restore.
#[derive(Clone, Copy)]
struct Scope<'a> {
    module: ModuleScope,
    /// The items of the innermost module, for re-export checks.
    items: &'a [Item],
    /// Inside a function or other block body, where enum glob imports are allowed.
    in_body: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ModuleScope {
    File,
    Inline,
}

struct Checker<'a, 'b> {
    style: &'a StyleSettings,
    file: &'a Path,
    scope: Scope<'a>,
    diagnostics: &'b mut Vec<StyleDiagnostic>,
}

impl<'a> Checker<'a, '_> {
    /// A checker for a nested scope. It appends to the same diagnostics, and this
    /// checker's scope is left as it was.
    fn nested(&mut self, scope: Scope<'a>) -> Checker<'a, '_> {
        Checker {
            style: self.style,
            file: self.file,
            scope,
            diagnostics: &mut *self.diagnostics,
        }
    }

    fn emit(&mut self, rule: StyleRule, span: Span, message: impl Into<String>) {
        if enabled(self.style, rule) {
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
            let scope = Scope {
                module: ModuleScope::Inline,
                items: items.as_slice(),
                in_body: false,
            };
            let mut inline = self.nested(scope);
            inline.check_module(items);
            for item in items {
                inline.visit_item(item);
            }
        }
    }

    fn visit_block(&mut self, block: &'ast syn::Block) {
        let scope = Scope {
            in_body: true,
            ..self.scope
        };
        let mut body = self.nested(scope);
        visit::visit_block(&mut body, block);
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
            let inline_super = self.scope.module == ModuleScope::Inline && path == ["super"];
            // `use Enum::*` inside a function body is allowed for enum-focused
            // code. Without name resolution, a type is recognized by its
            // UpperCamelCase name, as modules use snake_case.
            let block_enum = self.scope.in_body
                && path
                    .last()
                    .is_some_and(|segment| segment.starts_with(char::is_uppercase));
            let child_path = match path.as_slice() {
                [first, rest @ ..] if first == "self" => rest,
                rest => rest,
            };
            let child_reexport =
                !matches!(import.vis, Visibility::Inherited)
                    && non_leaf(self.scope.items)
                    && child_path.len() == 1
                    && self.scope.items.iter().any(
                        |item| matches!(item, Item::Mod(module) if module.ident == child_path[0]),
                    );
            let allowed = self.style.allowed_globs().contains(&path.join("::"));
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

pub(crate) fn item_name(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.into()
    } else {
        format!("{prefix}::{name}")
    }
}

fn record(span: Span, name: String, found: &mut Vec<ItemSpan>) {
    found.push(ItemSpan {
        start_line: span.start().line,
        end_line: span.end().line,
        start_column: span.start().column + 1,
        end_column: span.end().column + 1,
        item: name,
    });
}

fn index_fields(fields: &syn::Fields, prefix: &str, found: &mut Vec<ItemSpan>) {
    for field in fields {
        if let Some(ident) = &field.ident {
            record(field.span(), item_name(prefix, &ident.to_string()), found);
        }
    }
}

/// The item names of one file, relative to the file's module. `prefix` is the
/// module path that the names nest under; the file's own index uses none.
pub(crate) fn index_items(items: &[Item], prefix: &str, found: &mut Vec<ItemSpan>) {
    for item in items {
        let ident = match item {
            Item::Const(item) => Some(&item.ident),
            Item::Enum(item) => Some(&item.ident),
            Item::Fn(item) => Some(&item.sig.ident),
            Item::Mod(item) => Some(&item.ident),
            Item::Static(item) => Some(&item.ident),
            Item::Struct(item) => Some(&item.ident),
            Item::Trait(item) => Some(&item.ident),
            Item::Type(item) => Some(&item.ident),
            Item::Union(item) => Some(&item.ident),
            _ => None,
        };
        let name = ident.map(|ident| item_name(prefix, &ident.to_string()));
        if let Some(name) = &name {
            record(item.span(), name.clone(), found);
        }
        index_children(item, name.as_deref().unwrap_or(prefix), found);
    }
}

/// The items nested inside one item, named under `name`: module items, enum
/// variants, fields, and impl and trait members.
fn index_children(item: &Item, name: &str, found: &mut Vec<ItemSpan>) {
    match item {
        Item::Mod(module) => {
            if let Some((_, children)) = &module.content {
                index_items(children, name, found);
            }
        }
        Item::Enum(item) => {
            for variant in &item.variants {
                let variant_name = item_name(name, &variant.ident.to_string());
                record(variant.span(), variant_name.clone(), found);
                index_fields(&variant.fields, &variant_name, found);
            }
        }
        Item::Struct(item) => index_fields(&item.fields, name, found),
        Item::Impl(item) => {
            let owner = match &*item.self_ty {
                syn::Type::Path(path) => path
                    .path
                    .segments
                    .last()
                    .map(|part| item_name(name, &part.ident.to_string())),
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
                    let owner = owner.as_deref().unwrap_or(name);
                    record(member.span(), item_name(owner, &ident.to_string()), found);
                }
            }
        }
        Item::Trait(item) => {
            for member in &item.items {
                let ident = match member {
                    syn::TraitItem::Fn(item) => Some(&item.sig.ident),
                    syn::TraitItem::Const(item) => Some(&item.ident),
                    syn::TraitItem::Type(item) => Some(&item.ident),
                    _ => None,
                };
                if let Some(ident) = ident {
                    record(member.span(), item_name(name, &ident.to_string()), found);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests;
