//! Role: 検査の対象の Rust source の一覧と、file ごとの root module の path を作り、行数を検査する。
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use xtask_modules::ModuleKind;
use xtask_source::normalize;

use crate::context::Context;
use crate::model::{TargetAnalysis, TargetOutcome};
use crate::parallel;

/// The Rust files that the checks read, normalized and without duplicates, and the module
/// path of each file's root module.
pub(crate) struct Sources {
    pub(crate) files: Vec<PathBuf>,
    pub(crate) modules: BTreeMap<PathBuf, String>,
    pub(crate) errors: Vec<String>,
}

/// Collect the files of every package directory, every Cargo target root (even one that is
/// feature-gated or unanalyzed), and every analyzed module's file.
pub(crate) fn collect(context: &Context, analysis: &[TargetAnalysis]) -> Sources {
    let mut errors = Vec::new();
    let candidates = candidates(context, analysis, &mut errors);
    let mut resolved: BTreeMap<PathBuf, Option<PathBuf>> = BTreeMap::new();
    for (raw, _) in &candidates {
        if resolved.contains_key(raw) {
            continue;
        }
        let path = match normalize(raw) {
            Ok(path) => Some(path),
            Err(error) => {
                errors.push(format!("source line check: {error}"));
                None
            }
        };
        resolved.insert(raw.clone(), path);
    }
    let files: BTreeSet<PathBuf> = resolved.values().flatten().cloned().collect();
    let mut modules = BTreeMap::new();
    for (raw, root) in candidates {
        if let (Some(id), Some(Some(path))) = (root, resolved.get(&raw)) {
            modules.entry(path.clone()).or_insert(id);
        }
    }
    Sources {
        files: files.into_iter().collect(),
        modules,
        errors,
    }
}

/// Every raw path in the order the checks name it. A file carries the module path of its root
/// module when an analyzed target has one; the first target in the order wins.
fn candidates(
    context: &Context,
    analysis: &[TargetAnalysis],
    errors: &mut Vec<String>,
) -> Vec<(PathBuf, Option<String>)> {
    let mut found = Vec::new();
    for package in &context.workspace.packages {
        match package.manifest_path.parent() {
            Some(directory) => match xtask_source::rust_files(directory) {
                Ok(files) => found.extend(files.into_iter().map(|file| (file, None))),
                Err(error) => errors.push(format!("source discovery: {error}")),
            },
            None => errors.push(format!(
                "source discovery: {}: manifest has no parent",
                package.manifest_path.display()
            )),
        }
        found.extend(
            package
                .targets
                .iter()
                .map(|target| (target.src_path.clone(), None)),
        );
    }
    for target in analysis {
        for (_, outcome) in &target.outcomes {
            if let TargetOutcome::Analyzed(graph) = outcome {
                found.extend(graph.modules.iter().map(|module| {
                    let root = (module.kind == ModuleKind::File).then(|| module.id.clone());
                    (module.file.clone(), root)
                }));
            }
        }
    }
    found
}

/// The violations of the code line limit, one file per check. A panic in one check is an error
/// for that file only.
pub(crate) fn line_errors(files: &[PathBuf], limit: usize) -> Vec<String> {
    let results = parallel::map(files, |file| {
        xtask_source::check_files(std::slice::from_ref(file), limit)
    });
    let mut errors = Vec::new();
    for (file, result) in files.iter().zip(results) {
        match result {
            Ok(Ok(violations)) => errors.extend(violations.into_iter().map(|violation| {
                format!(
                    "{}: {} code lines (limit {}); split by responsibility",
                    violation.path.display(),
                    violation.count,
                    violation.limit
                )
            })),
            Ok(Err(error)) => errors.push(format!("source line check: {error}")),
            Err(panic) => errors.push(format!("source line check: {}: {panic}", file.display())),
        }
    }
    errors
}
