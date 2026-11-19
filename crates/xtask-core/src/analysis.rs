//! Role: target ごとの解析を並列に実行し、target の結果と解析の診断を集める。
use std::collections::BTreeSet;

use xtask_archive::Mode;
use xtask_modules::{Configuration, Settings};
use xtask_workspace::{Edition, Package, Target, TargetKind};

use crate::context::{Context, Environment};
use crate::model::{Report, TargetAnalysis, TargetOutcome};
use crate::parallel;

/// Analyze every target under each of its configurations. An environment that cannot be read
/// fails every target, and the other stages still run.
pub(crate) fn analyze_targets(
    context: &Context,
    environment: &Result<Environment, String>,
) -> (Vec<TargetAnalysis>, Report) {
    match environment {
        Ok(environment) => analyzed(context, environment),
        Err(error) => failed(context, error),
    }
}

fn failed(context: &Context, error: &str) -> (Vec<TargetAnalysis>, Report) {
    let mut report = Report::default();
    report.errors.push(format!("module analysis: {error}"));
    let analysis = context
        .workspace
        .packages
        .iter()
        .flat_map(|package| {
            package.targets.iter().map(move |target| {
                let outcomes = modes(target)
                    .into_iter()
                    .map(|mode| {
                        let reason = format!("module analysis: {error}");
                        (mode, TargetOutcome::Failed(reason))
                    })
                    .collect();
                target_analysis(package, target, outcomes)
            })
        })
        .collect();
    (analysis, report)
}

fn analyzed(context: &Context, environment: &Environment) -> (Vec<TargetAnalysis>, Report) {
    let mut report = Report::default();
    let settings = package_settings(context, environment);
    let (mut analysis, jobs) = plan(context, environment, &settings, &mut report);
    let results = parallel::map(&jobs, |job| job.run());
    for (job, result) in jobs.iter().zip(results) {
        let outcomes = match result {
            Ok(outcomes) => outcomes,
            Err(panic) => job
                .modes
                .iter()
                .map(|mode| {
                    let reason = format!("analysis panicked: {panic}");
                    (*mode, TargetOutcome::Failed(reason))
                })
                .collect(),
        };
        for (mode, outcome) in &outcomes {
            record(&mut report, job.package, job.target, *mode, outcome);
        }
        analysis[job.slot].outcomes = outcomes;
    }
    trusted_note(context, &mut report);
    (analysis, report)
}

/// One target's analysis, to run in parallel with the others.
struct Job<'a> {
    /// The index of the target's entry in the analysis list.
    slot: usize,
    package: &'a Package,
    target: &'a Target,
    settings: &'a Settings,
    modes: Vec<Mode>,
    configurations: Vec<Configuration>,
}

impl Job<'_> {
    /// Analyze the target under each configuration. The results follow `modes`.
    fn run(&self) -> Vec<(Mode, TargetOutcome)> {
        let results =
            xtask_modules::analyze(&self.target.src_path, self.settings, &self.configurations);
        self.modes
            .iter()
            .copied()
            .zip(results)
            .map(|(mode, result)| {
                let outcome = match result {
                    Ok(graph) => TargetOutcome::Analyzed(graph),
                    Err(error) => TargetOutcome::Failed(error),
                };
                (mode, outcome)
            })
            .collect()
    }
}

/// The analysis list with every target in order, and the jobs for the targets that need an
/// analysis. A target that is skipped already has its outcomes.
fn plan<'a>(
    context: &'a Context,
    environment: &Environment,
    settings: &'a [Settings],
    report: &mut Report,
) -> (Vec<TargetAnalysis>, Vec<Job<'a>>) {
    let mut analysis = Vec::new();
    let mut jobs = Vec::new();
    for (index, package) in context.workspace.packages.iter().enumerate() {
        for target in &package.targets {
            let slot = analysis.len();
            analysis.push(target_analysis(package, target, Vec::new()));
            let setting = &settings[index];
            if let Some(outcomes) = skipped(package, target, &setting.features, report) {
                analysis[slot].outcomes = outcomes;
                continue;
            }
            let modes = modes(target);
            let configurations = modes
                .iter()
                .map(|mode| environment.configuration(&target.kinds, *mode))
                .collect();
            jobs.push(Job {
                slot,
                package,
                target,
                settings: setting,
                modes,
                configurations,
            });
        }
    }
    (analysis, jobs)
}

/// The outcomes of a target that needs no analysis: an unsupported edition, or inactive
/// `required-features`. `None` means the target is analyzed.
fn skipped(
    package: &Package,
    target: &Target,
    features: &BTreeSet<String>,
    report: &mut Report,
) -> Option<Vec<(Mode, TargetOutcome)>> {
    if let Some(message) = unsupported_edition(target.edition) {
        report
            .errors
            .push(format!("{}::{}: {message}", package.name, target.name));
        let outcomes = modes(target)
            .into_iter()
            .map(|mode| (mode, TargetOutcome::UnsupportedEdition(message.clone())))
            .collect();
        return Some(outcomes);
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
        return Some(vec![(Mode::Normal, TargetOutcome::FeatureGated)]);
    }
    None
}

fn unsupported_edition(edition: Edition) -> Option<String> {
    let text = match edition {
        Edition::E2018 | Edition::E2021 | Edition::E2024 => return None,
        Edition::E2015 => "2015",
        Edition::Other => "an unknown edition",
    };
    Some(format!(
        "module name resolution supports Rust editions 2018/2021/2024; found {text}"
    ))
}

/// The configurations of a target: the normal one, and the `cfg(test)` one for unit-test
/// targets. A standalone test or bench target is analyzed only under `cfg(test)`.
fn modes(target: &Target) -> Vec<Mode> {
    let standalone = target
        .kinds
        .iter()
        .any(|kind| matches!(kind, TargetKind::Test | TargetKind::Bench));
    let unit_test = target.test && target.kinds.iter().any(is_unit_test_kind);
    if standalone {
        vec![Mode::Test]
    } else if unit_test {
        vec![Mode::Normal, Mode::Test]
    } else {
        vec![Mode::Normal]
    }
}

fn is_unit_test_kind(kind: &TargetKind) -> bool {
    matches!(
        kind,
        TargetKind::Lib
            | TargetKind::Bin
            | TargetKind::Example
            | TargetKind::ProcMacro
            | TargetKind::Rlib
            | TargetKind::Staticlib
            | TargetKind::Cdylib
            | TargetKind::Dylib
    )
}

/// The analysis settings of each package, in the order of the workspace's packages.
fn package_settings(context: &Context, environment: &Environment) -> Vec<Settings> {
    let analysis = &context.settings;
    context
        .workspace
        .packages
        .iter()
        .map(|package| Settings {
            features: environment
                .features
                .get(&package.name)
                .cloned()
                .unwrap_or_default(),
            trusted_attributes: analysis.trusted_attributes().clone(),
            trusted_derives: analysis.trusted_derives().clone(),
            trusted_macros: analysis.trusted_macros().clone(),
            trusted_globs: analysis.trusted_globs().clone(),
            max_role_chars: context.policy.limits().max_role_chars(),
        })
        .collect()
}

fn target_analysis(
    package: &Package,
    target: &Target,
    outcomes: Vec<(Mode, TargetOutcome)>,
) -> TargetAnalysis {
    TargetAnalysis {
        package: package.name.clone(),
        target: target.name.clone(),
        kinds: target.kinds.clone(),
        outcomes,
    }
}

/// Report the diagnostics and cycles of one analyzed configuration, or its failure.
fn record(
    report: &mut Report,
    package: &Package,
    target: &Target,
    mode: Mode,
    outcome: &TargetOutcome,
) {
    let kinds: Vec<&str> = target.kinds.iter().map(TargetKind::as_str).collect();
    let test = if mode == Mode::Test {
        ", cfg(test)"
    } else {
        ""
    };
    let label = format!(
        "{}::{} ({}{test})",
        package.name,
        target.name,
        kinds.join("+")
    );
    match outcome {
        TargetOutcome::Analyzed(graph) => {
            report.errors.extend(
                graph
                    .diagnostics
                    .iter()
                    .map(|diagnostic| format!("{label}: {diagnostic}")),
            );
            report.errors.extend(graph.cycles.iter().map(|cycle| {
                format!(
                    "{label}: module reference cycle among: {}",
                    cycle.join(", ")
                )
            }));
        }
        TargetOutcome::Failed(message) => report.errors.push(format!("{label}: {message}")),
        TargetOutcome::FeatureGated | TargetOutcome::UnsupportedEdition(_) => {}
    }
}

fn trusted_note(context: &Context, report: &mut Report) {
    let settings = &context.settings;
    if settings.trusted_attributes().is_empty()
        && settings.trusted_derives().is_empty()
        && settings.trusted_macros().is_empty()
    {
        return;
    }
    report.notes.push(format!(
        "source-only macro assumptions: attributes {:?}, derives {:?}, macros {:?}; generated code is not analyzed",
        settings.trusted_attributes(),
        settings.trusted_derives(),
        settings.trusted_macros()
    ));
}
