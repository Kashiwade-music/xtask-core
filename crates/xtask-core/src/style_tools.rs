//! Role: 整形器と型検査器を呼び出し、親workspaceのstyle診断を集約する。
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use xtask_workspace::{AnalysisSettings, StylePolicy, StyleRule, Workspace};

use crate::model::StyleDiagnostic;

mod clippy;
mod common;
mod probe;

pub(crate) use common::ToolReport;
use common::{cargo, failure, rustfmt};

/// The Google guide's column limit. Narrower project settings also satisfy it.
const MAX_WIDTH: usize = 100;

pub(crate) fn check(
    root: &Path,
    files: &[PathBuf],
    workspace: &Workspace,
    settings: &AnalysisSettings,
    policy: &StylePolicy,
) -> ToolReport {
    let mut report = ToolReport::default();
    if policy.is_enabled(StyleRule::Format) {
        width_limit(root, files, policy, &mut report);
        formatting(root, policy, &mut report);
    }
    clippy::check(root, workspace, settings, policy, &mut report);
    if policy.is_enabled(StyleRule::OptionsDefault) {
        probe::check(root, workspace, settings, policy, &mut report);
    }
    report.diagnostics.sort();
    report.diagnostics.dedup();
    // Each Clippy configuration group compiles the whole workspace, so a
    // compile error would otherwise be repeated once per group.
    let mut seen = BTreeSet::new();
    report.errors.retain(|error| seen.insert(error.clone()));
    report
}

/// The project's rustfmt configuration stays authoritative: overriding its
/// width would demand a layout that the repository's own `cargo fmt` never
/// produces. Instead, the effective width must not exceed the guide's limit.
fn width_limit(root: &Path, files: &[PathBuf], policy: &StylePolicy, report: &mut ToolReport) {
    let mut directories = BTreeMap::new();
    for file in files {
        let file = std::fs::canonicalize(file).unwrap_or_else(|_| file.clone());
        if let Some(directory) = file.parent() {
            directories
                .entry(directory.to_path_buf())
                .or_insert(file.clone());
        }
    }
    let mut reported = BTreeSet::new();
    for (directory, file) in directories {
        let output = match rustfmt(policy, root)
            .args(["--print-config", "current"])
            .arg(&file)
            .output()
        {
            Ok(output) if output.status.success() => output,
            Ok(output) => {
                report
                    .errors
                    .push(failure("style format configuration", &output));
                return;
            }
            Err(error) => {
                report.errors.push(format!(
                    "style format configuration: cannot run rustfmt: {error}"
                ));
                return;
            }
        };
        let width = String::from_utf8_lossy(&output.stdout)
            .lines()
            .find_map(|line| {
                line.strip_prefix("max_width = ")?
                    .trim()
                    .parse::<usize>()
                    .ok()
            });
        let Some(width) = width else {
            report.errors.push(format!(
                "style format configuration: rustfmt reported no max_width for {}",
                file.display()
            ));
            return;
        };
        if width <= MAX_WIDTH {
            continue;
        }
        // Report the nearest project configuration once, not every directory.
        let config = directory
            .ancestors()
            .take_while(|ancestor| ancestor.starts_with(root))
            .flat_map(|ancestor| {
                [
                    ancestor.join(".rustfmt.toml"),
                    ancestor.join("rustfmt.toml"),
                ]
            })
            .find(|candidate| candidate.is_file())
            .unwrap_or_else(|| root.join("rustfmt.toml"));
        if reported.insert(config.clone()) {
            report.diagnostics.push(StyleDiagnostic {
                rule: StyleRule::Format,
                path: config,
                line: 1,
                column: 1,
                message: format!(
                    "effective rustfmt max_width = {width} exceeds the {MAX_WIDTH}-column limit; set max_width to {MAX_WIDTH} or less"
                ),
                item: None,
            });
        }
    }
}

fn formatting(root: &Path, policy: &StylePolicy, report: &mut ToolReport) {
    let output = cargo(policy, root)
        .args(["fmt", "--all", "--", "--check"])
        .output();
    match output {
        Err(error) => report
            .errors
            .push(format!("style format: cannot run cargo fmt: {error}")),
        Ok(output) if output.status.success() => {}
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let fatal = output.status.code().is_none_or(|code| code > 1)
                || stderr.lines().chain(stdout.lines()).any(|line| {
                    let line = line.trim_start();
                    line.starts_with("error:")
                        || line.starts_with("Error:")
                        || line.starts_with("Error writing files:")
                });
            if fatal {
                report.errors.push(failure("style format", &output));
            }
            let mut found = false;
            for line in stdout.lines() {
                // rustfmt --check prefixes every hunk with `Diff in PATH:LINE:`.
                // Split from the end so Windows drive letters are preserved.
                let Some(location) = line
                    .strip_prefix("Diff in ")
                    .and_then(|line| line.strip_suffix(':'))
                else {
                    continue;
                };
                let Some((path, line)) = location.rsplit_once(':') else {
                    continue;
                };
                let Ok(line) = line.parse::<usize>() else {
                    continue;
                };
                found = true;
                report.diagnostics.push(StyleDiagnostic {
                    rule: StyleRule::Format,
                    path: root.join(path),
                    line,
                    column: 1,
                    message: "rustfmt formatting differs; run `cargo fmt --all`".into(),
                    item: None,
                });
            }
            if !found && !fatal {
                report.errors.push(format!(
                    "style format: cargo fmt failed without a formatting diff:\n{stdout}{stderr}"
                ));
            } else if !fatal && !stderr.trim().is_empty() {
                report.notes.push(format!("cargo fmt: {}", stderr.trim()));
            }
        }
    }
}

#[cfg(test)]
mod tests;
