//! Role: 構文スタイル検査の誤検知回避と外部moduleの対象解決を検証する。
use super::*;
use xtask_workspace::{Package, Target};

fn fixture(source: &str, policy: &StylePolicy) -> Vec<StyleDiagnostic> {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("lib.rs");
    fs::write(&file, source).unwrap();
    check(directory.path(), &workspace(&file), &[file], policy).unwrap()
}

fn workspace(file: &Path) -> Workspace {
    Workspace {
        root: file.parent().unwrap().to_path_buf(),
        packages: vec![Package {
            id: "demo".into(),
            name: "demo".into(),
            manifest_path: file.parent().unwrap().join("Cargo.toml"),
            targets: vec![Target {
                name: "demo".into(),
                kind: vec!["lib".into()],
                src_path: file.to_path_buf(),
                required_features: Vec::new(),
                test: true,
                edition: "2024".into(),
            }],
            features: BTreeMap::new(),
        }],
        edges: Vec::new(),
    }
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
fn uppercase_hex_ignores_strings_comments_and_macro_tokens() {
    let diagnostics = fixture(
        "// 0xAB\nconst A: u32 = 0xA_Fu32; const B: &str = \"0xAB\"; const C: u8 = 0xaf; mac!(0xAB);",
        &policy(Vec::new()),
    );
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].rule, StyleRule::HexLiterals);
    assert_eq!(diagnostics[0].line, 2);
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
    assert!(fixture("mod child; pub use child::Thing;", &settings).is_empty());
    assert_eq!(fixture("mod child; fn f() {}", &settings).len(), 1);
    assert_eq!(fixture("mod child {}", &settings).len(), 1);
}

#[test]
fn skip_is_detected_inside_cfg_attr() {
    let diagnostics = fixture(
        "#[rustfmt::skip] fn a() {} #[cfg_attr(any(), rustfmt::skip)] fn b() {}",
        &policy(vec![StyleRule::RustfmtSkip]),
    );
    assert_eq!(diagnostics.len(), 2);
}

#[test]
fn trait_selector_resolves_path_attribute_and_nested_inline_module() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("lib.rs");
    let child = directory.path().join("custom.rs");
    fs::write(&file, "#[path = \"custom.rs\"] mod adapter;").unwrap();
    fs::write(&child, "mod nested { pub trait Foreign {} }").unwrap();
    let mut settings = policy(vec![StyleRule::ExtensionTraitNames]);
    settings.extension_traits = vec!["demo::adapter::nested::Foreign".into()];
    let diagnostics = check(
        directory.path(),
        &workspace(&file),
        &[file, child.clone()],
        &settings,
    )
    .unwrap();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].path, child);
    assert!(diagnostics[0].message.contains("must end in Ext"));
}

#[test]
fn unmatched_trait_selector_fails_instead_of_silently_passing() {
    let mut settings = policy(vec![StyleRule::ExtensionTraitNames]);
    settings.extension_traits = vec!["demo::MissingExt".into()];
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("lib.rs");
    fs::write(&file, "pub trait ExistingExt {}").unwrap();
    let error = check(directory.path(), &workspace(&file), &[file], &settings).unwrap_err();
    assert!(error.contains("matched no source trait"));
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
