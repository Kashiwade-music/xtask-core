//! Role: packageごとのClippy設定を保ち、選択した型検査lintの診断を取り込む。
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use xtask_workspace::{AnalysisSettings, StylePolicy, StyleRule, Workspace};

use super::common::{ToolReport, cargo, cfg_environment, failure, feature_args};
use crate::model::StyleDiagnostic;

pub(super) fn check(
    root: &Path,
    workspace: &Workspace,
    settings: &AnalysisSettings,
    policy: &StylePolicy,
    report: &mut ToolReport,
) {
    // A workspace run preserves cross-package feature selection and dependency
    // feature unification. Packages sharing configuration use one invocation;
    // distinct package configs get independent runs and package-filtered output.
    let mut groups = BTreeMap::<(String, bool), BTreeSet<&str>>::new();
    for package in &workspace.packages {
        let api = policy.api_packages.contains(&package.name);
        if lints(policy, api).is_empty() {
            continue;
        }
        let directory = package.manifest_path.parent().unwrap_or(root);
        match merged_config(directory, policy) {
            Ok(config) => {
                groups.entry((config, api)).or_default().insert(&package.id);
            }
            Err(error) => report
                .errors
                .push(format!("style Clippy {}: {error}", package.name)),
        }
    }
    for ((configuration, api), selected) in groups {
        let config = match tempfile::tempdir() {
            Ok(config) => config,
            Err(error) => {
                report
                    .errors
                    .push(format!("style Clippy temporary config: {error}"));
                continue;
            }
        };
        if let Err(error) = fs::write(config.path().join("clippy.toml"), configuration) {
            report
                .errors
                .push(format!("style Clippy temporary config: {error}"));
            continue;
        }
        let lints = lints(policy, api);
        let mut command = cargo(policy, root);
        command.args([
            "clippy",
            "--locked",
            "--offline",
            "--workspace",
            "--all-targets",
            "--message-format=json",
        ]);
        feature_args(&mut command, settings);
        if let Err(error) = cfg_environment(&mut command, root, settings) {
            report.errors.push(format!("style Clippy: {error}"));
            continue;
        }
        command.env("CLIPPY_CONF_DIR", config.path());
        // Isolate this invocation from normal checks with different Clippy config.
        command.arg("--").args(["-D", "unknown_lints"]);
        for lint in lints.keys() {
            // force-warn allows policy exceptions to be applied by xtask without
            // cargo stopping on a lint before the remaining diagnostics exist.
            command.args(["--force-warn", lint]);
        }
        match command.output() {
            Err(error) => report
                .errors
                .push(format!("style Clippy: cannot run cargo clippy: {error}")),
            Ok(output) => {
                let compiler_error =
                    parse_selected_output(root, &output.stdout, &lints, Some(&selected), report);
                if !output.status.success() && !compiler_error {
                    report.errors.push(failure("style Clippy", &output));
                } else if output.status.success() && !output.stderr.is_empty() {
                    // Cargo progress lines need not be repeated in the user's report.
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    if stderr.contains("error:") {
                        report
                            .errors
                            .push(format!("style Clippy: {}", stderr.trim()));
                    }
                }
            }
        }
    }
}

pub(super) fn lints(policy: &StylePolicy, api: bool) -> BTreeMap<&'static str, StyleRule> {
    use StyleRule::*;
    let mut output = BTreeMap::new();
    for (rule, names) in [
        (UnstableFeatures, &["unstable_features"][..]),
        (Unwrap, &["clippy::unwrap_used"][..]),
        (CheckedUnwrap, &["clippy::unnecessary_unwrap"][..]),
        (
            FlexibleReferences,
            &[
                "clippy::borrowed_box",
                "clippy::ptr_arg",
                "clippy::ref_option",
            ][..],
        ),
        (DerivableImpls, &["clippy::derivable_impls"][..]),
        (NewDefault, &["clippy::new_without_default"][..]),
        (LenIsEmpty, &["clippy::len_without_is_empty"][..]),
        (MissingDocs, &["missing_docs"][..]),
        (SafetyDocs, &["clippy::missing_safety_doc"][..]),
        (SafetyComments, &["clippy::undocumented_unsafe_blocks"][..]),
    ] {
        if policy.is_enabled(rule) {
            output.extend(names.iter().map(|name| (*name, rule)));
        }
    }
    if api && policy.is_enabled(NonExhaustive) {
        output.insert("clippy::exhaustive_enums", NonExhaustive);
        output.insert("clippy::exhaustive_structs", NonExhaustive);
    }
    if policy.is_enabled(CatchUnwind) || policy.is_enabled(PoisonAccess) {
        // The reason embedded in each configured entry selects the actual rule.
        output.insert("clippy::disallowed_methods", CatchUnwind);
    }
    output
}

fn locate_config(start: &Path) -> Result<Option<PathBuf>, String> {
    let starting = std::env::var_os("CLIPPY_CONF_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| start.to_owned());
    for directory in starting.ancestors() {
        let hidden = directory.join(".clippy.toml");
        let ordinary = directory.join("clippy.toml");
        for path in [hidden, ordinary] {
            match path.try_exists() {
                Ok(true) => return Ok(Some(path)),
                Ok(false) => {}
                Err(error) => return Err(format!("{}: {error}", path.display())),
            }
        }
    }
    Ok(None)
}

pub(super) fn config_table(
    mut table: toml::Table,
    policy: &StylePolicy,
) -> Result<toml::Table, String> {
    // Test code may unwrap: `#[test]` functions and `#[cfg(test)]` modules are
    // exempted here, and integration test targets when parsing the output.
    if policy.is_enabled(StyleRule::Unwrap) {
        table.insert("allow-unwrap-in-tests".into(), true.into());
    }
    // Unsafe API documentation deliberately includes private items.
    if policy.is_enabled(StyleRule::SafetyDocs) {
        table.insert("check-private-items".into(), true.into());
    }
    if policy.is_enabled(StyleRule::FlexibleReferences) {
        table.insert("avoid-breaking-exported-api".into(), false.into());
    }
    let mut blocked = Vec::new();
    if policy.is_enabled(StyleRule::CatchUnwind) {
        blocked.push(("std::panic::catch_unwind", "xtask-style:catch-unwind"));
    }
    if policy.is_enabled(StyleRule::PoisonAccess) {
        for method in ["into_inner", "get_ref", "get_mut"] {
            blocked.push((
                match method {
                    "into_inner" => "std::sync::PoisonError::into_inner",
                    "get_ref" => "std::sync::PoisonError::get_ref",
                    _ => "std::sync::PoisonError::get_mut",
                },
                "xtask-style:poison-access",
            ));
        }
    }
    if !blocked.is_empty() {
        let array = table
            .entry("disallowed-methods")
            .or_insert_with(|| toml::Value::Array(Vec::new()))
            .as_array_mut()
            .ok_or("disallowed-methods must be an array")?;
        for (path, reason) in blocked {
            let mut entry = toml::Table::new();
            entry.insert("path".into(), path.into());
            entry.insert("reason".into(), reason.into());
            array.push(toml::Value::Table(entry));
        }
    }
    Ok(table)
}

fn merged_config(directory: &Path, policy: &StylePolicy) -> Result<String, String> {
    let original = locate_config(directory)?;
    let table = match original {
        Some(path) => fs::read_to_string(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?
            .parse::<toml::Table>()
            .map_err(|error| format!("{}: {error}", path.display()))?,
        None => toml::Table::new(),
    };
    let table = config_table(table, policy)?;
    toml::to_string(&table).map_err(|error| error.to_string())
}

#[cfg(test)]
pub(super) fn parse_output(
    root: &Path,
    stdout: &[u8],
    lints: &BTreeMap<&str, StyleRule>,
    report: &mut ToolReport,
) -> bool {
    parse_selected_output(root, stdout, lints, None, report)
}

fn parse_selected_output(
    root: &Path,
    stdout: &[u8],
    lints: &BTreeMap<&str, StyleRule>,
    selected: Option<&BTreeSet<&str>>,
    report: &mut ToolReport,
) -> bool {
    let mut compiler_error = false;
    for line in String::from_utf8_lossy(stdout).lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if value["reason"] != "compiler-message" {
            continue;
        }
        let message = &value["message"];
        let code = message["code"]["code"].as_str().unwrap_or("");
        let text = message["rendered"]
            .as_str()
            .or_else(|| message["message"].as_str())
            .unwrap_or("compiler diagnostic");
        let rule = if code == "clippy::disallowed_methods" {
            if text.contains("xtask-style:poison-access") {
                Some(StyleRule::PoisonAccess)
            } else if text.contains("xtask-style:catch-unwind") {
                Some(StyleRule::CatchUnwind)
            } else {
                None
            }
        } else {
            lints.get(code).copied()
        };
        let selected_package = selected.is_none_or(|packages| {
            value["package_id"]
                .as_str()
                .is_some_and(|id| packages.contains(id))
        });
        // Integration tests are test code throughout, including helpers that
        // Clippy's `allow-unwrap-in-tests` does not recognize.
        let test_target = value["target"]["kind"]
            .as_array()
            .is_some_and(|kinds| kinds.iter().any(|kind| kind == "test"));
        if rule == Some(StyleRule::Unwrap) && test_target {
            continue;
        }
        if let Some(rule) = rule.filter(|_| selected_package) {
            if let Some(span) = message["spans"]
                .as_array()
                .and_then(|spans| spans.iter().find(|span| span["is_primary"] == true))
            {
                // Selected lints are forced to warnings, but xtask reports them as
                // style errors, so the compiler's level prefix would be misleading.
                let text = text.trim();
                report.diagnostics.push(StyleDiagnostic {
                    rule,
                    path: root.join(span["file_name"].as_str().unwrap_or("Cargo.toml")),
                    line: span["line_start"].as_u64().unwrap_or(1) as usize,
                    column: span["column_start"].as_u64().unwrap_or(1) as usize,
                    message: text.strip_prefix("warning: ").unwrap_or(text).to_owned(),
                    item: None,
                });
            } else {
                // Unknown lint errors and locationless selected diagnostics must
                // not disappear just because there is no source span.
                report.errors.push(text.trim().to_owned());
                compiler_error = true;
            }
        } else if message["level"] == "error" || message["level"] == "failure-note" {
            report.errors.push(text.trim().to_owned());
            compiler_error = true;
        }
    }
    compiler_error
}
