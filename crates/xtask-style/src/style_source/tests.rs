//! Role: 構文スタイル検査の誤検知回避と外部moduleの対象解決を検証する。
use super::*;
use xtask_policy::Policy;

/// Style settings with the basic preset, plus the given `[style]` lines.
fn style(lines: &str) -> StyleSettings {
    Policy::from_toml(&format!("[style]\nenabled=true\n{lines}\n"))
        .unwrap()
        .style()
        .unwrap()
        .clone()
}

fn fixture(source: &str, style: &StyleSettings) -> Vec<StyleDiagnostic> {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("lib.rs");
    fs::write(&file, source).unwrap();
    check(&[file], style).diagnostics
}

#[test]
fn imports_allow_siblings_self_and_alias_but_not_nested_paths() {
    let diagnostics = fixture(
        "use foo::{self, Foo as Renamed}; use foo::Bar; use foo::{Foo, bar::Bar};",
        &style(""),
    );
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].rule, StyleRule::ImportGranularity);
}

#[test]
fn uppercase_hex_ignores_strings_and_comments_but_checks_macro_literals() {
    let diagnostics = fixture(
        "// 0xAB\nconst A: u32 = 0xA_Fu32; const B: &str = \"0xAB\"; const C: u8 = 0xaf;\nmac!(0xab, \"0xAB\", [0xC0]);",
        &style(""),
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
        &style("extra_rules=['glob-imports']"),
    );
    assert_eq!(diagnostics.len(), 1);
    let diagnostics = fixture(
        "pub use foreign::*;",
        &style("extra_rules=['glob-imports']"),
    );
    assert_eq!(diagnostics.len(), 1);
}

#[test]
fn allowed_globs_need_no_exception() {
    let diagnostics = fixture(
        "use framework::prelude::*; use framework::other::*;",
        &style("extra_rules=['glob-imports']\nallowed_globs=['framework::prelude']"),
    );
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
        &style("extra_rules=['glob-imports']"),
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
    let settings = style("extra_rules=['glob-imports']");
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
    let settings = style("extra_rules=['non-leaf-implementation']");
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
        &style("extra_rules=['rustfmt-skip']"),
    );
    assert_eq!(diagnostics.len(), 3);
}

#[test]
fn compiler_only_policy_does_not_parse_unrelated_source_fragments() {
    let settings = style("disabled_rules=['hex-literals','import-granularity']");
    assert!(
        fixture(
            "this is a macro include fragment, not a Rust file",
            &settings
        )
        .is_empty()
    );
}

#[test]
fn unparsable_file_does_not_hide_others() {
    let directory = tempfile::tempdir().unwrap();
    let broken = directory.path().join("broken.rs");
    let good = directory.path().join("good.rs");
    fs::write(&broken, "fn broken( {").unwrap();
    fs::write(&good, "const A: u32 = 0xAB;").unwrap();
    let report = check(&[broken.clone(), good.clone()], &style(""));
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.rule == StyleRule::HexLiterals && diagnostic.path == good),
        "{:?}",
        report.diagnostics
    );
    assert!(
        report
            .errors
            .iter()
            .any(|error| error.contains("broken.rs")),
        "{:?}",
        report.errors
    );
}

#[test]
fn nested_scopes_do_not_leak() {
    let diagnostics = fixture(
        "mod inner { fn f() { use std::cmp::Ordering::*; } }\nuse std::cmp::Ordering::*;\n",
        &style("extra_rules=['glob-imports']"),
    );
    let lines: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.rule == StyleRule::GlobImports)
        .map(|diagnostic| diagnostic.line)
        .collect();
    assert_eq!(lines, [2]);
}

#[test]
fn item_index_names_line_diagnostics() {
    let parsed = syn::parse_file("fn a() {}\nmod m {\n    fn b() {}\n}").unwrap();
    let mut items = Vec::new();
    index_items(&parsed.items, "", &mut items);
    let name_at = |line, column| innermost(&items, line, column).map(|item| item.item.as_str());
    assert_eq!(name_at(3, 5), Some("m::b"));
    assert_eq!(name_at(1, 1), Some("a"));
}
