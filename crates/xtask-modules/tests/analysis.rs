//! Role: モジュール読込み、名前解決、cfg別の参照と循環を検証する。
mod common;

use common::{Options, edges, graph, write_fixture};
use std::collections::BTreeSet;

#[test]
fn fixture_role_insertion_preserves_unicode_before_inline_braces() {
    let result = graph(
        &[(
            "lib.rs",
            "const TEXT: &str = \"寿\"; mod inner { pub fn f() {} }",
        )],
        Options::default(),
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
        Options::default(),
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
        Options::default(),
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
    let mut options = Options::default();
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
        Options::default(),
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
        Options::default(),
    );
    assert_eq!(
        edges(&result),
        BTreeSet::from([("crate::consumer", "crate::source")])
    );
    assert!(result.diagnostics.is_empty());
}

#[test]
fn include_and_block_modules_cannot_silently_pass() {
    let result = graph(
        &[(
            "lib.rs",
            "include!(\"generated.rs\"); fn f() { mod hidden {} }",
        )],
        Options::default(),
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
        Options::default(),
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
        Options::default(),
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
        Options::default(),
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
        Options::default(),
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
        Options::default(),
    );
    assert!(result.edges.is_empty(), "{:?}", result.edges);
}

#[test]
fn cfg_disabled_file_has_no_ghost_node_or_edges() {
    let result = graph(
        &[
            ("lib.rs", "mod a; fn f(){a::foo();}"),
            ("a.rs", "#![cfg(off)] pub fn foo(){}"),
        ],
        Options::default(),
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
        Options::default(),
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
        Options::default(),
    );
    assert_eq!(edges(&result), BTreeSet::from([("crate", "crate::a")]));
    assert!(result.cycles.is_empty());
}

#[test]
fn unresolved_explicit_local_paths_are_diagnostic() {
    let result = graph(
        &[("lib.rs", "fn f(_: crate::missing::T) {}")],
        Options::default(),
    );
    assert_eq!(result.diagnostics.len(), 1);
}

#[test]
fn generic_type_name_hides_module_without_import_edges() {
    let result = graph(
        &[(
            "lib.rs",
            "mod a {pub fn f(){}} fn generic<a>() {a::method();}",
        )],
        Options::default(),
    );
    assert!(result.edges.is_empty());
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
        Options {
            cfg: BTreeSet::from(["left".into()]),
            ..Default::default()
        },
    );
    assert!(result.cycles.is_empty());
    assert_eq!(edges(&result), BTreeSet::from([("crate::a", "crate::b")]));
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
        Options::default(),
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(edges(&result), BTreeSet::from([("crate::b", "crate::a")]));
}

#[test]
fn cfg_true_is_enabled() {
    let result = graph(
        &[
            ("lib.rs", "#[cfg(true)] mod x; #[cfg(false)] mod y;"),
            ("x.rs", ""),
        ],
        Options::default(),
    );
    assert!(result.modules.iter().any(|module| module.id == "crate::x"));
    assert!(!result.modules.iter().any(|module| module.id == "crate::y"));
}

#[test]
fn one_parse_two_configurations() {
    use xtask_modules::{Configuration, analyze};
    let directory = write_fixture(&[
        ("lib.rs", "//! Role: Root.\n#[cfg(test)] mod tests;"),
        ("tests.rs", "//! Role: Tests.\n"),
    ]);
    let options = Options::default();
    let test = Configuration {
        cfg: BTreeSet::from(["test".to_string()]),
    };
    let results = analyze(
        &directory.path().join("lib.rs"),
        &options.settings(),
        &[Configuration::default(), test],
    );
    assert_eq!(results.len(), 2);
    let normal = results[0].as_ref().unwrap();
    let with_test = results[1].as_ref().unwrap();
    assert!(
        !normal
            .modules
            .iter()
            .any(|module| module.id == "crate::tests")
    );
    assert!(
        with_test
            .modules
            .iter()
            .any(|module| module.id == "crate::tests")
    );
}

#[test]
fn errors_have_location() {
    let directory = write_fixture(&[("lib.rs", "//! Role: Root.\n#[path] mod x;\n")]);
    let error = Options::default()
        .analyze(&directory.path().join("lib.rs"))
        .unwrap_err();
    assert!(
        error.contains("lib.rs:2: invalid path attribute"),
        "{error}"
    );
}

#[test]
fn invalid_cfg_reported_once() {
    let result = graph(&[("lib.rs", "#[cfg(a::b)] mod x {}")], Options::default());
    let reported = result
        .diagnostics
        .iter()
        .filter(|message| message.contains("unsupported cfg predicate"))
        .count();
    assert_eq!(reported, 1, "{:?}", result.diagnostics);
}

#[test]
fn modules_are_sorted_with_parents_first() {
    let result = graph(
        &[
            ("lib.rs", "mod a; mod a_b;"),
            ("a.rs", "mod x;"),
            ("a/x.rs", ""),
            ("a_b.rs", ""),
        ],
        Options::default(),
    );
    let ids = result
        .modules
        .iter()
        .map(|module| module.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["crate", "crate::a", "crate::a::x", "crate::a_b"]);
    assert_eq!(result.modules[0].parent, None);
    for (index, module) in result.modules.iter().enumerate().skip(1) {
        assert!(module.parent.is_some_and(|parent| parent < index));
    }
    let child = result
        .modules
        .iter()
        .find(|module| module.id == "crate::a::x")
        .unwrap();
    assert_eq!(
        child
            .parent
            .map(|parent| result.modules[parent].id.as_str()),
        Some("crate::a")
    );
}

#[test]
fn declared_type_does_not_hide_glob_value() {
    let result = graph(
        &[(
            "lib.rs",
            "\nmod a { \n pub fn X() {} }\nmod b { \n use crate::a::*; type X = (); fn f() { X(); } }\n",
        )],
        Options::default(),
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(edges(&result).contains(&("crate::b", "crate::a")));
}

#[test]
fn shadowed_name_is_not_forwarded() {
    let forward = graph(
        &[(
            "lib.rs",
            "\nmod a { \n pub fn f() {} }\nmod b { \n pub use crate::a::*; pub fn f() {} }\nmod c { \n use crate::b::*; fn g() { f(); } }\n",
        )],
        Options::default(),
    );
    assert!(!edges(&forward).contains(&("crate::c", "crate::a")));
    assert!(edges(&forward).contains(&("crate::c", "crate::b")));
    let reversed = graph(
        &[(
            "lib.rs",
            "\nmod c { \n use crate::b::*; fn g() { f(); } }\nmod b { \n pub use crate::a::*; pub fn f() {} }\nmod a { \n pub fn f() {} }\n",
        )],
        Options::default(),
    );
    assert!(!edges(&reversed).contains(&("crate::c", "crate::a")));
}

#[test]
fn three_level_reexport_chain() {
    let result = graph(
        &[(
            "lib.rs",
            "\nmod a { \n pub fn X() {} }\nmod b { \n pub use crate::a::*; }\nmod c { \n pub use crate::b::*; }\nmod d { \n use crate::c::*; fn f() { X(); } }\n",
        )],
        Options::default(),
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(edges(&result).contains(&("crate::d", "crate::a")));
}

#[test]
fn external_glob_reexport_keeps_uncertainty() {
    let result = graph(
        &[(
            "lib.rs",
            "\nmod p { \n pub use ext::*; }\nmod c { \n use crate::p::*; #[derive(Clone)] struct S; }\n",
        )],
        Options::default(),
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.starts_with("crate::c:") && d.contains("Clone")),
        "{:?}",
        result.diagnostics
    );
}

#[test]
fn block_glob_does_not_hide_module_glob() {
    let result = graph(
        &[(
            "lib.rs",
            "\nuse foo::*;\nfn f() { use std::fmt::*; #[derive(Clone)] struct S; }\n",
        )],
        Options::default(),
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.starts_with("crate:") && d.contains("Clone")),
        "{:?}",
        result.diagnostics
    );
}

#[test]
fn glob_uses_every_resolved_module() {
    let result = graph(
        &[(
            "lib.rs",
            "\nmod a { \n pub fn f() {} }\nmod b { \n pub fn g() {} }\nmod p { \n pub use crate::a as pick;\n pub use crate::b as pick; }\nmod c { \n use crate::p::pick::*; fn h() { f(); g(); } }\n",
        )],
        Options::default(),
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(edges(&result).contains(&("crate::c", "crate::a")));
    assert!(edges(&result).contains(&("crate::c", "crate::b")));
}

#[test]
fn block_use_is_processed_once() {
    let result = graph(
        &[("lib.rs", "\nmod a { \n }\nfn f() { use crate::a::f; }\n")],
        Options::default(),
    );
    let count = result
        .diagnostics
        .iter()
        .filter(|d| d.contains("unresolved explicit local import"))
        .count();
    assert_eq!(count, 1, "{:?}", result.diagnostics);
}

#[test]
fn invalid_cfg_on_impl_fn_is_one_diagnostic() {
    let result = graph(
        &[("lib.rs", "struct S;\nimpl S { #[cfg(a::b)] fn f() {} }\n")],
        Options::default(),
    );
    let reported = result
        .diagnostics
        .iter()
        .filter(|message| message.contains("unsupported cfg predicate"))
        .count();
    assert_eq!(reported, 1, "{:?}", result.diagnostics);
}
