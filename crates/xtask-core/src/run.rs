//! Role: 検査の段階を順に実行し、報告と図のファイルへの書出しを統括する。
use std::collections::BTreeMap;
use std::path::PathBuf;

use xtask_style::{StyleDiagnostic, StyleInput};

use crate::analysis::analyze_targets;
use crate::archive::write_archive;
use crate::config::Config;
use crate::context::{Context, Environment, FeatureTable};
use crate::model::{Findings, Report, TargetAnalysis, TargetOutcome};
use crate::sources::{Sources, collect, line_errors};
use crate::visibility::visibility;

/// Analyze the active targets and return their module graphs, with the analysis diagnostics.
/// Nothing is checked beyond the analysis and nothing is written.
pub fn analyze(config: &Config) -> Result<(Vec<TargetAnalysis>, Report), String> {
    let context = Context::load(config)?;
    let features = context.features();
    let environment = analysis_environment(&context, &features);
    let (analysis, findings) = analyze_targets(&context, &features, &environment);
    Ok((analysis, findings.into_report()))
}

/// Run every repository check, and write the architecture diagrams when the configured output
/// asks for them. Only a failure to read the policy or the workspace is an `Err`; every other
/// failure is in the returned report.
pub fn run(config: &Config) -> Result<Report, String> {
    let context = Context::load(config)?;
    let features = context.features();
    let environment = analysis_environment(&context, &features);
    let (analysis, mut findings) = analyze_targets(&context, &features, &environment);
    let analyzed = analyzed_count(&analysis);
    // The diagram file records the analysis-stage messages, before the checks below add theirs.
    let notices = findings.messages();
    let Sources {
        files,
        modules,
        errors,
    } = collect(&context, &analysis);
    findings.errors.extend(errors);
    findings.errors.extend(line_errors(
        &files,
        context.policy.limits().max_code_lines(),
    ));
    findings
        .errors
        .extend(visibility(&context.workspace, &context.policy));
    let style_features = style_features(features, &environment);
    let (style_findings, style) = style_check(&context, &style_features, &files, &modules);
    findings.append(style_findings);
    let (archive, written) = archive_findings(write_archive(&context, &analysis, notices));
    findings.append(written);
    findings.notes.push(format!(
        "analyzed {} workspace crates and {analyzed} target/configuration graphs",
        context.workspace.packages.len()
    ));
    Ok(Report {
        style,
        archive,
        ..findings.into_report()
    })
}

/// The analysis environment. It is unavailable when the features are, and otherwise when the
/// cfg of rustc cannot be read.
fn analysis_environment(
    context: &Context,
    features: &Result<FeatureTable, String>,
) -> Result<Environment, String> {
    match features {
        Ok(_) => Environment::load(context),
        Err(error) => Err(error.clone()),
    }
}

/// The features that the style check reads. An unavailable environment reaches the check as its
/// feature error.
fn style_features(
    features: Result<FeatureTable, String>,
    environment: &Result<Environment, String>,
) -> Result<FeatureTable, String> {
    match environment {
        Ok(_) => features,
        Err(error) => Err(error.clone()),
    }
}

fn analyzed_count(analysis: &[TargetAnalysis]) -> usize {
    analysis
        .iter()
        .flat_map(|target| &target.outcomes)
        .filter(|(_, outcome)| matches!(outcome, TargetOutcome::Analyzed(_)))
        .count()
}

/// The style check, when the policy enables it. It runs even when the analysis environment
/// failed. Its diagnostics are kept apart from its errors and notes.
fn style_check(
    context: &Context,
    features: &Result<FeatureTable, String>,
    files: &[PathBuf],
    modules: &BTreeMap<PathBuf, String>,
) -> (Findings, Vec<StyleDiagnostic>) {
    let Some(style) = context.policy.style() else {
        return (Findings::default(), Vec::new());
    };
    let report = xtask_style::check(&StyleInput {
        root: &context.workspace.root,
        workspace: &context.workspace,
        analysis: &context.settings,
        features,
        style,
        files,
        modules,
    });
    let findings = Findings {
        errors: report.errors,
        notes: report.notes,
    };
    (findings, report.diagnostics)
}

/// The diagram file's messages: a note with its path when the file was written, or its error.
fn archive_findings(
    written: Result<Option<(PathBuf, usize)>, String>,
) -> (Option<PathBuf>, Findings) {
    match written {
        Ok(None) => (None, Findings::default()),
        Ok(Some((path, count))) => {
            let notes = vec![format!("wrote {} ({count} diagrams)", path.display())];
            (
                Some(path),
                Findings {
                    errors: Vec::new(),
                    notes,
                },
            )
        }
        Err(error) => (
            None,
            Findings {
                errors: vec![error],
                notes: Vec::new(),
            },
        ),
    }
}
