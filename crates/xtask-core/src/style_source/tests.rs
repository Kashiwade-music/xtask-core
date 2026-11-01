//! Role: 構文スタイル検査の誤検知回避と外部moduleの対象解決を検証する。
use super::*;

fn fixture(source: &str, policy: &StylePolicy) -> Vec<StyleDiagnostic> {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("lib.rs");
    fs::write(&file, source).unwrap();
    check(&[file], policy).unwrap()
}

fn policy(extra_rules: Vec<StyleRule>) -> StylePolicy {
    StylePolicy {
        enabled: true,
        extra_rules,
        ..StylePolicy::default()
    }
}

#[test]
fn imports_allow_siblings_self_and_alias_but_not_nested_paths() {
    let diagnostics = fixture(
        "use foo::{self, Foo as Renamed}; use foo::Bar; use foo::{Foo, bar::Bar};",
        &policy(Vec::new()),
    );
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].rule, StyleRule::ImportGranularity);
}

#[test]
fn uppercase_hex_ignores_strings_and_comments_but_checks_macro_literals() {
    let diagnostics = fixture(
        "// 0xAB\nconst A: u32 = 0xA_Fu32; const B: &str = \"0xAB\"; const C: u8 = 0xaf;\nmac!(0xab, \"0xAB\", [0xC0]);",
        &policy(Vec::new()),
    );
    let lines: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| (diagnostic.rule, diagnostic.line))
        .collect();
    assert_eq!(
        lines,
        [(StyleRule::HexLiterals, 2), (StyleRule::HexLiterals, 3)]
    );
}

#[test]
fn glob_exceptions_distinguish_inline_super_and_leaf_reexports() {
    let diagnostics = fixture(
        "mod child; pub use child::*; mod tests { use super::*; use unrelated::*; }",
        &policy(vec![StyleRule::GlobImports]),
    );
    assert_eq!(diagnostics.len(), 1);
    let diagnostics = fixture("pub use foreign::*;", &policy(vec![StyleRule::GlobImports]));
    assert_eq!(diagnostics.len(), 1);
}

#[test]
fn glob_enum_variants_are_allowed_only_inside_function_bodies() {
    let diagnostics = fixture(
        concat!(
            "use Kind::*;\n",
            "fn f() { use Kind::*; use crate::Kind::*; use self::module::*; }\n",
            "impl Kind { fn g() { use Kind::*; } }\n",
            "fn h() { mod inner { use Kind::*; } }\n",
        ),
        &policy(vec![StyleRule::GlobImports]),
    );
    let lines: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.rule == StyleRule::GlobImports)
        .map(|diagnostic| diagnostic.line)
        .collect();
    assert_eq!(lines, [1, 2, 4]);
}

#[test]
fn glob_child_reexport_accepts_self_prefix_only_for_declared_child() {
    let settings = policy(vec![StyleRule::GlobImports]);
    let diagnostics = fixture(
        "mod child; pub use self::child::*; pub use self::{child::*}; pub use self::foreign::*;",
        &settings,
    );
    let globs: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.rule == StyleRule::GlobImports)
        .collect();
    assert_eq!(globs.len(), 1);
    assert_eq!(fixture("pub use self::child::*;", &settings).len(), 1);
}

#[test]
fn non_leaf_exempts_test_and_bridge_modules_and_accepts_reexports() {
    let settings = policy(vec![StyleRule::NonLeafImplementation]);
    assert!(fixture("fn f() {} mod tests { fn test() {} }", &settings).is_empty());
    assert!(fixture("fn f() {} #[cxx::bridge] mod ffi {}", &settings).is_empty());
    assert!(fixture("fn f() {} #[cfg(test)] mod unit;", &settings).is_empty());
    assert!(fixture("mod child; pub use child::Thing;", &settings).is_empty());
    assert!(fixture("mod child; use std::fmt; extern crate alloc;", &settings).is_empty());
    assert_eq!(
        fixture("mod child; #[cfg(feature = \"x\")] mod other {}", &settings).len(),
        1
    );
    assert_eq!(fixture("mod child; fn f() {}", &settings).len(), 1);
    assert_eq!(fixture("mod child {}", &settings).len(), 1);
}

#[test]
fn skip_is_detected_inside_cfg_attr() {
    let diagnostics = fixture(
        "#[rustfmt::skip] fn a() {} #[cfg_attr(any(), rustfmt::skip)] fn b() {} #[rustfmt::skip::macros(m)] fn c() {}",
        &policy(vec![StyleRule::RustfmtSkip]),
    );
    assert_eq!(diagnostics.len(), 3);
}

#[test]
fn compiler_only_policy_does_not_parse_unrelated_source_fragments() {
    let mut settings = policy(Vec::new());
    settings.disabled_rules = vec![StyleRule::HexLiterals, StyleRule::ImportGranularity];
    assert!(
        fixture(
            "this is a macro include fragment, not a Rust file",
            &settings
        )
        .is_empty()
    );
}
