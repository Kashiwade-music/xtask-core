//! Role: target ごとの解析を並列に実行し、target の結果と解析の診断を集める。
use std::collections::BTreeSet;

use xtask_archive::Mode;
use xtask_modules::{Configuration, Settings};
use xtask_workspace::{Edition, Package, Target, TargetKind};

use crate::context::{Context, Environment, FeatureTable};
use crate::model::{Findings, TargetAnalysis, TargetOutcome};
use crate::parallel;

/// The outcome of each configuration of one target, in the order of its modes.
type Outcomes = Vec<(Mode, TargetOutcome)>;

/// Analyze every target under each of its configurations. When the features or the environment
/// cannot be read, every target fails and the other stages still run.
pub(crate) fn analyze_targets(
    context: &Context,
    features: &Result<FeatureTable, String>,
    environment: &Result<Environment, String>,
) -> (Vec<TargetAnalysis>, Findings) {
    match (features, environment) {
        (Ok(features), Ok(environment)) => analyzed(context, features, environment),
        (Err(error), _) | (_, Err(error)) => failed(context, error),
    }
}

fn failed(context: &Context, error: &str) -> (Vec<TargetAnalysis>, Findings) {
    let reason = analysis_failure(context, error);
    let analysis = context
        .workspace
        .packages
        .iter()
        .flat_map(|package| {
            package.targets.iter().map(|target| {
                let outcomes = modes(target)
                    .into_iter()
                    .map(|mode| (mode, TargetOutcome::Failed(reason.clone())))
                    .collect();
                target_analysis(package, target, outcomes)
            })
        })
        .collect();
    let findings = Findings {
        errors: vec![reason],
        notes: Vec::new(),
    };
    (analysis, findings)
}

/// The diagnostic for a failed environment. It names the configured analysis target: cargo's
/// last stderr line often omits it, so the target is the only place the reader can see it.
fn analysis_failure(context: &Context, error: &str) -> String {
    match context.settings.target() {
        Some(target) => format!("module analysis (target {target}): {error}"),
        None => format!("module analysis: {error}"),
    }
}

fn analyzed(
    context: &Context,
    features: &FeatureTable,
    environment: &Environment,
) -> (Vec<TargetAnalysis>, Findings) {
    let settings = package_settings(context, features);
    let (planned, mut findings) = plan(context, environment, &settings);
    let results = run_jobs(&planned);
    let (analysis, job_findings) = assemble(planned, results);
    findings.append(job_findings);
    findings.append(trusted_note(context));
    (analysis, findings)
}

/// One target's analysis, to run in parallel with the others.
struct Job<'a> {
    package: &'a Package,
    target: &'a Target,
    settings: &'a Settings,
    modes: Vec<Mode>,
    configurations: Vec<Configuration>,
}

impl Job<'_> {
    /// Analyze the target under each configuration. The results follow `modes`.
    fn run(&self) -> Outcomes {
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

    /// The outcomes of a finished job. A panic of its thread fails every configuration.
    fn outcomes(&self, result: Result<Outcomes, String>) -> Outcomes {
        result.unwrap_or_else(|panic| {
            self.modes
                .iter()
                .map(|mode| {
                    let reason = format!("analysis panicked: {panic}");
                    (*mode, TargetOutcome::Failed(reason))
                })
                .collect()
        })
    }
}

/// A target in the analysis list: finished already, or waiting for its analysis job.
enum Planned<'a> {
    /// A target that needs no analysis, with its outcomes.
    Done(TargetAnalysis),
    Job(Job<'a>),
}

/// The analysis list in target order, with the targets that need an analysis as jobs, and the
/// diagnostics of the targets that were skipped.
fn plan<'a>(
    context: &'a Context,
    environment: &Environment,
    settings: &'a [Settings],
) -> (Vec<Planned<'a>>, Findings) {
    let mut planned = Vec::new();
    let mut findings = Findings::default();
    for (package, setting) in context.workspace.packages.iter().zip(settings) {
        for target in &package.targets {
            match skipped(package, target, &setting.features) {
                Some((outcomes, skip)) => {
                    findings.append(skip);
                    planned.push(Planned::Done(target_analysis(package, target, outcomes)));
                }
                None => planned.push(Planned::Job(job(environment, package, target, setting))),
            }
        }
    }
    (planned, findings)
}

fn job<'a>(
    environment: &Environment,
    package: &'a Package,
    target: &'a Target,
    settings: &'a Settings,
) -> Job<'a> {
    let modes = modes(target);
    let configurations = modes
        .iter()
        .map(|mode| environment.configuration(&target.kinds, *mode))
        .collect();
    Job {
        package,
        target,
        settings,
        modes,
        configurations,
    }
}

/// Run the analysis of every job in parallel. The results follow the order of the jobs.
fn run_jobs(planned: &[Planned]) -> Vec<Result<Outcomes, String>> {
    let jobs: Vec<&Job> = planned
        .iter()
        .filter_map(|entry| match entry {
            Planned::Done(_) => None,
            Planned::Job(job) => Some(job),
        })
        .collect();
    parallel::map(&jobs, |job| job.run())
}

/// The analysis list with every outcome in target order, and the diagnostics of the jobs.
fn assemble(
    planned: Vec<Planned>,
    results: Vec<Result<Outcomes, String>>,
) -> (Vec<TargetAnalysis>, Findings) {
    let mut results = results.into_iter();
    let mut analysis = Vec::with_capacity(planned.len());
    let mut findings = Findings::default();
    for entry in planned {
        match entry {
            Planned::Done(target) => analysis.push(target),
            Planned::Job(job) => {
                let result = results.next().expect("one result for each job");
                let outcomes = job.outcomes(result);
                findings.append(record(job.package, job.target, &outcomes));
                analysis.push(target_analysis(job.package, job.target, outcomes));
            }
        }
    }
    (analysis, findings)
}

/// The outcomes of a target that needs no analysis, with its diagnostics: an unsupported
/// edition, or inactive `required-features`. `None` means the target is analyzed.
fn skipped(
    package: &Package,
    target: &Target,
    features: &BTreeSet<String>,
) -> Option<(Outcomes, Findings)> {
    if let Some(message) = unsupported_edition(target.edition) {
        let findings = Findings {
            errors: vec![format!("{}::{}: {message}", package.name, target.name)],
            notes: Vec::new(),
        };
        let outcomes = modes(target)
            .into_iter()
            .map(|mode| (mode, TargetOutcome::UnsupportedEdition(message.clone())))
            .collect();
        return Some((outcomes, findings));
    }
    if !target
        .required_features
        .iter()
        .all(|feature| features.contains(feature))
    {
        let findings = Findings {
            errors: Vec::new(),
            notes: vec![format!(
                "skipped {}::{}: required features are inactive",
                package.name, target.name
            )],
        };
        return Some((vec![(Mode::Normal, TargetOutcome::FeatureGated)], findings));
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
fn package_settings(context: &Context, features: &FeatureTable) -> Vec<Settings> {
    let analysis = &context.settings;
    context
        .workspace
        .packages
        .iter()
        .map(|package| Settings {
            features: features.get(&package.name).cloned().unwrap_or_default(),
            trusted_attributes: analysis.trusted_attributes().clone(),
            trusted_derives: analysis.trusted_derives().clone(),
            trusted_macros: analysis.trusted_macros().clone(),
            trusted_globs: analysis.trusted_globs().clone(),
            generated_type_attributes: analysis.generated_type_attributes().clone(),
            max_role_chars: context.policy.limits().max_role_chars(),
        })
        .collect()
}

fn target_analysis(package: &Package, target: &Target, outcomes: Outcomes) -> TargetAnalysis {
    TargetAnalysis {
        package: package.name.clone(),
        target: target.name.clone(),
        kinds: target.kinds.clone(),
        outcomes,
    }
}

/// The diagnostics and cycles of each analyzed configuration of a target, or its failure.
fn record(package: &Package, target: &Target, outcomes: &[(Mode, TargetOutcome)]) -> Findings {
    let mut findings = Findings::default();
    for (mode, outcome) in outcomes {
        let label = configuration_label(package, target, *mode);
        match outcome {
            TargetOutcome::Analyzed(graph) => {
                findings.errors.extend(
                    graph
                        .diagnostics
                        .iter()
                        .map(|diagnostic| format!("{label}: {diagnostic}")),
                );
                findings.errors.extend(graph.cycles.iter().map(|cycle| {
                    format!(
                        "{label}: module reference cycle among: {}",
                        cycle.join(", ")
                    )
                }));
            }
            TargetOutcome::Failed(message) => findings.errors.push(format!("{label}: {message}")),
            TargetOutcome::FeatureGated | TargetOutcome::UnsupportedEdition(_) => {}
        }
    }
    findings
}

/// The name of one configuration of a target, as the diagnostics show it.
fn configuration_label(package: &Package, target: &Target, mode: Mode) -> String {
    let kinds: Vec<&str> = target.kinds.iter().map(TargetKind::as_str).collect();
    let test = if mode == Mode::Test {
        ", cfg(test)"
    } else {
        ""
    };
    format!(
        "{}::{} ({}{test})",
        package.name,
        target.name,
        kinds.join("+")
    )
}

fn trusted_note(context: &Context) -> Findings {
    let settings = &context.settings;
    if settings.trusted_attributes().is_empty()
        && settings.trusted_derives().is_empty()
        && settings.trusted_macros().is_empty()
    {
        return Findings::default();
    }
    Findings {
        errors: Vec::new(),
        notes: vec![format!(
            "source-only macro assumptions: attributes {:?}, derives {:?}, macros {:?}; generated code is not analyzed",
            settings.trusted_attributes(),
            settings.trusted_derives(),
            settings.trusted_macros()
        )],
    }
}
