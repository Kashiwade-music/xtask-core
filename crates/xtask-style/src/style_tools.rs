//! Role: 整形器・Clippy・Default probeを呼び出し、その診断と通知を一つの報告にまとめる。
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use xtask_policy::StyleRule;

use crate::model::{StyleDiagnostic, StyleInput, StyleReport, enabled};

mod clippy;
mod common;
mod probe;

use common::{cargo, failure, rustfmt};

/// The Google guide's column limit. Narrower project settings also satisfy it.
const MAX_WIDTH: usize = 100;

/// Formatting runs beside the compiler checks. Its diagnostics come first in the report.
pub(crate) fn check(input: &StyleInput<'_>) -> StyleReport {
    let (mut report, compiler) = std::thread::scope(|scope| {
        let format = scope.spawn(|| format_check(input));
        let compiler = compiler_checks(input);
        (
            format.join().expect("style format thread panicked"),
            compiler,
        )
    });
    report.append(compiler);
    report.diagnostics.sort();
    report.diagnostics.dedup();
    // Each Clippy configuration group compiles the whole workspace, so a
    // compile error would otherwise be repeated once per group.
    let mut seen = BTreeSet::new();
    report.errors.retain(|error| seen.insert(error.clone()));
    report
}

fn format_check(input: &StyleInput<'_>) -> StyleReport {
    let mut report = StyleReport::default();
    if enabled(input.style, StyleRule::Format) {
        width_limit(input, &mut report);
        formatting(input, &mut report);
    }
    report
}

/// Clippy, then the Default probe when it is enabled.
fn compiler_checks(input: &StyleInput<'_>) -> StyleReport {
    let mut report = StyleReport::default();
    clippy::check(input, &mut report);
    if enabled(input.style, StyleRule::OptionsDefault) {
        probe::check(input, &mut report);
    }
    report
}

/// The project's rustfmt configuration stays authoritative: overriding its
/// width would demand a layout that the repository's own `cargo fmt` never
/// produces. Instead, the effective width must not exceed the guide's limit.
fn width_limit(input: &StyleInput<'_>, report: &mut StyleReport) {
    let root = input.root;
    let mut reported = BTreeSet::new();
    for (directory, file) in config_groups(root, input.files) {
        let output = match rustfmt(input.style, root)
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
        let config = project_config(root, &directory);
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

/// Group the files by the directory of their nearest rustfmt configuration within
/// the root. Files without one share the root. Each group keeps its first file,
/// which rustfmt reads the configuration for.
fn config_groups(root: &Path, files: &[PathBuf]) -> BTreeMap<PathBuf, PathBuf> {
    let mut groups = BTreeMap::new();
    for file in files {
        let directory = file
            .ancestors()
            .skip(1)
            .take_while(|directory| directory.starts_with(root))
            .find(|directory| config_file(directory).is_some())
            .map_or_else(|| root.to_path_buf(), Path::to_path_buf);
        groups.entry(directory).or_insert_with(|| file.clone());
    }
    groups
}

/// The rustfmt configuration in a directory. `.rustfmt.toml` takes precedence.
fn config_file(directory: &Path) -> Option<PathBuf> {
    [".rustfmt.toml", "rustfmt.toml"]
        .into_iter()
        .map(|name| directory.join(name))
        .find(|candidate| candidate.is_file())
}

/// The configuration that a directory's files use, or the root's default location.
fn project_config(root: &Path, directory: &Path) -> PathBuf {
    config_file(directory).unwrap_or_else(|| root.join("rustfmt.toml"))
}

fn formatting(input: &StyleInput<'_>, report: &mut StyleReport) {
    let root = input.root;
    let output = cargo(input.style, root)
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
