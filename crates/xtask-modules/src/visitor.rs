//! Role: 有効なRust構文を走査し、名前解決済みのモジュール参照を収集する。
use crate::{
    ast, cfg, loader,
    model::{AnalysisOptions, Reference},
    symbols::{
        Namespace, Symbols, Tables, Target, apply_import, declaration, imports, resolve_path,
    },
};
use std::collections::BTreeSet;
use syn::{
    Item,
    visit::{self, Visit},
};
pub(crate) struct References<'a> {
    pub(crate) module: &'a str,
    pub(crate) tables: &'a Tables,
    /// Textual-scope `macro_rules!` names per module.
    pub(crate) textual: &'a Tables,
    pub(crate) options: &'a AnalysisOptions,
    /// Modules whose trusted crate-local item macros may define items we cannot see.
    pub(crate) opaque: &'a BTreeSet<String>,
    pub(crate) scopes: Vec<Symbols>,
    pub(crate) namespace: Namespace,
    pub(crate) edges: &'a mut BTreeSet<Reference>,
    pub(crate) diagnostics: &'a mut BTreeSet<String>,
}

impl References<'_> {
    pub(crate) fn enabled(&mut self, attrs: &[syn::Attribute]) -> bool {
        // Most expression and statement nodes have no attributes. They cannot
        // disable code or require expansion, so avoid cloning the module and
        // local symbol tables just to compute unused intrinsic derive names.
        if attrs.is_empty() {
            return true;
        }
        match cfg::enabled(attrs, self.options) {
            Ok(true) => {
                match cfg::expansion_diagnostics(attrs, self.options, &self.intrinsic_derives()) {
                    Ok(diagnostics) => {
                        for diagnostic in diagnostics {
                            self.diagnostics
                                .insert(format!("{}: {diagnostic}", self.module));
                        }
                    }
                    Err(error) => {
                        self.diagnostics.insert(format!("{}: {error}", self.module));
                    }
                }
                true
            }
            Ok(false) => false,
            Err(error) => {
                self.diagnostics.insert(format!("{}: {error}", self.module));
                false
            }
        }
    }
    fn symbols(&self) -> Symbols {
        let mut symbols = Symbols::new();
        for scope in &self.scopes {
            for (name, target) in scope {
                symbols.insert(name.clone(), target.clone());
            }
        }
        symbols
    }
    fn intrinsic_derives(&self) -> BTreeSet<String> {
        let mut symbols = self.tables.get(self.module).cloned().unwrap_or_default();
        symbols.extend(self.symbols());
        let unknown_glob = symbols.get("$external_globs").is_some_and(|targets| targets.iter().any(|target|
            matches!(target, Target::External(path) if !path.first().is_some_and(|p| p == "std" || p == "core"))));
        let mut intrinsic = if unknown_glob {
            BTreeSet::new()
        } else {
            crate::intrinsic::DERIVES
                .iter()
                .map(|s| s.to_string())
                .collect()
        };
        for (name, targets) in symbols {
            let external = targets
                .iter()
                .filter_map(|target| match target {
                    Target::External(path) => Some(path),
                    _ => None,
                })
                .collect::<Vec<_>>();
            if !external.is_empty() {
                if external.iter().all(|path| crate::intrinsic::derive(path)) {
                    intrinsic.insert(name);
                } else {
                    intrinsic.remove(&name);
                }
            }
        }
        intrinsic
    }
    fn edge(&mut self, targets: BTreeSet<Target>) {
        let owners = targets
            .iter()
            .filter_map(|t| match t {
                Target::Module(o) | Target::Item(o, _) => Some(o),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        let ambiguous = [Namespace::Type, Namespace::Value]
            .iter()
            .any(|ns| targets.iter().filter(|t| t.matches(*ns)).count() > 1);
        if owners.len() > 1 && ambiguous {
            self.diagnostics.insert(format!(
                "{}: ambiguous local path resolution: {targets:?}",
                self.module
            ));
            return;
        }
        for target in targets {
            let owner = match target {
                Target::Module(o) | Target::Item(o, _) => o,
                Target::External(_) | Target::LocalValue => continue,
            };
            if owner != self.module {
                self.edges.insert(Reference {
                    from: self.module.into(),
                    to: owner,
                });
            }
        }
    }
    /// Opaque modules that a path unresolved in the symbol tables passes through.
    fn opaque_owners(&self, parts: &[String]) -> BTreeSet<String> {
        let symbols = self.symbols();
        let mut owners = BTreeSet::new();
        for k in 1..parts.len() {
            if parts[k] == "super" {
                continue;
            }
            for target in resolve_path(
                self.module,
                &parts[..k],
                self.tables,
                Some(&symbols),
                Some(Namespace::Type),
            ) {
                if let Target::Module(owner) = target
                    && self.opaque.contains(&owner)
                    && !self
                        .tables
                        .get(&owner)
                        .is_some_and(|table| table.contains_key(&parts[k]))
                {
                    owners.insert(owner);
                }
            }
        }
        owners
    }
    /// Treat an unresolved name in an opaque module as a reference to that module.
    /// Returns whether the path was absorbed, so no unresolved-path diagnostic applies.
    fn opaque_reference(&mut self, parts: &[String]) -> bool {
        let owners = self.opaque_owners(parts);
        for owner in &owners {
            self.edge(BTreeSet::from([Target::Module(owner.clone())]));
        }
        !owners.is_empty()
    }
    fn shadow(&mut self, ident: &syn::Ident) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(ident.to_string(), BTreeSet::new());
        }
    }
    fn bind_pattern(&mut self, pat: &syn::Pat) {
        struct Bindings(Vec<syn::Ident>);
        impl<'ast> Visit<'ast> for Bindings {
            fn visit_pat_ident(&mut self, pat: &'ast syn::PatIdent) {
                self.0.push(pat.ident.clone());
                visit::visit_pat_ident(self, pat);
            }
        }
        let mut bindings = Bindings(Vec::new());
        bindings.visit_pat(pat);
        for ident in bindings.0 {
            if let Some(scope) = self.scopes.last_mut() {
                scope.insert(ident.to_string(), BTreeSet::from([Target::LocalValue]));
            }
        }
    }
    fn generic_scope(&mut self, generics: &syn::Generics) {
        self.scopes.push(Symbols::new());
        for parameter in &generics.params {
            match parameter {
                syn::GenericParam::Type(p) => self.shadow(&p.ident),
                syn::GenericParam::Const(p) => self.shadow(&p.ident),
                _ => {}
            }
        }
    }
    fn use_item(&mut self, item: &syn::ItemUse) {
        let mut imports_list = Vec::new();
        imports(
            &item.tree,
            if item.leading_colon.is_some() {
                vec![String::new()]
            } else {
                Vec::new()
            },
            &mut imports_list,
        );
        let mut symbols = self.symbols();
        let mut strong = symbols.keys().cloned().collect::<BTreeSet<_>>();
        for import in &imports_list {
            if let Some(alias) = &import.alias {
                strong.insert(alias.clone());
            }
        }
        for import in imports_list {
            let targets = apply_import(
                &import,
                self.module,
                self.tables,
                &mut symbols,
                &strong,
                true,
            );
            if targets.is_empty()
                && import
                    .path
                    .first()
                    .is_some_and(|p| matches!(p.as_str(), "crate" | "self" | "super"))
                && !self.opaque_reference(&import.path)
            {
                self.diagnostics.insert(format!(
                    "{}: unresolved explicit local import: {}",
                    self.module,
                    import.path.join("::")
                ));
            }
            self.edge(targets);
        }
        if let Some(scope) = self.scopes.last_mut() {
            scope.extend(symbols);
        }
    }
    fn condition(&mut self, expr: &syn::Expr) {
        match expr {
            syn::Expr::Let(binding) => {
                self.visit_expr(&binding.expr);
                self.visit_pat(&binding.pat);
                self.bind_pattern(&binding.pat);
            }
            syn::Expr::Binary(binary) if matches!(binary.op, syn::BinOp::And(_)) => {
                self.condition(&binary.left);
                self.condition(&binary.right);
            }
            _ => self.visit_expr(expr),
        }
    }
}

impl<'ast> Visit<'ast> for References<'_> {
    fn visit_attribute(&mut self, _: &'ast syn::Attribute) {}
    // Visibility paths describe ancestor scope rather than references.
    fn visit_visibility(&mut self, _: &'ast syn::Visibility) {}
    fn visit_expr(&mut self, expr: &'ast syn::Expr) {
        if self.enabled(ast::expr_attrs(expr)) {
            visit::visit_expr(self, expr);
        }
    }
    fn visit_stmt_macro(&mut self, stmt: &'ast syn::StmtMacro) {
        if self.enabled(&stmt.attrs) {
            visit::visit_stmt_macro(self, stmt);
        }
    }
    fn visit_pat(&mut self, pat: &'ast syn::Pat) {
        let previous = self.namespace;
        self.namespace = Namespace::Value;
        visit::visit_pat(self, pat);
        self.namespace = previous;
    }
    fn visit_pat_struct(&mut self, pat: &'ast syn::PatStruct) {
        let previous = self.namespace;
        self.namespace = Namespace::Type;
        visit::visit_pat_struct(self, pat);
        self.namespace = previous;
    }
    fn visit_expr_struct(&mut self, expr: &'ast syn::ExprStruct) {
        let previous = self.namespace;
        self.namespace = Namespace::Type;
        visit::visit_expr_struct(self, expr);
        self.namespace = previous;
    }
    fn visit_impl_item(&mut self, item: &'ast syn::ImplItem) {
        if self.enabled(ast::impl_attrs(item)) {
            visit::visit_impl_item(self, item);
        }
    }
    fn visit_trait_item(&mut self, item: &'ast syn::TraitItem) {
        if self.enabled(ast::trait_attrs(item)) {
            visit::visit_trait_item(self, item);
        }
    }
    fn visit_foreign_item(&mut self, item: &'ast syn::ForeignItem) {
        if self.enabled(ast::foreign_attrs(item)) {
            visit::visit_foreign_item(self, item);
        }
    }
    fn visit_field_value(&mut self, field: &'ast syn::FieldValue) {
        if self.enabled(&field.attrs) {
            visit::visit_field_value(self, field);
        }
    }
    fn visit_item(&mut self, item: &'ast Item) {
        if !self.enabled(loader::attrs(item)) {
            return;
        }
        // Child modules are visited separately in their own resolution environment.
        if let Item::Mod(module) = item {
            if !self.scopes.is_empty() {
                self.diagnostics.insert(format!(
                    "{}: block-local module {} is unsupported",
                    self.module, module.ident
                ));
            }
            return;
        }
        let generics = match item {
            Item::Enum(i) => Some(&i.generics),
            Item::Struct(i) => Some(&i.generics),
            Item::Union(i) => Some(&i.generics),
            Item::Trait(i) => Some(&i.generics),
            Item::Type(i) => Some(&i.generics),
            Item::Impl(i) => Some(&i.generics),
            _ => None,
        };
        if let Some(generics) = generics {
            self.generic_scope(generics);
            self.shadow(&syn::Ident::new("Self", proc_macro2::Span::call_site()));
        }
        visit::visit_item(self, item);
        if generics.is_some() {
            self.scopes.pop();
        }
    }
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        self.use_item(item);
    }
    fn visit_path(&mut self, path: &'ast syn::Path) {
        if path.leading_colon.is_none() {
            let parts = path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>();
            let targets = resolve_path(
                self.module,
                &parts,
                self.tables,
                Some(&self.symbols()),
                Some(self.namespace),
            );
            if targets.is_empty() && self.opaque_reference(&parts) {
                // Names defined by a trusted local item macro belong to its module.
            } else if targets.is_empty()
                && parts
                    .first()
                    .is_some_and(|p| matches!(p.as_str(), "crate" | "self" | "super"))
            {
                self.diagnostics.insert(format!(
                    "{}: unresolved explicit local path: {}",
                    self.module,
                    parts.join("::")
                ));
            }
            self.edge(targets);
        }
        visit::visit_path(self, path);
    }
    fn visit_expr_path(&mut self, path: &'ast syn::ExprPath) {
        let previous = self.namespace;
        self.namespace = Namespace::Value;
        visit::visit_expr_path(self, path);
        self.namespace = previous;
    }
    fn visit_type_path(&mut self, path: &'ast syn::TypePath) {
        let previous = self.namespace;
        self.namespace = Namespace::Type;
        visit::visit_type_path(self, path);
        self.namespace = previous;
    }
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        self.generic_scope(&item.sig.generics);
        self.visit_signature(&item.sig);
        for arg in &item.sig.inputs {
            if let syn::FnArg::Typed(arg) = arg {
                self.bind_pattern(&arg.pat);
            }
        }
        self.visit_block(&item.block);
        self.scopes.pop();
    }
    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        if !self.enabled(&item.attrs) {
            return;
        }
        self.generic_scope(&item.sig.generics);
        self.visit_signature(&item.sig);
        for arg in &item.sig.inputs {
            if let syn::FnArg::Typed(arg) = arg {
                self.bind_pattern(&arg.pat);
            }
        }
        self.visit_block(&item.block);
        self.scopes.pop();
    }
    fn visit_trait_item_fn(&mut self, item: &'ast syn::TraitItemFn) {
        if !self.enabled(&item.attrs) {
            return;
        }
        self.generic_scope(&item.sig.generics);
        self.visit_signature(&item.sig);
        for arg in &item.sig.inputs {
            if let syn::FnArg::Typed(arg) = arg {
                self.bind_pattern(&arg.pat);
            }
        }
        if let Some(block) = &item.default {
            self.visit_block(block);
        }
        self.scopes.pop();
    }
    fn visit_block(&mut self, block: &'ast syn::Block) {
        self.scopes.push(Symbols::new());
        // Block items and imports are visible throughout the block, including before their declaration.
        for statement in &block.stmts {
            if let syn::Stmt::Item(item) = statement {
                if !self.enabled(loader::attrs(item)) {
                    continue;
                }
                if let Some((name, target)) = declaration(item, self.module) {
                    self.scopes
                        .last_mut()
                        .unwrap()
                        .insert(name, BTreeSet::from([target]));
                }
            }
        }
        for statement in &block.stmts {
            match statement {
                syn::Stmt::Item(Item::Use(item)) if self.enabled(&item.attrs) => {
                    self.use_item(item)
                }
                _ => {}
            }
        }
        visit::visit_block(self, block);
        self.scopes.pop();
    }
    fn visit_local(&mut self, local: &'ast syn::Local) {
        if !self.enabled(&local.attrs) {
            return;
        }
        // Initializers see the previous binding, not the new let binding.
        if let Some(init) = &local.init {
            self.visit_expr(&init.expr);
            if let Some((_, diverge)) = &init.diverge {
                self.visit_expr(diverge);
            }
        }
        self.visit_pat(&local.pat);
        self.bind_pattern(&local.pat);
    }
    fn visit_expr_closure(&mut self, expr: &'ast syn::ExprClosure) {
        self.scopes.push(Symbols::new());
        for pat in &expr.inputs {
            self.visit_pat(pat);
            self.bind_pattern(pat);
        }
        self.visit_return_type(&expr.output);
        self.visit_expr(&expr.body);
        self.scopes.pop();
    }
    fn visit_expr_for_loop(&mut self, expr: &'ast syn::ExprForLoop) {
        self.visit_expr(&expr.expr);
        self.scopes.push(Symbols::new());
        self.visit_pat(&expr.pat);
        self.bind_pattern(&expr.pat);
        self.visit_block(&expr.body);
        self.scopes.pop();
    }
    fn visit_expr_if(&mut self, expr: &'ast syn::ExprIf) {
        self.scopes.push(Symbols::new());
        self.condition(&expr.cond);
        self.visit_block(&expr.then_branch);
        self.scopes.pop();
        if let Some((_, otherwise)) = &expr.else_branch {
            self.visit_expr(otherwise);
        }
    }
    fn visit_expr_while(&mut self, expr: &'ast syn::ExprWhile) {
        self.scopes.push(Symbols::new());
        self.condition(&expr.cond);
        self.visit_block(&expr.body);
        self.scopes.pop();
    }
    fn visit_arm(&mut self, arm: &'ast syn::Arm) {
        if !self.enabled(&arm.attrs) {
            return;
        }
        self.scopes.push(Symbols::new());
        self.visit_pat(&arm.pat);
        self.bind_pattern(&arm.pat);
        if let Some((_, guard)) = &arm.guard {
            self.visit_expr(guard);
        }
        self.visit_expr(&arm.body);
        self.scopes.pop();
    }
    fn visit_field(&mut self, field: &'ast syn::Field) {
        if self.enabled(&field.attrs) {
            visit::visit_field(self, field);
        }
    }
    fn visit_variant(&mut self, variant: &'ast syn::Variant) {
        if self.enabled(&variant.attrs) {
            visit::visit_variant(self, variant);
        }
    }
    fn visit_item_macro(&mut self, item: &'ast syn::ItemMacro) {
        if item.mac.path.is_ident("macro_rules") {
            return;
        }
        if self
            .options
            .trusted_macros
            .contains(&macro_path_text(&item.mac.path))
        {
            let local_macro = local_macro_targets(
                self.module,
                &item.mac.path,
                self.tables,
                self.textual,
                Some(&self.symbols()),
            );
            if is_local_macro(&local_macro) {
                self.edge(local_macro);
            }
            return;
        }
        self.diagnostics.insert(format!(
            "{}: item macro expansion is unsupported: {}",
            self.module,
            item.mac
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>()
                .join("::")
        ));
    }
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        let parts = mac
            .path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect::<Vec<_>>();
        let local_macro = local_macro_targets(
            self.module,
            &mac.path,
            self.tables,
            self.textual,
            Some(&self.symbols()),
        );
        if is_local_macro(&local_macro) {
            self.edge(local_macro);
            if !self
                .options
                .trusted_macros
                .contains(&macro_path_text(&mac.path))
            {
                self.diagnostics.insert(format!(
                    "{}: local macro expansion is unsupported: {}",
                    self.module,
                    parts.join("::")
                ));
            }
            return;
        }
        let previous_namespace = self.namespace;
        self.namespace = Namespace::Macro;
        self.visit_path(&mac.path);
        self.namespace = previous_namespace;
        if self
            .options
            .trusted_macros
            .contains(&macro_path_text(&mac.path))
        {
            return;
        }
        let external = local_macro
            .iter()
            .filter_map(|target| match target {
                Target::External(path) => Some(path),
                _ => None,
            })
            .collect::<Vec<_>>();
        let name = if !external.is_empty() {
            let names = external
                .iter()
                .filter_map(|path| crate::intrinsic::macro_name(path))
                .collect::<BTreeSet<_>>();
            if names.len() != 1
                || external
                    .iter()
                    .any(|path| crate::intrinsic::macro_name(path).is_none())
            {
                self.diagnostics.insert(format!(
                    "{}: unsupported macro provider: {}",
                    self.module,
                    parts.join("::")
                ));
                return;
            }
            names.into_iter().next().unwrap().to_owned()
        } else if parts.len() > 1 {
            let known_local = mac.path.leading_colon.is_none()
                && self
                    .tables
                    .get(self.module)
                    .is_some_and(|table| table.contains_key(&parts[0]));
            if known_local || crate::intrinsic::macro_name(&parts).is_none() {
                self.diagnostics.insert(format!(
                    "{}: qualified macro expansion is unsupported: {}",
                    self.module,
                    parts.join("::")
                ));
                return;
            }
            crate::intrinsic::macro_name(&parts).unwrap().to_owned()
        } else {
            let mut symbols = self.tables.get(self.module).cloned().unwrap_or_default();
            symbols.extend(self.symbols());
            let unknown_glob = symbols.get("$external_globs").is_some_and(|targets| targets.iter().any(|target| matches!(target, Target::External(path) if !path.first().is_some_and(|p| p == "std" || p == "core" || p == "alloc"))));
            if unknown_glob
                || parts
                    .first()
                    .is_some_and(|name| name == "json" || name == "Token")
            {
                self.diagnostics.insert(format!(
                    "{}: macro provider cannot be established: {}",
                    self.module,
                    parts.join("::")
                ));
                return;
            }
            parts.last().cloned().unwrap_or_default()
        };
        if name == "json" {
            if let Ok(expressions) = crate::json::expressions(mac.tokens.clone()) {
                for expr in &expressions {
                    self.visit_expr(expr);
                }
                return;
            }
            self.diagnostics.insert(format!(
                "{}: unsupported serde_json::json! argument grammar",
                self.module
            ));
            return;
        }
        if name == "Token" {
            return;
        }
        if name == "matches" {
            struct Matches(syn::Expr, syn::Pat, Option<syn::Expr>);
            impl syn::parse::Parse for Matches {
                fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
                    let expr = input.parse()?;
                    input.parse::<syn::Token![,]>()?;
                    let pat = syn::Pat::parse_multi_with_leading_vert(input)?;
                    let guard = if input.peek(syn::Token![if]) {
                        input.parse::<syn::Token![if]>()?;
                        Some(input.parse()?)
                    } else {
                        None
                    };
                    if input.peek(syn::Token![,]) {
                        input.parse::<syn::Token![,]>()?;
                    }
                    Ok(Self(expr, pat, guard))
                }
            }
            if let Ok(Matches(expr, pat, guard)) = syn::parse2(mac.tokens.clone()) {
                self.visit_expr(&expr);
                self.scopes.push(Symbols::new());
                self.visit_pat(&pat);
                self.bind_pattern(&pat);
                if let Some(guard) = guard {
                    self.visit_expr(&guard);
                }
                self.scopes.pop();
                return;
            }
        }
        if matches!(
            name.as_str(),
            "assert"
                | "assert_eq"
                | "assert_ne"
                | "debug_assert"
                | "debug_assert_eq"
                | "debug_assert_ne"
                | "format"
                | "format_args"
                | "print"
                | "println"
                | "eprint"
                | "eprintln"
                | "panic"
                | "write"
                | "writeln"
                | "dbg"
                | "vec"
                | "todo"
                | "unimplemented"
        ) {
            use syn::parse::Parser;
            let parser = syn::punctuated::Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated;
            if let Ok(args) = parser.parse2(mac.tokens.clone()) {
                for arg in &args {
                    self.visit_expr(arg);
                }
                return;
            }
            if name == "vec" {
                struct Repeat(syn::Expr, syn::Expr);
                impl syn::parse::Parse for Repeat {
                    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
                        let value = input.parse()?;
                        input.parse::<syn::Token![;]>()?;
                        let count = input.parse()?;
                        Ok(Self(value, count))
                    }
                }
                if let Ok(Repeat(value, count)) = syn::parse2(mac.tokens.clone()) {
                    self.visit_expr(&value);
                    self.visit_expr(&count);
                    return;
                }
            }
        } else if matches!(
            name.as_str(),
            "stringify"
                | "concat"
                | "env"
                | "option_env"
                | "file"
                | "line"
                | "column"
                | "module_path"
                | "cfg"
                | "include_str"
                | "include_bytes"
        ) {
            return;
        }
        self.diagnostics.insert(format!(
            "{}: macro token analysis is unsupported: {name}",
            self.module
        ));
    }
}

/// Macro invocation path as written, joined with `::` and without a leading `::`.
fn macro_path_text(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

/// Targets a macro invocation path resolves to in the macro namespace.
fn local_macro_targets(
    module: &str,
    path: &syn::Path,
    tables: &Tables,
    textual: &Tables,
    local: Option<&Symbols>,
) -> BTreeSet<Target> {
    let mut parts = path
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect::<Vec<_>>();
    if path.leading_colon.is_some() {
        parts.insert(0, String::new());
    }
    let targets = resolve_path(module, &parts, tables, local, Some(Namespace::Macro));
    if is_local_macro(&targets) || parts.len() != 1 || path.leading_colon.is_some() {
        return targets;
    }
    // Bare names also see textual-scope macro_rules of this module and its ancestors.
    let mut scope = Some(module);
    while let Some(current) = scope {
        if let Some(found) = textual.get(current).and_then(|t| t.get(&parts[0])) {
            return found.clone();
        }
        scope = current.rsplit_once("::").map(|(parent, _)| parent);
    }
    targets
}

fn is_local_macro(targets: &BTreeSet<Target>) -> bool {
    targets
        .iter()
        .any(|target| matches!(target, Target::Item(_, Namespace::Macro)))
}

/// Modules containing a trusted item-position invocation of a crate-local macro.
/// Its expansion may define items, so unresolved names there are references to the module.
pub(crate) fn opaque_modules(
    sources: &[loader::Source],
    tables: &Tables,
    textual: &Tables,
    options: &AnalysisOptions,
) -> BTreeSet<String> {
    let mut opaque = BTreeSet::new();
    for source in sources {
        for item in &source.items {
            let Item::Macro(item) = item else { continue };
            if item.mac.path.is_ident("macro_rules")
                || !options
                    .trusted_macros
                    .contains(&macro_path_text(&item.mac.path))
                || !matches!(cfg::enabled(&item.attrs, options), Ok(true))
            {
                continue;
            }
            if is_local_macro(&local_macro_targets(
                &source.module.id,
                &item.mac.path,
                tables,
                textual,
                None,
            )) {
                opaque.insert(source.module.id.clone());
            }
        }
    }
    opaque
}
