//! Role: 有効なRust構文を走査し、名前解決済みのモジュール参照を収集する。
use crate::{
    ast, cfg, loader,
    macros::{self, Arguments, MacroGrammar},
    model::{Env, Reference},
    symbols::{
        Block, Import, LocalBinding, Modules, NAMESPACES, Namespace, Scope, Target, Textual,
        declarations, use_imports,
    },
    thread_local_items,
};
use std::{collections::BTreeSet, path::Path};
use syn::{
    Item,
    spanned::Spanned,
    visit::{self, Visit},
};

/// Where an item is: directly in a module, or inside a block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Context {
    Module,
    Block,
}

pub(crate) struct References<'a> {
    /// The index of the module being visited.
    pub(crate) module: usize,
    pub(crate) modules: Modules<'a>,
    /// Textual-scope `macro_rules!` names per module.
    pub(crate) textual: &'a Textual,
    /// The file of the module, which its cfg errors are located in.
    pub(crate) file: &'a Path,
    pub(crate) env: Env<'a>,
    /// Modules whose trusted crate-local item macros may define items we cannot see.
    pub(crate) opaque: &'a BTreeSet<usize>,
    /// The blocks and generic parameters around the node being visited, innermost last.
    pub(crate) blocks: Vec<Block>,
    pub(crate) edges: &'a mut BTreeSet<Reference>,
    pub(crate) diagnostics: &'a mut BTreeSet<String>,
}

impl References<'_> {
    fn id(&self) -> String {
        self.modules.layout.ids[self.module].clone()
    }

    /// Evaluates the attributes of an expression, statement, or associated item once: the cfg
    /// predicates decide whether the node is visited, and an enabled node's expansions are reported.
    /// Module items are not passed here; the loader evaluated them.
    pub(crate) fn enabled(&mut self, attrs: &[syn::Attribute]) -> bool {
        // Most expression and statement nodes have no attributes. They cannot
        // disable code or require expansion.
        if attrs.is_empty() {
            return true;
        }
        let metas = match cfg::effective(attrs, self.env) {
            Ok(metas) => metas,
            Err(error) => {
                self.cfg_error(attrs, &error);
                return false;
            }
        };
        match cfg::enabled_metas(&metas, self.env) {
            Ok(true) => {
                self.expansions(&metas);
                true
            }
            Ok(false) => false,
            Err(error) => {
                self.cfg_error(attrs, &error);
                false
            }
        }
    }

    /// Reports an invalid cfg, located at its attribute, and so disables the node.
    fn cfg_error(&mut self, attrs: &[syn::Attribute], error: &str) {
        let line = attrs
            .iter()
            .find(|attr| attr.path().is_ident("cfg") || attr.path().is_ident("cfg_attr"))
            .or(attrs.first())
            .map_or(1, |attr| attr.span().start().line);
        let message = loader::located(self.file, line, error);
        self.diagnostics.insert(message);
    }

    /// Reports the attribute expansions that this analysis does not model. The attributes
    /// are the expanded ones.
    pub(crate) fn expansions(&mut self, metas: &[syn::Meta]) {
        if metas.is_empty() {
            return;
        }
        let intrinsic = if metas.iter().any(|meta| meta.path().is_ident("derive")) {
            self.intrinsic_derives()
        } else {
            BTreeSet::new()
        };
        let id = self.id();
        match cfg::expansion_diagnostics(metas, self.env, &intrinsic) {
            Ok(diagnostics) => {
                for diagnostic in diagnostics {
                    let message = format!("{id}: {diagnostic}");
                    self.diagnostics.insert(message);
                }
            }
            Err(error) => {
                let message = format!("{id}: {error}");
                self.diagnostics.insert(message);
            }
        }
    }

    /// The module's scope and the blocks' scopes: the names that an import or a path may see.
    fn visible_scopes(&self) -> impl Iterator<Item = &Scope> {
        self.blocks
            .iter()
            .map(|block| &block.items)
            .chain(self.modules.scope(self.module))
    }

    /// Whether an unknown external glob import is visible from here.
    fn unknown_glob_visible(&self) -> bool {
        self.visible_scopes()
            .any(|scope| !scope.external_globs.is_empty())
    }

    /// The derives that are the standard ones, for the names visible here.
    fn intrinsic_derives(&self) -> BTreeSet<String> {
        let mut intrinsic = if self.unknown_glob_visible() {
            BTreeSet::new()
        } else {
            crate::intrinsic::DERIVES
                .iter()
                .map(|(name, _)| name.to_string())
                .collect()
        };
        for name in self.visible_names() {
            let external = NAMESPACES
                .iter()
                .flat_map(|namespace| {
                    self.modules
                        .lookup(self.module, &self.blocks, *namespace, &name)
                })
                .filter_map(|target| match target {
                    Target::External(path) => Some(path),
                    _ => None,
                })
                .collect::<Vec<_>>();
            if external.is_empty() {
                continue;
            }
            if external.iter().all(|path| crate::intrinsic::derive(path)) {
                intrinsic.insert(name);
            } else {
                intrinsic.remove(&name);
            }
        }
        intrinsic
    }

    /// Every name that a visible scope or block binds, in any namespace.
    fn visible_names(&self) -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        for scope in self.visible_scopes() {
            names.extend(scope.declared.keys().map(|(_, name)| name.clone()));
            names.extend(scope.explicit.keys().cloned());
            names.extend(scope.globbed.keys().map(|(_, name)| name.clone()));
        }
        for block in &self.blocks {
            names.extend(block.locals.keys().map(|(_, name)| name.clone()));
        }
        names
    }

    fn edge(&mut self, targets: BTreeSet<Target>) {
        let owners = targets
            .iter()
            .filter_map(Target::owner)
            .collect::<BTreeSet<_>>();
        let ambiguous = [Namespace::Type, Namespace::Value]
            .iter()
            .any(|namespace| targets.iter().filter(|t| t.matches(*namespace)).count() > 1);
        if owners.len() > 1 && ambiguous {
            let message = format!(
                "{}: ambiguous local path resolution: {targets:?}",
                self.id()
            );
            self.diagnostics.insert(message);
            return;
        }
        self.add_edges(&owners);
    }

    /// Records an edge from this module to each owner that is another module.
    fn add_edges(&mut self, owners: &BTreeSet<usize>) {
        let from = self.id();
        for owner in owners {
            if *owner == self.module {
                continue;
            }
            if let Some(to) = self.modules.layout.ids.get(*owner) {
                let to = to.clone();
                self.edges.insert(Reference {
                    from: from.clone(),
                    to,
                });
            }
        }
    }

    /// Opaque modules that a path unresolved in the symbol tables passes through.
    fn opaque_owners(&self, parts: &[String]) -> BTreeSet<usize> {
        let mut owners = BTreeSet::new();
        for k in 1..parts.len() {
            if parts[k] == "super" {
                continue;
            }
            for target in self.modules.resolve(
                self.module,
                &self.blocks,
                &parts[..k],
                Some(Namespace::Type),
            ) {
                if let Target::Module(owner) = target
                    && self.opaque.contains(&owner)
                    && !self
                        .modules
                        .scope(owner)
                        .is_some_and(|scope| scope.binds_any(&parts[k]))
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
            self.edge(BTreeSet::from([Target::Module(*owner)]));
        }
        !owners.is_empty()
    }

    /// Declares a generic parameter (or `Self`) of the innermost block: it hides the names of
    /// both namespaces.
    fn declare_generic(&mut self, ident: &syn::Ident) {
        let name = ident.to_string();
        if let Some(block) = self.blocks.last_mut() {
            block
                .locals
                .insert((Namespace::Type, name.clone()), LocalBinding::Generic);
            block
                .locals
                .insert((Namespace::Value, name), LocalBinding::Generic);
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
            if let Some(block) = self.blocks.last_mut() {
                block
                    .locals
                    .insert((Namespace::Value, ident.to_string()), LocalBinding::Value);
            }
        }
    }

    fn generic_scope(&mut self, generics: &syn::Generics) {
        self.blocks.push(Block::default());
        for parameter in &generics.params {
            match parameter {
                syn::GenericParam::Type(p) => self.declare_generic(&p.ident),
                syn::GenericParam::Const(p) => self.declare_generic(&p.ident),
                _ => {}
            }
        }
    }

    /// The innermost block's scope for its `use` items.
    fn block_items(&mut self) -> &mut Scope {
        &mut self
            .blocks
            .last_mut()
            .expect("a block scope is open for a block-level use")
            .items
    }

    /// Records the edges of one import, and the diagnostic when a local path is unresolved.
    fn import_edges(&mut self, import: &Import, targets: BTreeSet<Target>) {
        let local = import
            .path
            .first()
            .is_some_and(|first| matches!(first.as_str(), "crate" | "self" | "super"));
        if targets.is_empty() && local && !self.opaque_reference(&import.path) {
            let message = format!(
                "{}: unresolved explicit local import: {}",
                self.id(),
                import.path.join("::")
            );
            self.diagnostics.insert(message);
        }
        // A glob import names every module it resolves to, so it is not ambiguous.
        if import.glob {
            let owners = targets.iter().filter_map(Target::owner).collect();
            self.add_edges(&owners);
        } else {
            self.edge(targets);
        }
    }

    /// The `use` items of a module, which were applied to its scope before the visit. Their
    /// edges and unresolved paths are reported here.
    fn module_use(&mut self, item: &syn::ItemUse) {
        for import in use_imports(item) {
            let targets = self
                .modules
                .import_targets(self.module, &self.blocks, &import, true);
            self.import_edges(&import, targets);
        }
    }

    /// Applies the `use` items of a block, once and in order. The block's names are declared
    /// first, so that a name is visible throughout the block.
    fn block_uses(&mut self, items: &[&syn::ItemUse]) {
        let imports = items
            .iter()
            .flat_map(|item| use_imports(item))
            .collect::<Vec<_>>();
        for import in imports.iter().filter(|import| !import.glob) {
            if let Some(alias) = &import.alias {
                self.block_items()
                    .explicit
                    .entry(alias.clone())
                    .or_default();
            }
        }
        for import in imports {
            let targets = self
                .modules
                .import_targets(self.module, &self.blocks, &import, true);
            if import.glob {
                let glob = self.modules.glob(&targets, self.env.settings);
                self.block_items().add_glob(glob);
            } else if let Some(alias) = &import.alias {
                self.block_items()
                    .explicit
                    .entry(alias.clone())
                    .or_default()
                    .extend(targets.iter().cloned());
            }
            self.import_edges(&import, targets);
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

    /// Records the references of a path in one namespace, and visits its qualified self and
    /// the generic arguments of its segments.
    fn path_in(&mut self, qself: Option<&syn::QSelf>, path: &syn::Path, namespace: Namespace) {
        if let Some(qself) = qself {
            self.visit_type(&qself.ty);
        }
        if path.leading_colon.is_none() {
            self.reference(path, namespace);
        }
        for segment in &path.segments {
            self.visit_path_arguments(&segment.arguments);
        }
    }

    /// Resolves a path in one namespace, and records its edge or its unresolved-path diagnostic.
    fn reference(&mut self, path: &syn::Path, namespace: Namespace) {
        let parts = path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect::<Vec<_>>();
        let targets = self
            .modules
            .resolve(self.module, &self.blocks, &parts, Some(namespace));
        if targets.is_empty() && self.opaque_reference(&parts) {
            // Names defined by a trusted local item macro belong to its module.
        } else if targets.is_empty()
            && parts
                .first()
                .is_some_and(|p| matches!(p.as_str(), "crate" | "self" | "super"))
        {
            let message = format!(
                "{}: unresolved explicit local path: {}",
                self.id(),
                parts.join("::")
            );
            self.diagnostics.insert(message);
        }
        self.edge(targets);
    }

    /// Visits a module-level item. Its module's `use` items are reported here.
    pub(crate) fn module_item(&mut self, item: &Item) {
        self.item(item, Context::Module);
    }

    fn item(&mut self, item: &Item, context: Context) {
        match item {
            // Child modules are visited separately in their own resolution environment.
            Item::Mod(module) => {
                if context == Context::Block {
                    let message = format!(
                        "{}: block-local module {} is unsupported",
                        self.id(),
                        module.ident
                    );
                    self.diagnostics.insert(message);
                }
                return;
            }
            // A block's `use` items are applied when the block begins, once.
            Item::Use(item) => {
                if context == Context::Module {
                    self.module_use(item);
                }
                return;
            }
            _ => {}
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
            self.declare_generic(&syn::Ident::new("Self", proc_macro2::Span::call_site()));
        }
        visit::visit_item(self, item);
        if generics.is_some() {
            self.blocks.pop();
        }
    }

    /// Visits the statics of a standard `thread_local!` body: their types and initializers.
    fn standard_statics(&mut self, tokens: &proc_macro2::TokenStream) {
        let Ok(statics) = thread_local_items::statics(tokens.clone()) else {
            return;
        };
        for item in &statics {
            if self.enabled(&item.attrs) {
                self.visit_type(&item.ty);
                self.visit_expr(&item.expr);
            }
        }
    }

    /// Records the edges of a call whose macro is local, and reports it unless the macro is trusted.
    fn local_macro_call(&mut self, mac: &syn::Macro, local_macro: BTreeSet<Target>) {
        self.edge(local_macro);
        let path = ast::path_text(&mac.path);
        if !self.env.settings.trusted_macros.contains(&path) {
            let message = format!(
                "{}: local macro expansion is unsupported: {path}",
                self.id()
            );
            self.diagnostics.insert(message);
        }
    }

    /// The name of the built-in macro that a call expands with. Reports a diagnostic and returns
    /// `None` when the provider is unsupported or cannot be established.
    fn provider_name(
        &mut self,
        mac: &syn::Macro,
        parts: &[String],
        local_macro: &BTreeSet<Target>,
    ) -> Option<String> {
        let external = local_macro
            .iter()
            .filter_map(|target| match target {
                Target::External(path) => Some(path),
                _ => None,
            })
            .collect::<Vec<_>>();
        if !external.is_empty() {
            let names = external
                .iter()
                .filter_map(|path| crate::intrinsic::macro_name(path))
                .collect::<BTreeSet<_>>();
            if names.len() != 1
                || external
                    .iter()
                    .any(|path| crate::intrinsic::macro_name(path).is_none())
            {
                let message = format!(
                    "{}: unsupported macro provider: {}",
                    self.id(),
                    parts.join("::")
                );
                self.diagnostics.insert(message);
                return None;
            }
            return Some(names.into_iter().next().unwrap().to_owned());
        }
        if parts.len() > 1 {
            let known_local = mac.path.leading_colon.is_none()
                && self
                    .modules
                    .scope(self.module)
                    .is_some_and(|scope| scope.binds_any(&parts[0]));
            let name = crate::intrinsic::macro_name(parts);
            if known_local || name.is_none() {
                let message = format!(
                    "{}: qualified macro expansion is unsupported: {}",
                    self.id(),
                    parts.join("::")
                );
                self.diagnostics.insert(message);
                return None;
            }
            return name.map(str::to_owned);
        }
        if self.unknown_glob_visible() {
            let message = format!(
                "{}: macro provider cannot be established: {}",
                self.id(),
                parts.join("::")
            );
            self.diagnostics.insert(message);
            return None;
        }
        Some(parts.last().cloned().unwrap_or_default())
    }

    /// Visits the arguments of a built-in macro by its grammar. A name that the grammar table does
    /// not model reports a diagnostic, unless it has no arguments to visit.
    fn visit_named_macro(&mut self, mac: &syn::Macro, name: &str) {
        let modeled = match macros::grammar(name) {
            Some(grammar) => self.visit_grammar(mac, grammar),
            None => matches!(
                name,
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
            ),
        };
        if !modeled {
            let message = format!("{}: macro token analysis is unsupported: {name}", self.id());
            self.diagnostics.insert(message);
        }
    }

    /// Visits the arguments of a call by its grammar. Returns false when the tokens do not match it.
    fn visit_grammar(&mut self, mac: &syn::Macro, grammar: MacroGrammar) -> bool {
        if let MacroGrammar::Matches { trailing_format } = grammar {
            return self.visit_matches(mac, trailing_format);
        }
        match macros::arguments(mac.tokens.clone(), grammar) {
            Ok(arguments) => {
                self.visit_arguments(arguments);
                true
            }
            Err(_) => false,
        }
    }

    /// Visits a `matches!`-style call: its expression, its pattern and guard in a block of their
    /// own, then its format arguments. Returns false when the arguments do not parse.
    fn visit_matches(&mut self, mac: &syn::Macro, trailing_format: bool) -> bool {
        let Ok(arguments) = macros::matches_arguments(mac.tokens.clone(), trailing_format) else {
            return false;
        };
        self.visit_expr(&arguments.expr);
        self.blocks.push(Block::default());
        self.visit_pat(&arguments.pattern);
        self.bind_pattern(&arguments.pattern);
        if let Some(guard) = &arguments.guard {
            self.visit_expr(guard);
        }
        self.blocks.pop();
        self.visit_arguments(arguments.format);
        true
    }

    /// Visits the expressions of a call, then the names that its format string captures: each is a
    /// reference to a value of the scope.
    fn visit_arguments(&mut self, arguments: Arguments) {
        for expr in &arguments.exprs {
            self.visit_expr(expr);
        }
        for name in &arguments.captures {
            if let Some(path) = macros::capture_path(name) {
                self.reference(&path, Namespace::Value);
            }
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
        match pat {
            syn::Pat::Path(pat) => self.path_in(pat.qself.as_ref(), &pat.path, Namespace::Value),
            syn::Pat::TupleStruct(pat) => {
                self.path_in(pat.qself.as_ref(), &pat.path, Namespace::Value);
                for elem in &pat.elems {
                    self.visit_pat(elem);
                }
            }
            syn::Pat::Struct(pat) => {
                self.path_in(pat.qself.as_ref(), &pat.path, Namespace::Type);
                for field in &pat.fields {
                    self.visit_pat(&field.pat);
                }
            }
            _ => visit::visit_pat(self, pat),
        }
    }
    fn visit_expr_struct(&mut self, expr: &'ast syn::ExprStruct) {
        self.path_in(expr.qself.as_ref(), &expr.path, Namespace::Type);
        for field in &expr.fields {
            self.visit_field_value(field);
        }
        if let Some(rest) = &expr.rest {
            self.visit_expr(rest);
        }
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
    /// A block-level item. Module items go through `module_item`.
    fn visit_item(&mut self, item: &'ast Item) {
        self.item(item, Context::Block);
    }
    fn visit_path(&mut self, path: &'ast syn::Path) {
        self.path_in(None, path, Namespace::Type);
    }
    fn visit_expr_path(&mut self, path: &'ast syn::ExprPath) {
        self.path_in(path.qself.as_ref(), &path.path, Namespace::Value);
    }
    fn visit_type_path(&mut self, path: &'ast syn::TypePath) {
        self.path_in(path.qself.as_ref(), &path.path, Namespace::Type);
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
        self.blocks.pop();
    }
    // The attributes of an associated item were evaluated by visit_impl_item and visit_trait_item.
    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        self.generic_scope(&item.sig.generics);
        self.visit_signature(&item.sig);
        for arg in &item.sig.inputs {
            if let syn::FnArg::Typed(arg) = arg {
                self.bind_pattern(&arg.pat);
            }
        }
        self.visit_block(&item.block);
        self.blocks.pop();
    }
    fn visit_trait_item_fn(&mut self, item: &'ast syn::TraitItemFn) {
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
        self.blocks.pop();
    }
    fn visit_block(&mut self, block: &'ast syn::Block) {
        self.blocks.push(Block::default());
        // Each block item's attributes are evaluated once here; a disabled item is skipped entirely.
        let active = block
            .stmts
            .iter()
            .map(|statement| match statement {
                syn::Stmt::Item(item) => self.enabled(ast::attrs(item)),
                _ => true,
            })
            .collect::<Vec<_>>();
        let live = block
            .stmts
            .iter()
            .zip(&active)
            .filter(|(_, on)| **on)
            .map(|(statement, _)| statement)
            .collect::<Vec<_>>();
        // Block items are visible throughout the block, including before their declaration.
        for statement in &live {
            if let syn::Stmt::Item(item) = statement
                && !matches!(item, Item::Mod(_))
            {
                for (namespace, name, target) in
                    declarations(item, self.module, &self.modules.layout.ids)
                {
                    if let Some(block) = self.blocks.last_mut() {
                        block
                            .locals
                            .insert((namespace, name), LocalBinding::Item(target));
                    }
                }
            }
        }
        let uses = live
            .iter()
            .filter_map(|statement| match statement {
                syn::Stmt::Item(Item::Use(item)) => Some(item),
                _ => None,
            })
            .collect::<Vec<_>>();
        self.block_uses(&uses);
        for statement in live {
            // The block's `use` items were applied above.
            if !matches!(statement, syn::Stmt::Item(Item::Use(_))) {
                self.visit_stmt(statement);
            }
        }
        self.blocks.pop();
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
        self.blocks.push(Block::default());
        for pat in &expr.inputs {
            self.visit_pat(pat);
            self.bind_pattern(pat);
        }
        self.visit_return_type(&expr.output);
        self.visit_expr(&expr.body);
        self.blocks.pop();
    }
    fn visit_expr_for_loop(&mut self, expr: &'ast syn::ExprForLoop) {
        self.visit_expr(&expr.expr);
        self.blocks.push(Block::default());
        self.visit_pat(&expr.pat);
        self.bind_pattern(&expr.pat);
        self.visit_block(&expr.body);
        self.blocks.pop();
    }
    fn visit_expr_if(&mut self, expr: &'ast syn::ExprIf) {
        self.blocks.push(Block::default());
        self.condition(&expr.cond);
        self.visit_block(&expr.then_branch);
        self.blocks.pop();
        if let Some((_, otherwise)) = &expr.else_branch {
            self.visit_expr(otherwise);
        }
    }
    fn visit_expr_while(&mut self, expr: &'ast syn::ExprWhile) {
        self.blocks.push(Block::default());
        self.condition(&expr.cond);
        self.visit_block(&expr.body);
        self.blocks.pop();
    }
    fn visit_arm(&mut self, arm: &'ast syn::Arm) {
        if !self.enabled(&arm.attrs) {
            return;
        }
        self.blocks.push(Block::default());
        self.visit_pat(&arm.pat);
        self.bind_pattern(&arm.pat);
        if let Some((_, guard)) = &arm.guard {
            self.visit_expr(guard);
        }
        self.visit_expr(&arm.body);
        self.blocks.pop();
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
            .env
            .settings
            .trusted_macros
            .contains(&ast::path_text(&item.mac.path))
        {
            let local_macro = local_macro_targets(
                &self.modules,
                self.textual,
                self.module,
                &item.mac.path,
                &self.blocks,
            );
            if is_local_macro(&local_macro) {
                self.edge(local_macro);
            } else if thread_local_items::provider(
                &item.mac.path,
                &self.modules.layout.uses[self.module],
            )
            .is_some()
            {
                self.standard_statics(&item.mac.tokens);
            }
            return;
        }
        let message = format!(
            "{}: item macro expansion is unsupported: {}",
            self.id(),
            item.mac
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>()
                .join("::")
        );
        self.diagnostics.insert(message);
    }
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        let parts = mac
            .path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect::<Vec<_>>();
        let local_macro = local_macro_targets(
            &self.modules,
            self.textual,
            self.module,
            &mac.path,
            &self.blocks,
        );
        if is_local_macro(&local_macro) {
            self.local_macro_call(mac, local_macro);
            return;
        }
        self.path_in(None, &mac.path, Namespace::Macro);
        if self
            .env
            .settings
            .trusted_macros
            .contains(&ast::path_text(&mac.path))
        {
            return;
        }
        if let Some(name) = self.provider_name(mac, &parts, &local_macro) {
            self.visit_named_macro(mac, &name);
        }
    }
}

/// Targets a macro invocation path resolves to in the macro namespace.
pub(crate) fn local_macro_targets(
    modules: &Modules,
    textual: &Textual,
    module: usize,
    path: &syn::Path,
    blocks: &[Block],
) -> BTreeSet<Target> {
    let mut parts = path
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect::<Vec<_>>();
    if path.leading_colon.is_some() {
        parts.insert(0, String::new());
    }
    let targets = modules.resolve(module, blocks, &parts, Some(Namespace::Macro));
    if is_local_macro(&targets) || parts.len() != 1 || path.leading_colon.is_some() {
        return targets;
    }
    // Bare names also see textual-scope macro_rules of this module and its ancestors.
    let mut scope = Some(module);
    while let Some(current) = scope {
        if let Some(found) = textual.get(current).and_then(|table| table.get(&parts[0])) {
            return found.clone();
        }
        scope = modules.layout.parents.get(current).copied().flatten();
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
    modules: &Modules,
    textual: &Textual,
    env: Env,
) -> BTreeSet<usize> {
    let mut opaque = BTreeSet::new();
    for (module, source) in sources.iter().enumerate() {
        for (item, _) in source.items() {
            let Item::Macro(item) = item else { continue };
            if item.mac.path.is_ident("macro_rules")
                || !env
                    .settings
                    .trusted_macros
                    .contains(&ast::path_text(&item.mac.path))
            {
                continue;
            }
            if is_local_macro(&local_macro_targets(
                modules,
                textual,
                module,
                &item.mac.path,
                &[],
            )) {
                opaque.insert(module);
            }
        }
    }
    opaque
}
