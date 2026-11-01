//! Role: lintの選択、Clippy設定の保持、compiler診断の分類を検証する。
use std::collections::BTreeMap;
use std::path::Path;

use serde_json::json;
use xtask_workspace::{AnalysisSettings, StylePolicy, StyleRule, Workspace};

use super::{ToolReport, clippy};

#[test]
fn lint_selection_is_explicit_and_non_exhaustive_is_scoped() {
    let policy = StylePolicy {
        enabled: true,
        extra_rules: vec![StyleRule::NonExhaustive],
        ..StylePolicy::default()
    };
    let ordinary = clippy::lints(&policy, false);
    assert!(ordinary.contains_key("clippy::unwrap_used"));
    assert!(!ordinary.contains_key("clippy::expect_used"));
    assert!(!ordinary.contains_key("clippy::exhaustive_enums"));
    let api = clippy::lints(&policy, true);
    assert!(api.contains_key("clippy::exhaustive_structs"));
}

#[test]
fn configuration_preserves_project_settings_and_extends_banned_methods() {
    let table =
        "msrv = '1.85'\ndisallowed-methods = ['std::env::set_var']\ndisallowed-names = ['special']"
            .parse::<toml::Table>()
            .unwrap();
    let policy = StylePolicy {
        enabled: true,
        extra_rules: vec![StyleRule::CatchUnwind, StyleRule::PoisonAccess],
        ..StylePolicy::default()
    };
    let merged = clippy::config_table(table, &policy).unwrap();
    assert_eq!(merged["msrv"].as_str(), Some("1.85"));
    assert_eq!(merged["disallowed-names"][0].as_str(), Some("special"));
    assert_eq!(merged["disallowed-methods"].as_array().unwrap().len(), 5);
    assert_eq!(merged["allow-unwrap-in-tests"].as_bool(), Some(false));
    assert_eq!(merged["check-private-items"].as_bool(), Some(true));
}

fn diagnostic(code: &str, level: &str, text: &str) -> serde_json::Value {
    json!({"reason":"compiler-message", "message": {
        "level":level,"code":{"code":code},"message":text,
        "spans":[{"file_name":"src/lib.rs","line_start":7,"column_start":9,"is_primary":true}]
    }})
}

#[test]
fn compiler_json_separates_policy_violations_and_compile_errors() {
    let stdout = [
        diagnostic("clippy::unwrap_used", "warning", "unwrap"),
        diagnostic(
            "clippy::disallowed_methods",
            "warning",
            "xtask-style:poison-access",
        ),
        diagnostic(
            "clippy::disallowed_methods",
            "warning",
            "project's unrelated ban",
        ),
        diagnostic("E0425", "error", "cannot find value"),
    ]
    .iter()
    .map(serde_json::Value::to_string)
    .collect::<Vec<_>>()
    .join("\n");
    let mut report = ToolReport::default();
    let lints = BTreeMap::from([("clippy::unwrap_used", StyleRule::Unwrap)]);
    assert!(clippy::parse_output(
        Path::new("workspace"),
        stdout.as_bytes(),
        &lints,
        &mut report
    ));
    assert_eq!(report.diagnostics.len(), 2);
    assert_eq!(
        report.diagnostics[0].path,
        Path::new("workspace/src/lib.rs")
    );
    assert_eq!(report.diagnostics[0].line, 7);
    assert_eq!(report.diagnostics[1].rule, StyleRule::PoisonAccess);
    assert_eq!(report.errors, ["cannot find value"]);
}

#[test]
fn unknown_lints_without_spans_are_not_silently_ignored() {
    let stdout = json!({"reason":"compiler-message","message":{"level":"error","message":"unknown lint","code":{"code":"unknown_lints"},"spans":[]}}).to_string();
    let mut report = ToolReport::default();
    assert!(clippy::parse_output(
        Path::new("."),
        stdout.as_bytes(),
        &BTreeMap::new(),
        &mut report
    ));
    assert_eq!(report.errors, ["unknown lint"]);
}

fn fixture(source: &str) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("src")).unwrap();
    std::fs::write(
        directory.path().join("Cargo.toml"),
        "[package]\nname='tool-fixture'\nversion='0.1.0'\nedition='2021'\n[workspace]\n",
    )
    .unwrap();
    std::fs::write(directory.path().join("src/lib.rs"), source).unwrap();
    let output = super::common::cargo(&StylePolicy::default(), directory.path())
        .args(["generate-lockfile", "--offline"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    directory
}

fn only(rules: &[StyleRule]) -> StylePolicy {
    StylePolicy {
        enabled: true,
        extra_rules: rules
            .iter()
            .copied()
            .filter(|rule| !StyleRule::ALL[..13].contains(rule))
            .collect(),
        disabled_rules: StyleRule::ALL[..13]
            .iter()
            .copied()
            .filter(|rule| !rules.contains(rule))
            .collect(),
        ..StylePolicy::default()
    }
}

#[test]
fn real_clippy_distinguishes_standard_aliases_and_same_named_methods() {
    let fixture = fixture(
        "type Maybe = Option<u8>;\nstruct Own;\nimpl Own { fn unwrap(self) -> u8 { 1 } }\npub fn f(value: Maybe) -> u8 { let _ = Own.unwrap(); value.unwrap() }\n",
    );
    let workspace = Workspace::load(fixture.path()).unwrap();
    let report = super::check(
        fixture.path(),
        &workspace,
        &AnalysisSettings::default(),
        &only(&[StyleRule::Unwrap]),
    );
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(report.diagnostics.len(), 1, "{:?}", report.diagnostics);
    assert_eq!(report.diagnostics[0].rule, StyleRule::Unwrap);
}

#[test]
fn changed_clippy_configuration_rechecks_cached_sources() {
    let fixture = fixture(
        "pub fn f() { let _ = std::panic::catch_unwind(|| ()); let _ = std::sync::PoisonError::new(1).into_inner(); }\n",
    );
    let workspace = Workspace::load(fixture.path()).unwrap();
    let settings = AnalysisSettings::default();
    let first = super::check(
        fixture.path(),
        &workspace,
        &settings,
        &only(&[StyleRule::CatchUnwind]),
    );
    assert!(first.errors.is_empty(), "{:?}", first.errors);
    assert_eq!(first.diagnostics.len(), 1, "{:?}", first.diagnostics);
    assert_eq!(first.diagnostics[0].rule, StyleRule::CatchUnwind);
    let second = super::check(
        fixture.path(),
        &workspace,
        &settings,
        &only(&[StyleRule::PoisonAccess]),
    );
    assert!(second.errors.is_empty(), "{:?}", second.errors);
    assert_eq!(second.diagnostics.len(), 1, "{:?}", second.diagnostics);
    assert_eq!(second.diagnostics[0].rule, StyleRule::PoisonAccess);
}

#[test]
fn default_probe_distinguishes_missing_impl_and_invalid_selector_and_preserves_lock() {
    let fixture =
        fixture("pub struct Missing;\n#[derive(Default)] pub struct Good;\nstruct Private;\n");
    let workspace = Workspace::load(fixture.path()).unwrap();
    let settings = AnalysisSettings::default();
    let before = std::fs::read(fixture.path().join("Cargo.lock")).unwrap();
    let mut policy = only(&[StyleRule::OptionsDefault]);
    policy.options_structs.push("tool-fixture::Missing".into());
    let missing = super::check(fixture.path(), &workspace, &settings, &policy);
    assert!(missing.errors.is_empty(), "{:?}", missing.errors);
    assert_eq!(missing.diagnostics.len(), 1, "{:?}", missing.diagnostics);
    policy.options_structs[0] = "tool-fixture::Good".into();
    let good = super::check(fixture.path(), &workspace, &settings, &policy);
    assert!(good.errors.is_empty(), "{:?}", good.errors);
    assert!(good.diagnostics.is_empty(), "{:?}", good.diagnostics);
    policy.options_structs[0] = "tool-fixture::Private".into();
    let private = super::check(fixture.path(), &workspace, &settings, &policy);
    assert!(!private.errors.is_empty());
    assert!(private.diagnostics.is_empty());
    assert_eq!(
        before,
        std::fs::read(fixture.path().join("Cargo.lock")).unwrap()
    );
}

#[test]
fn cross_package_feature_requests_are_accepted_for_each_package() {
    let fixture = fixture("pub fn f() {}\n");
    std::fs::write(
        fixture.path().join("Cargo.toml"),
        "[workspace]\nmembers=['left','right']\nresolver='2'\n",
    )
    .unwrap();
    for name in ["left", "right"] {
        std::fs::create_dir_all(fixture.path().join(name).join("src")).unwrap();
        std::fs::write(fixture.path().join(name).join("Cargo.toml"), format!("[package]\nname='{name}'\nversion='0.1.0'\nedition='2021'\n[features]\nspecial=[]\n")).unwrap();
        std::fs::write(
            fixture.path().join(name).join("src/lib.rs"),
            "#[cfg(feature=\"special\")] pub fn f(value: Option<u8>) -> u8 { value.unwrap() }\n",
        )
        .unwrap();
    }
    let output = super::common::cargo(&StylePolicy::default(), fixture.path())
        .args(["generate-lockfile", "--offline"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let workspace = Workspace::load(fixture.path()).unwrap();
    let settings = AnalysisSettings {
        features: vec!["right/special".into()],
        ..AnalysisSettings::default()
    };
    let report = super::check(
        fixture.path(),
        &workspace,
        &settings,
        &only(&[StyleRule::Unwrap]),
    );
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert!(!report.diagnostics.is_empty());
}

#[test]
fn configured_rustflags_are_not_silently_replaced_by_analysis_cfg() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join(".cargo")).unwrap();
    let path = directory.path().join(".cargo/config.toml");
    for config in [
        "[build]\nrustflags=['--cfg', 'project_flag']\n",
        "[target.'cfg(windows)']\nrustflags=['-C', 'target-feature=+crt-static']\n",
    ] {
        std::fs::write(&path, config).unwrap();
        let error = super::common::guard_configured_rustflags(directory.path()).unwrap_err();
        assert!(error.contains("CARGO_ENCODED_RUSTFLAGS"), "{error}");
        assert!(error.contains("analysis.cfg"), "{error}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), config);
    }
}

#[test]
fn empty_cfg_does_not_override_or_reject_existing_cargo_flags() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join(".cargo")).unwrap();
    std::fs::write(
        directory.path().join(".cargo/config.toml"),
        "[build]\nrustflags=['--cfg', 'project_flag']\n",
    )
    .unwrap();
    let mut command = super::common::cargo(&StylePolicy::default(), directory.path());
    super::common::cfg_environment(&mut command, directory.path(), &AnalysisSettings::default())
        .unwrap();
    assert!(
        !command
            .get_envs()
            .any(|(name, _)| name == "CARGO_ENCODED_RUSTFLAGS")
    );
}

#[test]
fn configured_cfg_activates_compiler_checked_code() {
    let fixture =
        fixture("#[cfg(xtask_style_cfg)] pub fn f(value: Option<u8>) -> u8 { value.unwrap() }\n");
    let workspace = Workspace::load(fixture.path()).unwrap();
    let settings = AnalysisSettings {
        cfg: vec!["xtask_style_cfg".into()],
        ..AnalysisSettings::default()
    };
    let report = super::check(
        fixture.path(),
        &workspace,
        &settings,
        &only(&[StyleRule::Unwrap]),
    );
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(report.diagnostics.len(), 1, "{:?}", report.diagnostics);
    assert_eq!(report.diagnostics[0].rule, StyleRule::Unwrap);
}
