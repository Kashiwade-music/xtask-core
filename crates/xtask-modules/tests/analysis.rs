//! Role: モジュール読込み、名前解決、cfg別の参照と循環を検証する。
use std::{collections::BTreeSet, fs};
use xtask_modules::{AnalysisOptions, ModuleGraph, analyze};

fn graph(files: &[(&str, &str)], options: AnalysisOptions) -> ModuleGraph {
    let directory = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    for (path, source) in files {
        let path = directory.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, with_fixture_roles(source)).unwrap();
    }
    analyze(&directory.path().join("lib.rs"), &options).unwrap()
}

// Existing reference-resolution fixtures need valid module annotations so their
// diagnostic assertions remain focused on reference analysis. This helper is
// deliberately local to these fixtures; role-policy tests write source verbatim.
fn with_fixture_roles(source: &str) -> String {
    use syn::visit::{self, Visit};

    struct InlineModules(Vec<proc_macro2::LineColumn>);
    impl<'ast> Visit<'ast> for InlineModules {
        fn visit_item_mod(&mut self, module: &'ast syn::ItemMod) {
            if let Some((brace, _)) = &module.content {
                self.0.push(brace.span.open().end());
            }
            visit::visit_item_mod(self, module);
        }
    }
    let Ok(file) = syn::parse_file(source) else {
        // Malformed-source regression fixtures must retain their original text.
        return source.to_owned();
    };
    let mut modules = InlineModules(Vec::new());
    modules.visit_file(&file);
    let mut positions: Vec<_> = modules
        .0
        .iter()
        .map(|location| {
            let start: usize = source
                .split_inclusive('\n')
                .take(location.line - 1)
                .map(str::len)
                .sum();
            start
                + source[start..]
                    .char_indices()
                    .nth(location.column)
                    .map_or(source.len() - start, |(offset, _)| offset)
        })
        .collect();
    positions.sort_unstable();
    let mut annotated = source.to_owned();
    for position in positions.into_iter().rev() {
        annotated.insert_str(
            position,
            "\n//! Role: Exercise module reference analysis.\n",
        );
    }
    format!("//! Role: Exercise module reference analysis.\n{annotated}")
}

fn edges(graph: &ModuleGraph) -> BTreeSet<(&str, &str)> {
    graph
        .edges
        .iter()
        .map(|r| (r.from.as_str(), r.to.as_str()))
        .collect()
}

#[test]
fn fixture_role_insertion_preserves_unicode_before_inline_braces() {
    let result = graph(
        &[(
            "lib.rs",
            "const TEXT: &str = \"寿\"; mod inner { pub fn f() {} }",
        )],
        AnalysisOptions::default(),
    );
    assert_eq!(result.modules.len(), 2);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}

#[test]
fn hierarchy_is_not_a_reference_and_expression_paths_count() {
    let result = graph(
        &[
            ("lib.rs", "mod a; mod b;"),
            ("a.rs", "pub fn f() { crate::b::g(); }"),
            ("b.rs", "pub fn g() { super::a::f(); }"),
        ],
        AnalysisOptions::default(),
    );
    assert_eq!(result.modules.len(), 3);
    assert_eq!(
        edges(&result),
        BTreeSet::from([("crate::a", "crate::b"), ("crate::b", "crate::a")])
    );
    assert_eq!(result.cycles, vec![vec!["crate::a", "crate::b"]]);
}

#[test]
fn aliases_globs_and_chained_reexports_resolve_to_owners() {
    let result = graph(
        &[(
            "lib.rs",
            r#"
        mod origin { pub struct Thing; pub fn f() {} }
        mod api { pub use crate::origin::{Thing as Alias, f}; }
        mod facade { pub use crate::api::*; }
        mod consumer { use crate::facade::{self as public, Alias}; fn f(_: Alias) { public::f(); } }
    "#,
        )],
        AnalysisOptions::default(),
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(edges(&result).contains(&("crate::consumer", "crate::origin")));
    assert!(edges(&result).contains(&("crate::consumer", "crate::facade")));
    assert!(edges(&result).contains(&("crate::facade", "crate::api")));
}

#[test]
fn cfg_and_cfg_attr_select_one_module_graph() {
    let files = [
        (
            "lib.rs",
            r#"
        #[cfg(feature="left")] #[path="left.rs"] mod selected;
        #[cfg(not(feature="left"))] #[cfg_attr(unix, path="right.rs")] mod selected;
        mod target { pub fn call() {} }
    "#,
        ),
        ("left.rs", "pub fn f() { crate::target::call(); }"),
        ("right.rs", "pub fn f() {}"),
    ];
    let mut options = AnalysisOptions::default();
    options.cfg.insert("unix".into());
    assert!(graph(&files, options.clone()).edges.is_empty());
    options.features.insert("left".into());
    assert_eq!(
        edges(&graph(&files, options)),
        BTreeSet::from([("crate::selected", "crate::target")])
    );
}

#[test]
fn inline_and_out_of_line_module_directories() {
    let result = graph(
        &[
            (
                "lib.rs",
                "mod a; #[path=\"custom\"] mod inline { mod child; }",
            ),
            ("a.rs", "mod child;"),
            ("a/child.rs", ""),
            ("custom/child.rs", ""),
        ],
        AnalysisOptions::default(),
    );
    assert_eq!(result.modules.len(), 5);
    assert!(result.edges.is_empty());
}

#[test]
fn scopes_aliases_and_generic_parameters_do_not_create_false_edges() {
    let result = graph(
        &[(
            "lib.rs",
            r#"
        mod source { pub struct T; pub fn f() {} }
        mod consumer {
            use crate::source as source;
            fn local<source>() { source::method(); }
            fn f() {
                { use crate::source as inner; inner::f(); }
                inner::f();
                let source = 0;
                source::f(); // values do not hide modules in the type namespace
            }
        }
    "#,
        )],
        AnalysisOptions::default(),
    );
    assert_eq!(
        edges(&result),
        BTreeSet::from([("crate::consumer", "crate::source")])
    );
    assert!(result.diagnostics.is_empty());
}

#[test]
fn macros_are_parsed_only_when_their_grammar_is_known() {
    let result = graph(
        &[(
            "lib.rs",
            r#"
        mod source { pub fn f() -> usize { 1 } }
        mod consumer { fn f() { assert_eq!(crate::source::f(), 1); let _ = vec![crate::source::f(); 2]; unknown!(crate::source::f()); } }
    "#,
        )],
        AnalysisOptions::default(),
    );
    assert_eq!(
        edges(&result),
        BTreeSet::from([("crate::consumer", "crate::source")])
    );
    assert_eq!(result.diagnostics.len(), 1);
}

#[test]
fn include_and_block_modules_cannot_silently_pass() {
    let result = graph(
        &[(
            "lib.rs",
            "include!(\"generated.rs\"); fn f() { mod hidden {} }",
        )],
        AnalysisOptions::default(),
    );
    assert_eq!(result.diagnostics.len(), 2);
}

#[test]
fn explicit_imports_override_ambiguous_globs() {
    let result = graph(
        &[(
            "lib.rs",
            r#"
        mod a { pub struct Thing; } mod b { pub struct Thing; }
        mod c { use crate::a::*; use crate::b::*; use crate::a::Thing; fn f(_: Thing) {} }
    "#,
        )],
        AnalysisOptions::default(),
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}

#[test]
fn namespaces_distinguish_function_and_struct_imports() {
    let result = graph(
        &[(
            "lib.rs",
            r#"
        mod a {pub fn Foo() {}} mod b {pub struct Foo {}}
        mod c {use crate::a::Foo; use crate::b::Foo; fn f(){let _: Foo; Foo();}}
    "#,
        )],
        AnalysisOptions::default(),
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        edges(&result),
        BTreeSet::from([("crate::c", "crate::a"), ("crate::c", "crate::b")])
    );
}

#[test]
fn absolute_external_use_does_not_bind_same_named_local_module() {
    let result = graph(
        &[(
            "lib.rs",
            r#"
        mod std {pub mod mem {pub fn drop<T>(_:T){crate::a::f();}}}
        mod a {use ::std::mem::drop; pub fn f(){drop(1)}}
    "#,
        )],
        AnalysisOptions::default(),
    );
    assert!(result.cycles.is_empty());
    assert_eq!(
        edges(&result),
        BTreeSet::from([("crate::std::mem", "crate::a")])
    );
}

#[test]
fn explicit_path_files_search_children_in_the_physical_parent() {
    let result = graph(
        &[
            ("lib.rs", "mod a;"),
            ("a.rs", "#[path=\"alternate.rs\"] mod renamed;"),
            ("alternate.rs", "mod child;"),
            ("child.rs", ""),
        ],
        AnalysisOptions::default(),
    );
    assert_eq!(result.modules.len(), 4);
    assert!(result.edges.is_empty());
}

#[test]
fn cfg_filters_every_ast_reference_container() {
    let result = graph(
        &[(
            "lib.rs",
            r#"
        mod a { pub struct T; pub fn f()->usize {1} }
        mod b {
            struct S;
            impl S { #[cfg(off)] const X: usize = crate::a::f(); #[cfg(off)] type T = crate::a::T; }
            trait Tr { #[cfg(off)] const X: usize = crate::a::f(); #[cfg(off)] type T: crate::a::T; }
            fn f(){ #[cfg(off)] crate::a::f(); #[cfg(off)] assert_eq!(crate::a::f(), 1); }
        }
    "#,
        )],
        AnalysisOptions::default(),
    );
    assert!(result.edges.is_empty(), "{:?}", result.edges);
}

#[test]
fn custom_expansions_require_explicit_trust() {
    let files = [("lib.rs", "#[custom::emit] #[derive(Custom)] struct S;")];
    let result = graph(&files, AnalysisOptions::default());
    assert_eq!(result.diagnostics.len(), 2);
    let mut options = AnalysisOptions::default();
    options.trusted_attributes.insert("custom::emit".into());
    options.trusted_derives.insert("Custom".into());
    assert!(graph(&files, options).diagnostics.is_empty());
}

#[test]
fn builtin_inert_attributes_and_derive_helpers_need_no_expansion() {
    let result = graph(
        &[(
            "lib.rs",
            "#[derive(Default)] enum E { #[default] A, B } #[windows_subsystem = \"windows\"] fn f() {}",
        )],
        AnalysisOptions::default(),
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}

#[test]
fn tool_attributes_need_no_expansion() {
    let result = graph(
        &[(
            "lib.rs",
            "#[clippy::msrv = \"1.90\"] #[rustfmt::skip] #[rust_analyzer::skip]              #[diagnostic::on_unimplemented(message = \"m\")] trait T {}",
        )],
        AnalysisOptions::default(),
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}

#[test]
fn unknown_attributes_remain_diagnostics() {
    let result = graph(
        &[("lib.rs", "#[my_attr] #[other::tool] struct S;")],
        AnalysisOptions::default(),
    );
    assert_eq!(result.diagnostics.len(), 2, "{:?}", result.diagnostics);
}

#[test]
fn cfg_disabled_file_has_no_ghost_node_or_edges() {
    let result = graph(
        &[
            ("lib.rs", "mod a; fn f(){a::foo();}"),
            ("a.rs", "#![cfg(off)] pub fn foo(){}"),
        ],
        AnalysisOptions::default(),
    );
    assert_eq!(result.modules.len(), 1);
    assert!(result.edges.is_empty());
}

#[test]
fn root_alias_and_nested_super_paths_find_owning_module() {
    let result = graph(
        &[(
            "lib.rs",
            r#"
        extern crate self as this;
        mod a { pub fn f(){} }
        mod b { mod c { fn f(){this::a::f(); super::super::a::f();} } }
    "#,
        )],
        AnalysisOptions::default(),
    );
    assert_eq!(
        edges(&result),
        BTreeSet::from([("crate::b::c", "crate::a")])
    );
}

#[test]
fn visibility_and_attribute_names_are_not_module_references() {
    let result = graph(
        &[(
            "lib.rs",
            r#"
        pub use a::T;
        mod cfg {}
        mod a { #[cfg(all())] pub(crate) struct T; }
    "#,
        )],
        AnalysisOptions::default(),
    );
    assert_eq!(edges(&result), BTreeSet::from([("crate", "crate::a")]));
    assert!(result.cycles.is_empty());
}

#[test]
fn unresolved_explicit_local_paths_are_diagnostic() {
    let result = graph(
        &[("lib.rs", "fn f(_: crate::missing::T) {}")],
        AnalysisOptions::default(),
    );
    assert_eq!(result.diagnostics.len(), 1);
}

#[test]
fn formatted_macro_patterns_guards_and_json_values_are_references() {
    let result = graph(
        &[(
            "lib.rs",
            r#"
        mod a {pub fn f()->usize {1} pub enum E {V}}
        mod b {fn f(){
            let _ = matches!(0, _ if crate::a::f() > 0);
            let _ = serde_json::json!({"outer":[{"value": crate::a::f()}]});
            todo!("{}", crate::a::f());
        }}
    "#,
        )],
        AnalysisOptions::default(),
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(edges(&result), BTreeSet::from([("crate::b", "crate::a")]));
}

#[test]
fn generic_type_name_hides_module_without_import_edges() {
    let result = graph(
        &[(
            "lib.rs",
            "mod a {pub fn f(){}} fn generic<a>() {a::method();}",
        )],
        AnalysisOptions::default(),
    );
    assert!(result.edges.is_empty());
}

#[test]
fn local_macros_do_not_inherit_builtin_argument_grammar() {
    let result = graph(
        &[(
            "lib.rs",
            "macro_rules! assert { ($($t:tt)*) => {} } fn f(){assert!(anything);} ",
        )],
        AnalysisOptions::default(),
    );
    assert_eq!(result.diagnostics.len(), 1);
}

#[test]
fn mutually_exclusive_cfg_expressions_do_not_form_a_cycle() {
    let result = graph(
        &[(
            "lib.rs",
            r#"
        mod a {pub fn f(){#[cfg(left)] crate::b::f();}}
        mod b {pub fn f(){#[cfg(not(left))] crate::a::f();}}
    "#,
        )],
        AnalysisOptions {
            cfg: BTreeSet::from(["left".into()]),
            ..Default::default()
        },
    );
    assert!(result.cycles.is_empty());
    assert_eq!(edges(&result), BTreeSet::from([("crate::a", "crate::b")]));
}

#[test]
fn external_imports_cannot_masquerade_as_builtin_macros_or_derives() {
    let result = graph(
        &[(
            "lib.rs",
            r#"
        use evil::{assert, Clone};
        #[derive(Clone)] struct S;
        fn f(){assert!(anything);}
    "#,
        )],
        AnalysisOptions::default(),
    );
    assert_eq!(result.diagnostics.len(), 2, "{:?}", result.diagnostics);
}

#[test]
fn external_providers_survive_alias_reexports_and_globs() {
    let result = graph(
        &[(
            "lib.rs",
            r#"
        mod api {pub use evil::{check as assert, Custom as Clone};}
        use crate::api::*;
        #[derive(Clone)] struct S;
        fn f(){assert!(anything);}
    "#,
        )],
        AnalysisOptions::default(),
    );
    assert_eq!(result.diagnostics.len(), 2, "{:?}", result.diagnostics);
}

#[test]
fn known_macro_and_derive_provider_aliases_remain_supported() {
    let result = graph(
        &[(
            "lib.rs",
            r#"
        use std::assert_eq as check;
        use std::clone::Clone as Copyable;
        use serde_json::json as data;
        #[derive(Copyable)] struct S;
        mod a {pub fn f()->usize {1}}
        fn f(){check!(crate::a::f(), 1); let _ = data!({"x": crate::a::f()});}
    "#,
        )],
        AnalysisOptions::default(),
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(edges(&result), BTreeSet::from([("crate", "crate::a")]));
}

#[test]
fn unknown_external_globs_require_known_macro_providers() {
    let result = graph(
        &[(
            "lib.rs",
            "use evil::*; #[derive(Clone)] struct S; fn f(){assert!(true);}",
        )],
        AnalysisOptions::default(),
    );
    assert_eq!(result.diagnostics.len(), 2, "{:?}", result.diagnostics);
}

#[test]
fn named_struct_patterns_use_the_type_namespace() {
    let result = graph(
        &[(
            "lib.rs",
            r#"
        mod a {pub struct S {pub n:u8} pub enum E {Named {n:u8}, Tuple(u8)}}
        mod b {fn f(s:crate::a::S, e:crate::a::E){
            let _ = crate::a::S {n:1};
            let _ = crate::a::E::Named {n:1};
            let crate::a::S {n:_}=s;
            match e {crate::a::E::Named {n:_} => {}, crate::a::E::Tuple(_) => {}}
        } fn generic<const N:u8>(){} fn g(){generic::<{crate::a::S {n:1}.n}>();}}
    "#,
        )],
        AnalysisOptions::default(),
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(edges(&result), BTreeSet::from([("crate::b", "crate::a")]));
}
