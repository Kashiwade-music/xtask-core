//! Role: style外部ツールの実行設定と診断集約に共通する処理を提供する。
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use xtask_workspace::{AnalysisSettings, StylePolicy};

use crate::model::StyleDiagnostic;

#[derive(Default)]
pub(crate) struct ToolReport {
    pub diagnostics: Vec<StyleDiagnostic>,
    pub errors: Vec<String>,
    pub notes: Vec<String>,
}

pub(super) fn cargo(policy: &StylePolicy, root: &Path) -> Command {
    // `cargo +toolchain` is understood by the rustup proxy, not a toolchain's
    // concrete cargo binary supplied by Cargo through CARGO.
    let executable = if policy.toolchain.is_some() {
        std::ffi::OsString::from("cargo")
    } else {
        std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into())
    };
    let mut command = Command::new(executable);
    if let Some(toolchain) = &policy.toolchain {
        command.arg(format!("+{toolchain}"));
        // Explicitly selecting a toolchain must also select its compiler and
        // formatter, even when the caller configured absolute executable paths.
        command
            .env_remove("RUSTC")
            .env_remove("RUSTDOC")
            .env_remove("RUSTFMT");
    }
    command.current_dir(root);
    command
}

/// Run the formatter that `cargo fmt` would select for the same policy.
pub(super) fn rustfmt(policy: &StylePolicy, root: &Path) -> Command {
    let mut command = match &policy.toolchain {
        Some(toolchain) => {
            let mut command = Command::new("rustfmt");
            command.arg(format!("+{toolchain}"));
            command
        }
        None => Command::new(std::env::var_os("RUSTFMT").unwrap_or_else(|| "rustfmt".into())),
    };
    command.current_dir(root);
    command
}

pub(super) fn feature_args(command: &mut Command, settings: &AnalysisSettings) {
    if let Some(target) = &settings.target {
        command.args(["--target", target]);
    }
    if settings.all_features {
        command.arg("--all-features");
    }
    if !settings.default_features {
        command.arg("--no-default-features");
    }
    if !settings.features.is_empty() {
        command.arg("--features").arg(settings.features.join(","));
    }
}

pub(super) fn cfg_environment(
    command: &mut Command,
    root: &Path,
    settings: &AnalysisSettings,
) -> Result<(), String> {
    if settings.cfg.is_empty() {
        return Ok(());
    }
    // Cargo gives encoded flags precedence over RUSTFLAGS. Preserve the active
    // environment flags before appending the explicitly configured cfg values.
    let inherited = match std::env::var("CARGO_ENCODED_RUSTFLAGS") {
        Ok(value) => value,
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err(
                "CARGO_ENCODED_RUSTFLAGS must be UTF-8 when analysis.cfg is configured".into(),
            );
        }
        Err(std::env::VarError::NotPresent) => match std::env::var("RUSTFLAGS") {
            Ok(value) => value.split_whitespace().collect::<Vec<_>>().join("\u{1f}"),
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err("RUSTFLAGS must be UTF-8 when analysis.cfg is configured".into());
            }
            Err(std::env::VarError::NotPresent) => {
                guard_configured_rustflags(root)?;
                String::new()
            }
        },
    };
    let mut flags = if inherited.is_empty() {
        Vec::new()
    } else {
        inherited.split('\u{1f}').map(str::to_owned).collect()
    };
    for cfg in &settings.cfg {
        flags.push("--cfg".into());
        flags.push(cfg.clone());
    }
    command.env("CARGO_ENCODED_RUSTFLAGS", flags.join("\u{1f}"));
    Ok(())
}

/// Decline ambiguous Cargo configuration rather than replacing project flags.
/// This is deliberately conservative: target-specific flags may be inactive on
/// this target, but determining Cargo's complete target matching is its job.
pub(super) fn guard_configured_rustflags(root: &Path) -> Result<(), String> {
    for (name, _) in std::env::vars_os() {
        let name = name.to_string_lossy();
        if name.starts_with("CARGO_TARGET_") && name.ends_with("_RUSTFLAGS") {
            return Err(flag_conflict(&format!("environment variable {name}")));
        }
    }
    let mut configurations = BTreeSet::new();
    for directory in root.ancestors() {
        configurations.insert(directory.join(".cargo/config.toml"));
        configurations.insert(directory.join(".cargo/config"));
    }
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                .map(|home| PathBuf::from(home).join(".cargo"))
        });
    if let Some(home) = cargo_home {
        configurations.insert(home.join("config.toml"));
        configurations.insert(home.join("config"));
    }
    for path in configurations {
        if !path
            .try_exists()
            .map_err(|error| format!("{}: {error}", path.display()))?
        {
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let table = text
            .parse::<toml::Table>()
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let build = table
            .get("build")
            .and_then(toml::Value::as_table)
            .is_some_and(|table| table.contains_key("rustflags"));
        let target = table
            .get("target")
            .and_then(toml::Value::as_table)
            .is_some_and(|targets| {
                targets.values().any(|target| {
                    target
                        .as_table()
                        .is_some_and(|table| table.contains_key("rustflags"))
                })
            });
        if build || target {
            return Err(flag_conflict(&path.display().to_string()));
        }
    }
    Ok(())
}

fn flag_conflict(location: &str) -> String {
    format!(
        "analysis.cfg cannot be appended safely to Cargo rustflags from {location}; set CARGO_ENCODED_RUSTFLAGS (arguments separated by U+001F) or RUSTFLAGS to include the project's existing flags explicitly; no compiler check was run"
    )
}

pub(super) fn failure(label: &str, output: &Output) -> String {
    format!(
        "{label} failed ({}):\n{}{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}
