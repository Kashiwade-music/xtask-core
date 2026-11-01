//! Role: 親workspaceへの20規則の適用、例外と非破壊の実ツール連携を検証する。
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::Command;

use xtask_core::{Config, Report, StyleRule, run};

const ALL_RULES: &str = "[style]\nenabled = true\nextra_rules = ['glob-imports', 'non-leaf-implementation', 'rustfmt-skip', 'non-exhaustive', 'catch-unwind', 'poison-access', 'options-default']\napi_packages = ['fixture']\noptions_structs = ['fixture::api::Options']\n";

fn fixture(files: &[(&str, &str)], policy: &str) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir_all(directory.path().join("src")).unwrap();
    fs::create_dir_all(directory.path().join("dev/xtask/policy")).unwrap();
    fs::write(
        directory.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .unwrap();
    for (path, contents) in files {
        fs::write(directory.path().join(path), contents).unwrap();
    }
    fs::write(
        directory.path().join("dev/xtask/policy/workspace.toml"),
        policy,
    )
    .unwrap();
    let output = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
        .args(["generate-lockfile", "--offline"])
        .current_dir(directory.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    directory
}

fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    for relative in [
        "Cargo.toml",
        "Cargo.lock",
        "dev/xtask/policy/workspace.toml",
        "src/lib.rs",
        "src/api.rs",
    ] {
        let path = root.join(relative);
        if path.exists() {
            files.insert(relative.into(), fs::read(path).unwrap());
        }
    }
    files
}

fn check_unchanged(root: &Path) -> Report {
    let before = snapshot(root);
    let mut config = Config::new(root);
    config.no_write = true;
    let report = run(&config).unwrap();
    assert_eq!(
        snapshot(root),
        before,
        "style check modified repository input"
    );
    assert!(
        !root.join("architecture").exists(),
        "--no-write created architecture artifacts"
    );
    report
}

#[test]
fn all_twenty_rules_accept_a_documented_public_api() {
    let directory = fixture(
        &[
            (
                "src/lib.rs",
                "//! Role: Export the checked public API.\n\npub mod api;\n",
            ),
            (
                "src/api.rs",
                r#"//! Role: Provide documented values and options.

/// Configure value processing.
#[derive(Default)]
#[non_exhaustive]
pub struct Options {
    /// Enable processing.
    pub enabled: bool,
}

/// Own a sequence of values.
#[derive(Default)]
pub struct Values {
    values: Vec<u8>,
}

impl Values {
    /// Construct an empty sequence.
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the number of values.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Indicate whether the sequence contains no values.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

/// Add a count operation to value collections.
pub trait ValueExt {
    /// Count all stored values.
    fn count(&self) -> usize;
}

impl ValueExt for Values {
    fn count(&self) -> usize {
        self.len()
    }
}
"#,
            ),
        ],
        ALL_RULES,
    );
    let report = check_unchanged(directory.path());
    assert!(report.is_success(), "{:#?}", report.errors);
    assert!(report.style_diagnostics.is_empty());
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("checked 20 style rules"))
    );
}

#[test]
fn tool_and_source_violations_are_aggregated_without_changing_inputs() {
    let directory = fixture(
        &[
            (
                "src/lib.rs",
                "//! Role: Export deliberately invalid style examples.\npub mod api;\npub const HEX:u8=0xAF;\n",
            ),
            (
                "src/api.rs",
                r#"//! Role: Exercise independent mechanical style diagnostics.
use std::{fmt, collections::HashMap};
use std::collections::*;

pub struct Options { pub enabled: bool }
pub struct Manual { value: u8 }
impl Default for Manual { fn default() -> Self { Self { value: 0 } } }
pub struct Factory;
impl Factory { pub fn new() -> Self { Self } }
pub struct Values(Vec<u8>);
impl Values { pub fn len(&self) -> usize { self.0.len() } }
pub fn borrowed(input: &Vec<u8>) -> usize { input.len() }
pub fn checked(value: Option<u8>) -> u8 {
    if value.is_some() { value.unwrap() } else { 0 }
}
pub unsafe fn raw(pointer: *const u8) -> u8 { unsafe { *pointer } }
pub fn catch() { let _ = std::panic::catch_unwind(|| {}); }
pub fn poisoned(error: std::sync::PoisonError<u8>) -> u8 { error.into_inner() }
#[rustfmt::skip]
pub fn skipped( ) { }
"#,
            ),
        ],
        ALL_RULES,
    );
    let report = check_unchanged(directory.path());
    let found: BTreeSet<_> = report
        .style_diagnostics
        .iter()
        .map(|diagnostic| diagnostic.rule)
        .collect();
    let expected: BTreeSet<_> = StyleRule::ALL
        .into_iter()
        .filter(|rule| *rule != StyleRule::UnstableFeatures)
        .collect();
    let missing: Vec<_> = expected.difference(&found).collect();
    assert!(
        missing.is_empty(),
        "missing rules {missing:?}; diagnostics: {:#?}; errors: {:#?}",
        report.style_diagnostics,
        report.errors
    );
    assert!(!report.is_success());
    assert!(
        report
            .style_diagnostics
            .iter()
            .all(|diagnostic| diagnostic.path.is_absolute()
                && diagnostic.line > 0
                && diagnostic.column > 0)
    );
}

#[test]
fn scoped_exceptions_preserve_sibling_and_other_rule_violations() {
    let policy = concat!(
        "[style]\nenabled=true\n",
        "disabled_rules=['format','import-granularity','unstable-features','checked-unwrap','flexible-references','derivable-impls','new-default','len-is-empty','missing-docs','safety-docs','safety-comments']\n",
        "[[style.exceptions]]\nrule='unwrap'\npath='src/lib.rs'\nitem='reviewed'\nreason='Fixture deliberately demonstrates the approved invariant'\n"
    );
    let directory = fixture(
        &[(
            "src/lib.rs",
            "//! Role: Verify exact rule and item exception boundaries.\nfn reviewed(value: Option<u8>) -> u8 { let _ = 0xAA; value.unwrap() }\nfn unreviewed(value: Option<u8>) -> u8 { value.unwrap() }\n",
        )],
        policy,
    );
    let report = check_unchanged(directory.path());
    let unwraps: Vec<_> = report
        .style_diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.rule == StyleRule::Unwrap)
        .collect();
    assert_eq!(unwraps.len(), 1, "{:#?}", report.errors);
    assert_eq!(unwraps[0].item.as_deref(), Some("unreviewed"));
    assert!(
        report
            .style_diagnostics
            .iter()
            .any(|diagnostic| diagnostic.rule == StyleRule::HexLiterals
                && diagnostic.item.as_deref() == Some("reviewed"))
    );
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("Fixture deliberately demonstrates"))
    );
}

#[test]
fn unstable_features_are_rejected_and_disabled_style_never_uses_its_toolchain() {
    let directory = fixture(
        &[(
            "src/lib.rs",
            "//! Role: Reject an unstable compiler feature.\n#![feature(test)]\n",
        )],
        "[style]\nenabled=true\ndisabled_rules=['format','hex-literals','import-granularity','unwrap','checked-unwrap','flexible-references','derivable-impls','new-default','len-is-empty','missing-docs','safety-docs','safety-comments']\n",
    );
    let report = check_unchanged(directory.path());
    assert!(!report.is_success());
    assert!(
        report
            .errors
            .iter()
            .any(|error| error.contains("feature") || error.contains("unstable")),
        "{:#?}",
        report.errors
    );

    let directory = fixture(
        &[(
            "src/lib.rs",
            "//! Role: Preserve the existing checks without style tools.\nfn old_style( ) { let _ = Some(1).unwrap(); }\n",
        )],
        "[style]\ntoolchain='xtask-style-test-unavailable-toolchain'\n",
    );
    let report = check_unchanged(directory.path());
    assert!(report.is_success(), "{:#?}", report.errors);
    assert!(report.style_diagnostics.is_empty());
    assert!(
        !report
            .notes
            .iter()
            .any(|note| note.contains("checked") && note.contains("style rules"))
    );
}

fn optional_policy(rules: &str, settings: &str, exceptions: &str) -> String {
    let disabled = StyleRule::ALL[..13]
        .iter()
        .map(|rule| format!("'{rule}'"))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "[style]\nenabled=true\ndisabled_rules=[{disabled}]\nextra_rules=[{rules}]\n{settings}\n{exceptions}"
    )
}

#[test]
fn wildcard_exceptions_cannot_suppress_invalid_rule_targets() {
    for (rule, settings, expected) in [
        (
            "non-exhaustive",
            "api_packages=['absent-package']",
            "absent-package",
        ),
        (
            "options-default",
            "options_structs=['fixture::api::MissingOptions']",
            "MissingOptions",
        ),
        (
            "options-default",
            "options_structs=['fixture::api::PrivateOptions']",
            "private",
        ),
    ] {
        let exception = format!(
            "[[style.exceptions]]\nrule='{rule}'\npath='*'\nreason='This wildcard must not suppress an invalid target'\n"
        );
        let policy = optional_policy(&format!("'{rule}'"), settings, &exception);
        let directory = fixture(
            &[
                (
                    "src/lib.rs",
                    "//! Role: Expose target validation fixtures.\npub mod api;\n",
                ),
                (
                    "src/api.rs",
                    "//! Role: Provide an inaccessible options fixture.\nstruct PrivateOptions;\n",
                ),
            ],
            &policy,
        );
        let report = check_unchanged(directory.path());
        assert!(!report.is_success(), "invalid {rule} target was suppressed");
        assert!(
            report.errors.iter().any(|error| error.contains(expected)),
            "{rule} should report {expected:?}: {:#?}",
            report.errors
        );
        assert!(
            !report
                .notes
                .iter()
                .any(|note| note.starts_with("style exception")),
            "invalid target became a suppressible style violation: {:#?}",
            report.notes
        );
    }
}

#[test]
fn exact_item_exceptions_work_for_options_selectors() {
    let policy = optional_policy(
        "'options-default'",
        "options_structs=['fixture::api::Options']",
        "[[style.exceptions]]\nrule='options-default'\npath='src/lib.rs'\nitem='api::Options'\nreason='This fixture requires an explicit option value'\n",
    );
    let directory = fixture(
        &[
            (
                "src/lib.rs",
                "//! Role: Expose selected optional rule fixtures.\npub mod api;\n",
            ),
            (
                "src/api.rs",
                "//! Role: Verify exact exceptions for selected items.\npub struct Options;\n",
            ),
        ],
        &policy,
    );
    let report = check_unchanged(directory.path());
    assert!(report.is_success(), "{:#?}", report.errors);
    assert!(report.style_diagnostics.is_empty());
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("This fixture requires an explicit option value")),
        "exception not applied: {:#?}",
        report.notes
    );
    assert!(
        !report
            .notes
            .iter()
            .any(|note| note.contains("unused style exception")),
        "{:#?}",
        report.notes
    );
}
