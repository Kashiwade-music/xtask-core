//! Role: マクロ、属性、derive と外部 provider の解決（trust の要否を含む）を検証する。
#[path = "common/analysis.rs"]
mod analysis_support;
mod common;

use analysis_support::{edges, graph};
use common::Options;
use std::collections::BTreeSet;

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
        Options::default(),
    );
    assert_eq!(
        edges(&result),
        BTreeSet::from([("crate::consumer", "crate::source")])
    );
    assert_eq!(result.diagnostics.len(), 1);
}

#[test]
fn custom_expansions_require_explicit_trust() {
    let files = [("lib.rs", "#[custom::emit] #[derive(Custom)] struct S;")];
    let result = graph(&files, Options::default());
    assert_eq!(result.diagnostics.len(), 2);
    let mut options = Options::default();
    options.trusted_attributes.insert("custom::emit".into());
    options.trusted_derives.insert("Custom".into());
    assert!(graph(&files, options).diagnostics.is_empty());
}

#[test]
fn trusted_macros_skip_expansion_diagnostics_only_on_exact_path() {
    let files = [(
        "lib.rs",
        "proptest! { fn a() {} } fn f() { prop_assert!(1 == 1); ::proptest::prop_oneof![1, 2]; }",
    )];
    let result = graph(&files, Options::default());
    assert!(!result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let mut options = Options::default();
    for name in ["proptest", "prop_assert", "proptest::prop_oneof"] {
        options.trusted_macros.insert(name.into());
    }
    let result = graph(&files, options);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);

    let mut options = Options::default();
    options.trusted_macros.insert("proptest".into());
    let result = graph(
        &[(
            "lib.rs",
            "proptest! { fn a() {} } other! { fn b() {} } fn f() { other_expr!(1); prop_assert!(true); }",
        )],
        options,
    );
    assert!(
        result.diagnostics.iter().any(|d| d.contains("other")),
        "{:?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.iter().all(|d| !d.contains("proptest")),
        "{:?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.iter().any(|d| d.contains("prop_assert")),
        "{:?}",
        result.diagnostics
    );
}

fn trusting(names: &[&str]) -> Options {
    let mut options = Options::default();
    for name in names {
        options.trusted_macros.insert((*name).into());
    }
    options
}

#[test]
fn untrusted_local_macro_calls_keep_their_diagnostic() {
    let files = [(
        "lib.rs",
        "macro_rules! ensure { ($e:expr) => { assert!($e) }; } mod user { fn f() { ensure!(true); } } fn g() { ensure!(true); }",
    )];
    let result = graph(&files, Options::default());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.contains("local macro expansion is unsupported: ensure")),
        "{:?}",
        result.diagnostics
    );
}

#[test]
fn trusted_local_expression_macro_records_edge_to_definition_module() {
    let files = [(
        "lib.rs",
        "mod defs { macro_rules! ensure { ($e:expr) => { assert!($e) }; } pub(crate) use ensure; } mod user { use crate::defs::ensure; fn f() { ensure!(true); } }",
    )];
    let result = graph(&files, trusting(&["ensure"]));
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(edges(&result).contains(&("crate::user", "crate::defs")));
    let untrusted = graph(&files, Options::default());
    assert!(!untrusted.diagnostics.is_empty());
    // Only the exact written path is trusted.
    let other = graph(&files, trusting(&["crate::defs::ensure"]));
    assert!(!other.diagnostics.is_empty(), "{:?}", other.diagnostics);
}

#[test]
fn trusted_local_macro_matches_in_statement_type_and_pattern_positions() {
    let files = [(
        "lib.rs",
        "mod defs { macro_rules! m { () => { u8 }; } pub(crate) use m; } mod user { use crate::defs::m; fn f(_: m!()) { m!(); let m!() = 1u8; } }",
    )];
    let result = graph(&files, trusting(&["m"]));
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(edges(&result).contains(&("crate::user", "crate::defs")));
    assert!(!graph(&files, Options::default()).diagnostics.is_empty());
}

#[test]
fn trusted_local_item_macro_names_are_references_to_its_module() {
    let files = [(
        "lib.rs",
        r#"
        mod defs { macro_rules! make { () => { pub fn generated() {} }; } pub(crate) use make; }
        mod m { use crate::defs::make; make!(); }
        mod user { fn f() { crate::m::generated(); } }
        mod uses { use crate::m::generated; fn f() { generated(); } }
        mod inner { use crate::m; fn f() { m::generated(); } }
        mod selfcall {
            use crate::defs::make;
            make!();
            fn f() { self::generated(); crate::selfcall::generated(); }
        }
    "#,
    )];
    let result = graph(&files, trusting(&["make"]));
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        edges(&result),
        BTreeSet::from([
            ("crate::inner", "crate::m"),
            ("crate::m", "crate::defs"),
            ("crate::selfcall", "crate::defs"),
            ("crate::user", "crate::m"),
            ("crate::uses", "crate::m"),
        ])
    );
}

#[test]
fn unresolved_names_stay_diagnostic_outside_opaque_modules_and_when_untrusted() {
    let files = [(
        "lib.rs",
        r#"
        mod defs { macro_rules! make { () => { pub fn generated() {} }; } pub(crate) use make; }
        mod m { use crate::defs::make; make!(); }
        mod other { pub fn real() {} }
        mod user { fn f() { crate::other::generated(); } }
    "#,
    )];
    let result = graph(&files, trusting(&["make"]));
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert!(result.diagnostics[0].contains("crate::other::generated"));

    // (d) Untrusted item-position macros keep both their diagnostic and unresolved paths.
    let result = graph(&files, Options::default());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.contains("item macro expansion is unsupported: make")),
        "{:?}",
        result.diagnostics
    );
    let files = [(
        "lib.rs",
        r#"
        mod defs { macro_rules! make { () => { pub fn generated() {} }; } pub(crate) use make; }
        mod m { use crate::defs::make; make!(); }
        mod user { fn f() { crate::m::generated(); } }
    "#,
    )];
    let result = graph(&files, Options::default());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.contains("unresolved explicit local path: crate::m::generated")),
        "{:?}",
        result.diagnostics
    );
}

#[test]
fn trusted_local_item_macro_records_edge_to_definition_module() {
    let files = [(
        "lib.rs",
        "mod defs { macro_rules! make { () => { fn g() {} }; } pub(crate) use make; } mod m { use crate::defs::make; make!(); }",
    )];
    let result = graph(&files, trusting(&["make"]));
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(edges(&result).contains(&("crate::m", "crate::defs")));
}

#[test]
fn trusted_local_macros_match_macro_export_and_crate_paths_exactly() {
    let files = [(
        "lib.rs",
        r#"
        #[macro_export]
        macro_rules! ensure { ($e:expr) => { assert!($e) }; }
        #[macro_export]
        macro_rules! make { () => { pub fn generated() {} }; }
        mod m { crate::make!(); }
        mod user { fn f() { crate::ensure!(true); crate::m::generated(); } }
    "#,
    )];
    let result = graph(&files, trusting(&["crate::ensure", "crate::make"]));
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        edges(&result),
        BTreeSet::from([
            ("crate::m", "crate"),
            ("crate::user", "crate"),
            ("crate::user", "crate::m"),
        ])
    );
    // Bare names do not match the written `crate::` path.
    let result = graph(&files, trusting(&["ensure", "make"]));
    assert!(!result.diagnostics.is_empty());
}

#[test]
fn textual_scope_macros_resolve_from_child_modules() {
    let files = [(
        "lib.rs",
        r#"
        macro_rules! ensure { ($e:expr) => { assert!($e) }; }
        mod user { mod deep { fn f() { ensure!(true); } } }
    "#,
    )];
    let result = graph(&files, trusting(&["ensure"]));
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        edges(&result),
        BTreeSet::from([("crate::user::deep", "crate")])
    );
    let result = graph(&files, Options::default());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.contains("local macro expansion is unsupported: ensure")),
        "{:?}",
        result.diagnostics
    );
}

#[test]
fn macro_use_modules_extend_the_parent_textual_scope() {
    let files = [
        (
            "lib.rs",
            "#[macro_use] mod defs; mod plain { macro_rules! hidden { () => {}; } } mod user { fn f() { ensure!(true); hidden!(); } }",
        ),
        (
            "defs.rs",
            "macro_rules! ensure { ($e:expr) => { assert!($e) }; }",
        ),
    ];
    let result = graph(&files, trusting(&["ensure", "hidden"]));
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    // Without #[macro_use], `hidden` is not in textual scope and creates no edge.
    assert_eq!(
        edges(&result),
        BTreeSet::from([("crate::user", "crate::defs")])
    );
    let result = graph(&files, trusting(&["hidden"]));
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.contains("local macro expansion is unsupported: ensure")),
        "{:?}",
        result.diagnostics
    );
}

#[test]
fn exported_macros_live_at_the_crate_root_wherever_defined() {
    let files = [(
        "lib.rs",
        r#"
        mod defs {
            #[macro_export]
            macro_rules! ensure { ($e:expr) => { assert!($e) }; }
            #[macro_export]
            macro_rules! make { () => { pub fn generated() {} }; }
        }
        mod m { crate::make!(); }
        mod user { fn f() { crate::ensure!(true); crate::m::generated(); } }
    "#,
    )];
    let result = graph(&files, trusting(&["crate::ensure", "crate::make"]));
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        edges(&result),
        BTreeSet::from([
            ("crate::m", "crate::defs"),
            ("crate::user", "crate::defs"),
            ("crate::user", "crate::m"),
        ])
    );
    let result = graph(&files, Options::default());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.contains("local macro expansion is unsupported: crate::ensure")),
        "{:?}",
        result.diagnostics
    );
}

#[test]
fn crate_root_item_macro_makes_unresolved_crate_paths_references_to_the_root() {
    let files = [(
        "lib.rs",
        r#"
        macro_rules! make { () => { pub fn generated() {} }; }
        make!();
        mod user { fn f() { crate::generated(); } }
    "#,
    )];
    let result = graph(&files, trusting(&["make"]));
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(edges(&result), BTreeSet::from([("crate::user", "crate")]));
}

#[test]
fn macro_use_chains_cross_several_modules() {
    let files = [
        (
            "lib.rs",
            "#[macro_use] mod a; mod user { fn f() { ensure!(true); } }",
        ),
        ("a.rs", "#[macro_use] mod b;"),
        ("a/b.rs", "#[macro_use] mod c;"),
        (
            "a/b/c.rs",
            "macro_rules! ensure { ($e:expr) => { assert!($e) }; }",
        ),
    ];
    let result = graph(&files, trusting(&["ensure"]));
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        edges(&result),
        BTreeSet::from([("crate::user", "crate::a::b::c")])
    );
}

#[test]
fn cfg_disabled_macro_definitions_are_not_in_textual_scope() {
    let files = [(
        "lib.rs",
        r#"
        #[cfg(off)] #[macro_use] mod off { macro_rules! from_mod { () => {}; } }
        #[cfg(off)] macro_rules! from_item { () => {}; }
        mod user { fn f() { from_mod!(); from_item!(); } }
    "#,
    )];
    let result = graph(&files, trusting(&["from_mod", "from_item"]));
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.edges.is_empty(), "{:?}", result.edges);
    let result = graph(&files, Options::default());
    assert!(
        result
            .diagnostics
            .iter()
            .all(|d| !d.contains("local macro expansion")),
        "{:?}",
        result.diagnostics
    );
}

#[test]
fn nearest_textual_scope_definition_wins() {
    let files = [(
        "lib.rs",
        r#"
        macro_rules! pick { () => {}; }
        mod near { macro_rules! pick { () => {}; } mod user { fn f() { pick!(); } } }
        mod far { fn f() { pick!(); } }
    "#,
    )];
    let result = graph(&files, trusting(&["pick"]));
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        edges(&result),
        BTreeSet::from([
            ("crate::far", "crate"),
            ("crate::near::user", "crate::near")
        ])
    );
}

#[test]
fn cfg_attr_can_supply_macro_use_and_macro_export() {
    let files = [
        (
            "lib.rs",
            r#"
        #[cfg_attr(on, macro_use)] mod used;
        mod exporter {
            #[cfg_attr(on, macro_export)]
            macro_rules! exported { () => {}; }
        }
        mod user { fn f() { scoped!(); crate::exported!(); } }
    "#,
        ),
        ("used.rs", "macro_rules! scoped { () => {}; }"),
    ];
    let mut options = trusting(&["scoped", "crate::exported"]);
    options.cfg.insert("on".into());
    let result = graph(&files, options);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        edges(&result),
        BTreeSet::from([
            ("crate::user", "crate::exporter"),
            ("crate::user", "crate::used")
        ])
    );
    // Without the cfg, neither attribute applies and no local macro is found.
    let result = graph(&files, trusting(&["scoped", "crate::exported"]));
    assert!(result.edges.is_empty(), "{:?}", result.edges);
}

#[test]
fn builtin_inert_attributes_and_derive_helpers_need_no_expansion() {
    let result = graph(
        &[(
            "lib.rs",
            "#[derive(Default)] enum E { #[default] A, B } #[windows_subsystem = \"windows\"] fn f() {}",
        )],
        Options::default(),
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
        Options::default(),
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}

#[test]
fn unknown_attributes_remain_diagnostics() {
    let result = graph(
        &[("lib.rs", "#[my_attr] #[other::tool] struct S;")],
        Options::default(),
    );
    assert_eq!(result.diagnostics.len(), 2, "{:?}", result.diagnostics);
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
            todo!("{}", crate::a::f());
        }}
    "#,
        )],
        Options::default(),
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(edges(&result), BTreeSet::from([("crate::b", "crate::a")]));
}

#[test]
fn json_requires_trust() {
    let source = r#"
        mod a { pub fn f() -> u8 { 1 } }
        fn f() { serde_json::json!({"a": crate::a::f()}) }
    "#;
    let untrusted = graph(&[("lib.rs", source)], Options::default());
    assert!(!untrusted.diagnostics.is_empty());
    let trusted = graph(
        &[("lib.rs", source)],
        Options {
            trusted_macros: BTreeSet::from(["serde_json::json".into()]),
            ..Default::default()
        },
    );
    assert!(trusted.diagnostics.is_empty(), "{:?}", trusted.diagnostics);
    assert!(edges(&trusted).is_empty());
}

#[test]
fn implicit_format_capture() {
    let result = graph(
        &[(
            "lib.rs",
            "mod a { pub const N: u8 = 1; } mod b { use crate::a::N; fn f() -> String { format!(\"{N}\") } }",
        )],
        Options::default(),
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(edges(&result), BTreeSet::from([("crate::b", "crate::a")]));
}

#[test]
fn local_macros_do_not_inherit_builtin_argument_grammar() {
    let result = graph(
        &[(
            "lib.rs",
            "macro_rules! assert { ($($t:tt)*) => {} } fn f(){assert!(anything);} ",
        )],
        Options::default(),
    );
    assert_eq!(result.diagnostics.len(), 1);
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
        Options::default(),
    );
    assert_eq!(result.diagnostics.len(), 2, "{:?}", result.diagnostics);
}

#[test]
fn unattributed_expressions_keep_references_with_a_shadowed_derive_provider() {
    let result = graph(
        &[(
            "lib.rs",
            r#"
        use evil::Clone;
        #[cfg_attr(active, derive(Clone))] struct Unsupported;
        #[cfg(inactive)] #[derive(Clone)] struct Excluded;
        mod provider {pub fn value()->u32 {1}}
        fn plain()->u32 {
            let value = crate::provider::value();
            match value {0 => 0, _ => value + crate::provider::value()}
        }
    "#,
        )],
        Options {
            cfg: BTreeSet::from(["active".into()]),
            ..Default::default()
        },
    );
    assert_eq!(
        edges(&result),
        BTreeSet::from([("crate", "crate::provider")])
    );
    assert_eq!(
        result.diagnostics,
        ["crate: derive macro expansion is unsupported: Clone"]
    );
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
        Options::default(),
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
        #[derive(Copyable)] struct S;
        mod a {pub fn f()->usize {1}}
        fn f(){check!(crate::a::f(), 1);}
    "#,
        )],
        Options::default(),
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
        Options::default(),
    );
    assert_eq!(result.diagnostics.len(), 2, "{:?}", result.diagnostics);
}

#[test]
fn trusted_external_globs_keep_std_macro_providers() {
    let mut options = Options::default();
    options.trusted_globs.insert("framework::prelude".into());
    let result = graph(
        &[(
            "lib.rs",
            "use framework::prelude::*; #[derive(Clone)] struct S; fn f(){assert!(true);}",
        )],
        options,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}

#[test]
fn block_glob_reaches_block_derive() {
    // The block's own `use` items are applied before its items' attributes are expanded.
    for source in [
        "fn f() { use ext::*; #[derive(Clone)] struct S; }",
        "fn f() { use other::Clone; #[derive(Clone)] struct S; }",
    ] {
        let result = graph(&[("lib.rs", source)], Options::default());
        assert!(
            result
                .diagnostics
                .contains(&"crate: derive macro expansion is unsupported: Clone".to_string()),
            "{source}: {:?}",
            result.diagnostics
        );
    }
}

#[test]
fn prelude_attribute_under_unknown_glob() {
    let result = graph(
        &[(
            "lib.rs",
            "mod c { use ext::*; #[test] fn f() {} #[inline] fn g() {} }",
        )],
        Options {
            cfg: BTreeSet::from(["test".into()]),
            ..Default::default()
        },
    );
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert!(result.diagnostics[0].contains("test"));
}
