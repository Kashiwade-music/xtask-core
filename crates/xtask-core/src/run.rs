//! Role: Cargo・Rust設定を解決し、共通ルールの検査と図の出力を統括する。
use std::collections::{BTreeMap, BTreeSet};
use std::process;

use xtask_modules::AnalysisOptions;
use xtask_workspace::{AnalysisSettings, Policy, Workspace};

use crate::{Analysis, Config, Report, TargetGraph, graphs};

/// Analyze active Cargo features and Rust target cfg. Unit-test configurations
/// are separate graphs, so mutually exclusive production/test edges never merge.
pub fn analyze(config: &Config) -> Result<Analysis, String> {
    let policy = load_policy(config)?.unwrap_or_default();
    let max_role_chars = role_limit(config, &policy)?;
    let settings = config.analysis.as_ref().unwrap_or(&policy.analysis);
    let workspace = Workspace::load(&config.root)?;
    analyze_workspace(config, settings, workspace, max_role_chars)
}

fn load_policy(config: &Config) -> Result<Option<Policy>, String> {
    if config
        .policy
        .try_exists()
        .map_err(|error| format!("{}: {error}", config.policy.display()))?
    {
        Policy::load(&config.policy).map(Some)
    } else {
        Ok(None)
    }
}

fn role_limit(config: &Config, policy: &Policy) -> Result<usize, String> {
    let limit = config
        .max_role_chars
        .unwrap_or(policy.limits.max_role_chars);
    if limit == 0 {
        return Err("max_role_chars must be a positive integer".into());
    }
    Ok(limit)
}

fn analyze_workspace(
    config: &Config,
    settings: &AnalysisSettings,
    workspace: Workspace,
    max_role_chars: usize,
) -> Result<Analysis, String> {
    let resolved = Workspace::resolved_features(
        &config.root,
        &settings.features,
        settings.default_features,
        settings.all_features,
        settings.target.as_deref(),
    )?;
    let cfg = rustc_cfg(config, settings)?;
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
                vec![true]
            } else if unit_test {
                vec![false, true]
            } else {
                vec![false]
            };
            for test in modes {
                let mut active_cfg = cfg.clone();
                let procedural = target.kind.iter().any(|kind| kind == "proc-macro");
                if procedural || target.kind.iter().any(|kind| kind == "custom-build") {
                    // Cargo builds scripts and procedural macros for the host.
                    let mut host = settings.clone();
                    host.target = None;
                    active_cfg = rustc_cfg(config, &host)?;
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
                    trusted_attributes: settings.trusted_attributes.clone(),
                    trusted_derives: settings.trusted_derives.clone(),
                    trusted_macros: settings.trusted_macros.clone(),
                    max_role_chars,
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
    if !settings.trusted_attributes.is_empty()
        || !settings.trusted_derives.is_empty()
        || !settings.trusted_macros.is_empty()
    {
        report.notes.push(format!("source-only macro assumptions: attributes {:?}, derives {:?}, macros {:?}; generated code is not analyzed", settings.trusted_attributes, settings.trusted_derives, settings.trusted_macros));
    }
    Ok(Analysis {
        workspace,
        targets,
        report,
    })
}

fn rustc_cfg(config: &Config, settings: &AnalysisSettings) -> Result<BTreeSet<String>, String> {
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let mut command = process::Command::new(rustc);
    command.args(["--print", "cfg"]).current_dir(&config.root);
    if let Some(target) = &settings.target {
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
    cfg.extend(settings.cfg.iter().cloned());
    Ok(cfg)
}

/// Execute every repository check and write crate/module SVG artifacts, unless
/// `no_write` is set. That mode validates graph structure without invoking Node.
/// Rule violations and independent output failures are collected in the report.
pub fn run(config: &Config) -> Result<Report, String> {
    if config.prune && config.no_write {
        return Err("prune cannot be combined with no_write".into());
    }
    let policy = Policy::load(&config.policy)?;
    let output = config.output.as_ref().unwrap_or(&policy.output);
    if output.as_os_str().to_string_lossy().trim().is_empty() {
        return Err("output must be a nonempty path".into());
    }
    let graph_config = Config {
        output: Some(output.clone()),
        ..config.clone()
    };
    let max_role_chars = role_limit(config, &policy)?;
    let settings = config.analysis.as_ref().unwrap_or(&policy.analysis);
    let workspace = Workspace::load(&config.root)?;
    let mut analysis = match analyze_workspace(config, settings, workspace.clone(), max_role_chars)
    {
        Ok(analysis) => analysis,
        Err(error) => Analysis {
            workspace,
            targets: Vec::new(),
            report: Report {
                errors: vec![format!("module analysis: {error}")],
                ..Report::default()
            },
        },
    };
    analysis
        .report
        .errors
        .extend(analysis.workspace.check_visibility(&policy));
    check_lines(&mut analysis, policy.limits.max_code_lines);
    if let Err(error) = graphs::modules(
        &graph_config,
        &analysis.targets,
        policy.diagrams.targets,
        &mut analysis.report,
    ) {
        analysis
            .report
            .errors
            .push(format!("module diagrams: {error}"));
    }
    if let Err(error) = graphs::dependencies(
        &graph_config,
        &analysis.workspace,
        &mut analysis.report,
        max_role_chars,
    ) {
        analysis
            .report
            .errors
            .push(format!("crate diagram: {error}"));
    }
    analysis.report.notes.push(format!(
        "analyzed {} workspace crates and {} target/configuration graphs",
        analysis.workspace.packages.len(),
        analysis.targets.len()
    ));
    Ok(analysis.report)
}

fn check_lines(analysis: &mut Analysis, limit: usize) {
    let mut files = BTreeSet::new();
    for package in &analysis.workspace.packages {
        files.extend(package.targets.iter().map(|target| target.src_path.clone()));
        let Some(directory) = package.manifest_path.parent() else {
            analysis.report.errors.push(format!(
                "source discovery: {}: manifest has no parent",
                package.manifest_path.display()
            ));
            continue;
        };
        match xtask_source::rust_files(directory) {
            Ok(discovered) => files.extend(discovered),
            Err(error) => analysis
                .report
                .errors
                .push(format!("source discovery: {error}")),
        }
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
    let mut identities = BTreeMap::new();
    for file in files {
        match std::fs::canonicalize(&file) {
            Ok(identity) => {
                identities.entry(identity).or_insert(file);
            }
            Err(error) => analysis
                .report
                .errors
                .push(format!("source line check: {}: {error}", file.display())),
        }
    }
    for file in identities.into_values() {
        match xtask_source::check_files(std::slice::from_ref(&file), limit) {
            Ok(violations) => {
                for violation in violations {
                    analysis.report.errors.push(format!(
                        "{}: {} code lines (limit {}); split by responsibility",
                        violation.path.display(),
                        violation.count,
                        violation.limit
                    ));
                }
            }
            Err(error) => analysis
                .report
                .errors
                .push(format!("source line check: {error}")),
        }
    }
}
