//! Role: 整形器と型検査器を呼び出し、親workspaceのstyle診断を集約する。
use std::path::Path;

use xtask_workspace::{AnalysisSettings, StylePolicy, StyleRule, Workspace};

use crate::model::StyleDiagnostic;

mod clippy;
mod common;
mod probe;

pub(crate) use common::ToolReport;
use common::{cargo, failure};

pub(crate) fn check(
    root: &Path,
    workspace: &Workspace,
    settings: &AnalysisSettings,
    policy: &StylePolicy,
) -> ToolReport {
    let mut report = ToolReport::default();
    if policy.is_enabled(StyleRule::Format) {
        formatting(root, policy, &mut report);
    }
    clippy::check(root, workspace, settings, policy, &mut report);
    if policy.is_enabled(StyleRule::OptionsDefault) {
        probe::check(root, workspace, settings, policy, &mut report);
    }
    report.diagnostics.sort();
    report.diagnostics.dedup();
    report
}

fn formatting(root: &Path, policy: &StylePolicy, report: &mut ToolReport) {
    let output = cargo(policy, root)
        .args(["fmt", "--all", "--", "--check", "--config", "max_width=100"])
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
                    message: "rustfmt formatting differs (max_width = 100)".into(),
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
