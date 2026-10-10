use std::collections::BTreeSet;
use std::process;

use xtask_modules::AnalysisOptions;
use xtask_workspace::{Policy, Workspace};

use crate::{Analysis, Command, Config, Report, TargetGraph, graphs};

/// Analyze active Cargo features and Rust target cfg. Unit-test configurations
/// are separate graphs, so mutually exclusive production/test edges never merge.
pub fn analyze(config: &Config) -> Result<Analysis, String> {
    let workspace = Workspace::load(&config.root)?;
    analyze_workspace(config, workspace)
}

fn analyze_workspace(config: &Config, workspace: Workspace) -> Result<Analysis, String> {
    let resolved = Workspace::resolved_features(
        &config.root,
        &config.features,
        config.default_features,
        config.all_features,
        config.target.as_deref(),
    )?;
    let cfg = rustc_cfg(config)?;
    let mut report = Report::default();
    let mut targets = Vec::new();
    for package in &workspace.packages {
        let features = resolved.get(&package.name).cloned().ok_or_else(|| {
            format!(
                "Cargo feature resolution omitted workspace member `{}`",
                package.name
            )
        })?;
        for target in &package.targets {
            if !matches!(target.edition.as_str(), "2018" | "2021" | "2024") {
                report.errors.push(format!(
                    "{}::{}: module name resolution supports Rust editions 2018/2021/2024; found {}",
                    package.name, target.name, target.edition
                ));
                continue;
            }
            if !target
                .required_features
                .iter()
                .all(|feature| features.contains(feature))
            {
                report.notes.push(format!(
                    "skipped {}::{}: required features are inactive",
                    package.name, target.name
                ));
                continue;
            }
            let kind = target.kind.join("+");
            let standalone_test = target
                .kind
                .iter()
                .any(|kind| kind == "test" || kind == "bench");
            let unit_test = target.test
                && target.kind.iter().any(|kind| {
                    matches!(
                        kind.as_str(),
                        "lib"
                            | "bin"
                            | "example"
                            | "proc-macro"
                            | "rlib"
                            | "staticlib"
                            | "cdylib"
                            | "dylib"
                    )
                });
            let modes: Vec<bool> = if standalone_test {
                if config.tests {
                    vec![true]
                } else {
                    continue;
                }
            } else if unit_test && config.tests {
                vec![false, true]
            } else {
                vec![false]
            };
            for test in modes {
                let mut active_cfg = cfg.clone();
                let procedural = target.kind.iter().any(|kind| kind == "proc-macro");
                if procedural || target.kind.iter().any(|kind| kind == "custom-build") {
                    // Cargo builds scripts and procedural macros for the host.
                    let mut host = config.clone();
                    host.target = None;
                    active_cfg = rustc_cfg(&host)?;
                }
                if procedural {
                    active_cfg.insert("proc_macro".into());
                }
                if test {
                    active_cfg.insert("test".into());
                }
                let options = AnalysisOptions {
                    cfg: active_cfg,
                    features: features.clone(),
                    trusted_attributes: config.trusted_attributes.clone(),
                    trusted_derives: config.trusted_derives.clone(),
                };
                let context = format!(
                    "{}::{} ({kind}{})",
                    package.name,
                    target.name,
                    if test { ", cfg(test)" } else { "" }
                );
                match xtask_modules::analyze(&target.src_path, &options) {
                    Ok(graph) => {
                        for diagnostic in &graph.diagnostics {
                            report.errors.push(format!("{context}: {diagnostic}"));
                        }
                        for cycle in &graph.cycles {
                            report.errors.push(format!(
                                "{context}: module reference cycle among: {}",
                                cycle.join(", ")
                            ));
                        }
                        targets.push(TargetGraph {
                            package: package.name.clone(),
                            target: target.name.clone(),
                            kind: kind.clone(),
                            test,
                            graph,
                        });
                    }
                    Err(error) => report.errors.push(format!("{context}: {error}")),
                }
            }
        }
    }
    if !config.trusted_attributes.is_empty() || !config.trusted_derives.is_empty() {
        report.notes.push(format!("source-only macro assumptions: attributes {:?}, derives {:?}; generated code is not analyzed", config.trusted_attributes, config.trusted_derives));
    }
    Ok(Analysis {
        workspace,
        targets,
        report,
    })
}

fn rustc_cfg(config: &Config) -> Result<BTreeSet<String>, String> {
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let mut command = process::Command::new(rustc);
    command.args(["--print", "cfg"]).current_dir(&config.root);
    if let Some(target) = &config.target {
        command.args(["--target", target]);
    }
    let output = command
        .output()
        .map_err(|error| format!("could not read rustc cfg: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "rustc --print cfg failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let text = String::from_utf8(output.stdout)
        .map_err(|error| format!("rustc cfg is not UTF-8: {error}"))?;
    let mut cfg: BTreeSet<String> = text.lines().map(str::to_owned).collect();
    cfg.extend(config.cfg.iter().cloned());
    Ok(cfg)
}

/// Execute a read-only check or explicitly generate DOT/SVG artifacts.
pub fn run(config: &Config, command: Command) -> Result<Report, String> {
    let workspace = Workspace::load(&config.root)?;
    if command == Command::Dependencies {
        let mut report = Report::default();
        graphs::dependencies(config, &workspace, &mut report)?;
        return Ok(report);
    }
    let mut analysis = analyze_workspace(config, workspace)?;
    if command == Command::Check {
        let policy = Policy::load(&config.policy)?;
        analysis
            .report
            .errors
            .extend(analysis.workspace.check_visibility(&policy));
        let mut files = BTreeSet::new();
        for package in &analysis.workspace.packages {
            let directory = package
                .manifest_path
                .parent()
                .ok_or_else(|| "manifest has no parent".to_owned())?;
            files.extend(xtask_source::rust_files(directory)?);
            files.extend(package.targets.iter().map(|target| target.src_path.clone()));
        }
        for target in &analysis.targets {
            files.extend(
                target
                    .graph
                    .modules
                    .iter()
                    .map(|module| module.file.clone()),
            );
        }
        let violations = xtask_source::check_files(
            &files.into_iter().collect::<Vec<_>>(),
            policy.limits.max_code_lines,
        )?;
        for violation in violations {
            analysis.report.errors.push(format!(
                "{}: {} code lines (limit {}); split by responsibility",
                violation.path.display(),
                violation.count,
                violation.limit
            ));
        }
    } else {
        graphs::modules(config, &analysis.targets, &mut analysis.report)?;
        if command == Command::Graphs {
            graphs::dependencies(config, &analysis.workspace, &mut analysis.report)?;
        }
    }
    analysis.report.notes.push(format!(
        "analyzed {} workspace crates and {} target/configuration graphs",
        analysis.workspace.packages.len(),
        analysis.targets.len()
    ));
    Ok(analysis.report)
}
