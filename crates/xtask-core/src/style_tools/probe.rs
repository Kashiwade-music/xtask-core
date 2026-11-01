//! Role: 明示された公開options型のDefault実装を独立したコンパイルで検査する。
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use xtask_workspace::{AnalysisSettings, StylePolicy, StyleRule, Workspace};

use super::common::{ToolReport, cargo, cfg_environment};
use crate::model::StyleDiagnostic;

pub(super) fn check(
    root: &Path,
    workspace: &Workspace,
    settings: &AnalysisSettings,
    policy: &StylePolicy,
    report: &mut ToolReport,
) {
    let features = match Workspace::resolved_features(
        root,
        &settings.features,
        settings.default_features,
        settings.all_features,
        settings.target.as_deref(),
    ) {
        Ok(features) => features,
        Err(error) => {
            report
                .errors
                .push(format!("style options-default: {error}"));
            return;
        }
    };
    for selector in &policy.options_structs {
        if let Err(error) = check_one(
            root, workspace, settings, policy, &features, selector, report,
        ) {
            report
                .errors
                .push(format!("style options-default `{selector}`: {error}"));
        }
    }
}

fn check_one(
    root: &Path,
    workspace: &Workspace,
    settings: &AnalysisSettings,
    policy: &StylePolicy,
    features: &BTreeMap<String, BTreeSet<String>>,
    selector: &str,
    report: &mut ToolReport,
) -> Result<(), String> {
    let (name, item) = selector
        .split_once("::")
        .ok_or("expected package::module::Type")?;
    let package = workspace
        .packages
        .iter()
        .find(|package| package.name == name)
        .ok_or_else(|| format!("package `{name}` is not a workspace member"))?;
    let library = package
        .targets
        .iter()
        .find(|target| {
            target
                .kind
                .iter()
                .any(|kind| matches!(kind.as_str(), "lib" | "rlib"))
        })
        .ok_or("a public Rust library target is required")?;
    let directory = package
        .manifest_path
        .parent()
        .ok_or("package manifest has no parent")?;
    let selected_features = features
        .get(name)
        .ok_or("feature resolution omitted this package")?;
    let temporary = tempfile::tempdir().map_err(|error| error.to_string())?;
    let scratch = temporary.path();
    fs::create_dir(scratch.join("src")).map_err(|error| error.to_string())?;
    let manifest = manifest(root, directory, name, selected_features)?;
    fs::write(scratch.join("Cargo.toml"), manifest).map_err(|error| error.to_string())?;
    // This lockfile copy preserves the parent's resolved versions, while Cargo
    // may add the throwaway probe package to its own copy. The parent stays intact.
    fs::copy(root.join("Cargo.lock"), scratch.join("Cargo.lock"))
        .map_err(|error| format!("copy parent Cargo.lock: {error}"))?;
    let source = format!(
        "fn require_default<T: Default>() {{}}\nfn main() {{ require_default::<checked::{item}>(); }}\n"
    );
    fs::write(scratch.join("src/main.rs"), source).map_err(|error| error.to_string())?;
    let mut command = cargo(policy, root);
    command
        .args([
            "check",
            "--offline",
            "--message-format=json",
            "--manifest-path",
        ])
        .arg(scratch.join("Cargo.toml"));
    command
        .arg("--target-dir")
        .arg(root.join("target/xtask-style-probes"));
    if let Some(target) = &settings.target {
        command.args(["--target", target]);
    }
    cfg_environment(&mut command, root, settings)?;
    let output = command
        .output()
        .map_err(|error| format!("cannot run Default probe: {error}"))?;
    if !output.status.success() {
        let mut missing_default = Vec::new();
        let mut operational_errors = Vec::new();
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            if value["reason"] != "compiler-message" || value["message"]["level"] != "error" {
                continue;
            }
            let message = &value["message"];
            let text = message["rendered"]
                .as_str()
                .or_else(|| message["message"].as_str())
                .unwrap_or("compiler diagnostic")
                .trim()
                .to_owned();
            if value["target"]["name"] == "xtask-style-default-probe"
                && message["code"]["code"] == "E0277"
                && text.contains("Default")
            {
                missing_default.push(text);
            } else {
                operational_errors.push(text);
            }
        }
        // A typo, private type, unavailable target or failed dependency is a
        // failed check, not a suppressible claim that Default is missing.
        if !operational_errors.is_empty() || missing_default.is_empty() {
            return Err(if operational_errors.is_empty() {
                format!(
                    "Default probe could not complete:\n{}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                )
            } else {
                operational_errors.join("\n")
            });
        }
        report.diagnostics.push(StyleDiagnostic {
            rule: StyleRule::OptionsDefault,
            path: library.src_path.clone(),
            line: 1,
            column: 1,
            message: format!(
                "`{selector}` does not implement Default:\n{}",
                missing_default.join("\n")
            ),
            item: Some(selector.into()),
        });
    }
    Ok(())
}

fn manifest(
    root: &Path,
    directory: &Path,
    package: &str,
    features: &BTreeSet<String>,
) -> Result<String, String> {
    let mut dependency = toml::Table::new();
    dependency.insert("package".into(), package.into());
    dependency.insert(
        "path".into(),
        directory.to_string_lossy().into_owned().into(),
    );
    dependency.insert("default-features".into(), false.into());
    dependency.insert(
        "features".into(),
        toml::Value::Array(features.iter().cloned().map(toml::Value::String).collect()),
    );
    let mut dependencies = toml::Table::new();
    dependencies.insert("checked".into(), dependency.into());
    let mut metadata = toml::Table::new();
    metadata.insert("name".into(), "xtask-style-default-probe".into());
    metadata.insert("version".into(), "0.0.0".into());
    metadata.insert("edition".into(), "2021".into());
    let mut manifest = toml::Table::new();
    manifest.insert("package".into(), metadata.into());
    manifest.insert("workspace".into(), toml::Table::new().into());
    manifest.insert("dependencies".into(), dependencies.into());
    // Root-level overrides do not propagate through path dependencies. Keep the
    // parent's overrides and resolve their relative paths against that root.
    let parent = fs::read_to_string(root.join("Cargo.toml"))
        .map_err(|error| error.to_string())?
        .parse::<toml::Table>()
        .map_err(|error| error.to_string())?;
    for key in ["patch", "replace"] {
        if let Some(value) = parent.get(key) {
            let mut value = value.clone();
            absolute_paths(&mut value, root);
            manifest.insert(key.into(), value);
        }
    }
    toml::to_string(&manifest).map_err(|error| error.to_string())
}

fn absolute_paths(value: &mut toml::Value, root: &Path) {
    if let Some(table) = value.as_table_mut() {
        if let Some(path) = table
            .get_mut("path")
            .and_then(|value| value.as_str().map(str::to_owned))
        {
            table.insert(
                "path".into(),
                root.join(path).to_string_lossy().into_owned().into(),
            );
        }
        for (_, child) in table.iter_mut() {
            absolute_paths(child, root);
        }
    }
}
