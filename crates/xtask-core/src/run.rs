//! Role: 検査の段階を順に実行し、報告と図のファイルへの書出しを統括する。
use std::collections::BTreeMap;
use std::path::PathBuf;

use xtask_style::{StyleInput, StyleReport};

use crate::analysis::analyze_targets;
use crate::archive::write_archive;
use crate::config::Config;
use crate::context::{Context, Environment};
use crate::model::{Report, TargetAnalysis, TargetOutcome};
use crate::sources::{Sources, collect, line_errors};
use crate::visibility::visibility;

/// Analyze the active targets and return their module graphs, with the analysis diagnostics.
/// Nothing is checked beyond the analysis and nothing is written.
pub fn analyze(config: &Config) -> Result<(Vec<TargetAnalysis>, Report), String> {
    let context = Context::load(config)?;
    let environment = Environment::load(&context);
    Ok(analyze_targets(&context, &environment))
}

/// Run every repository check, and write the architecture diagrams when the configured output
/// asks for them. Only a failure to read the policy or the workspace is an `Err`; every other
/// failure is in the returned report.
pub fn run(config: &Config) -> Result<Report, String> {
    let context = Context::load(config)?;
    let environment = Environment::load(&context);
    let (analysis, mut report) = analyze_targets(&context, &environment);
    let analyzed = analysis
        .iter()
        .flat_map(|target| &target.outcomes)
        .filter(|(_, outcome)| matches!(outcome, TargetOutcome::Analyzed(_)))
        .count();
    // The diagram file records the analysis-stage messages, before the checks below add theirs.
    let notices: Vec<String> = report.errors.iter().chain(&report.notes).cloned().collect();
    let Sources {
        files,
        modules,
        errors,
    } = collect(&context, &analysis);
    report.errors.extend(errors);
    report.errors.extend(line_errors(
        &files,
        context.policy.limits().max_code_lines(),
    ));
    report
        .errors
        .extend(visibility(&context.workspace, context.policy.visible_to()));
    if let Some(style) = style_check(&context, &environment, &files, &modules) {
        report.errors.extend(style.errors);
        report.notes.extend(style.notes);
        report.style = style.diagnostics;
    }
    match write_archive(&context, &analysis, notices) {
        Ok(None) => {}
        Ok(Some((path, count))) => {
            report
                .notes
                .push(format!("wrote {} ({count} diagrams)", path.display()));
            report.archive = Some(path);
        }
        Err(error) => report.errors.push(error),
    }
    report.notes.push(format!(
        "analyzed {} workspace crates and {analyzed} target/configuration graphs",
        context.workspace.packages.len()
    ));
    Ok(report)
}

/// The style check, when the policy enables it. It runs even when the analysis environment
/// failed; that failure then reaches the check as its feature error.
fn style_check(
    context: &Context,
    environment: &Result<Environment, String>,
    files: &[PathBuf],
    modules: &BTreeMap<PathBuf, String>,
) -> Option<StyleReport> {
    let style = context.policy.style()?;
    let features = environment
        .as_ref()
        .map(|environment| environment.features.clone())
        .map_err(Clone::clone);
    Some(xtask_style::check(&StyleInput {
        root: &context.workspace.root,
        workspace: &context.workspace,
        analysis: &context.settings,
        features: &features,
        style,
        files,
        modules,
    }))
}
