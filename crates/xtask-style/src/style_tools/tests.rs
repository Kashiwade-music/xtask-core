//! Role: lintの選択、Clippy設定の保持、compiler診断の分類を検証する。
use std::collections::BTreeMap;
use std::path::Path;

use serde_json::json;
use xtask_policy::{AnalysisSettings, Policy, StyleRule, StyleSettings};

use super::{clippy, common};
use crate::model::StyleReport;

/// Style settings with the basic preset, plus the given `[style]` lines.
fn style(lines: &str) -> StyleSettings {
    Policy::from_toml(&format!("[style]\nenabled=true\n{lines}\n"))
        .unwrap()
        .style()
        .unwrap()
        .clone()
}

#[test]
fn lint_selection_is_explicit_and_non_exhaustive_is_scoped() {
    let policy = style("extra_rules=['non-exhaustive']\napi_packages=['my-api']");
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
    let policy = style("extra_rules=['catch-unwind','poison-access']");
    let merged = clippy::config_table(table, &policy).unwrap();
    assert_eq!(merged["msrv"].as_str(), Some("1.85"));
    assert_eq!(merged["disallowed-names"][0].as_str(), Some("special"));
    assert_eq!(merged["disallowed-methods"].as_array().unwrap().len(), 5);
    assert_eq!(merged["allow-unwrap-in-tests"].as_bool(), Some(true));
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
    let mut report = StyleReport::default();
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
    let mut report = StyleReport::default();
    assert!(clippy::parse_output(
        Path::new("."),
        stdout.as_bytes(),
        &BTreeMap::new(),
        &mut report
    ));
    assert_eq!(report.errors, ["unknown lint"]);
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
        let error = common::guard_configured_rustflags(directory.path()).unwrap_err();
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
    let mut command = common::cargo(&style(""), directory.path());
    common::cfg_environment(&mut command, directory.path(), &AnalysisSettings::default()).unwrap();
    assert!(
        !command
            .get_envs()
            .any(|(name, _)| name == "CARGO_ENCODED_RUSTFLAGS")
    );
}
