//! Role: 図のファイルへ書き出す対象を選び、配置して符号化し、原子的に保存する。
use std::io::Write;
use std::path::{Path, PathBuf};

use tempfile::NamedTempFile;
use xtask_archive::Archive;
use xtask_diagram::{Input, Outcome, TargetInput};
use xtask_policy::DiagramTargets;
use xtask_workspace::TargetKind;

use crate::config::Output;
use crate::context::Context;
use crate::model::{TargetAnalysis, TargetOutcome};
use crate::parallel;

/// Write the diagram file when the output asks for one. Returns the path and the number of
/// diagrams, or `None` when no file is wanted. `notices` are the analysis-stage messages.
pub(crate) fn write_archive(
    context: &Context,
    analysis: &[TargetAnalysis],
    notices: Vec<String>,
) -> Result<Option<(PathBuf, usize)>, String> {
    let Some(path) = output_path(context) else {
        return Ok(None);
    };
    let archive = build(context, analysis, notices);
    let bytes = xtask_archive::encode(&archive)
        .map_err(|error| format!("cannot encode {}: {error}", path.display()))?;
    persist(&path, &bytes).map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    Ok(Some((path, archive.diagrams.len())))
}

/// The file to write: the policy's or the API's path, relative to the root unless absolute.
fn output_path(context: &Context) -> Option<PathBuf> {
    let relative = match &context.output {
        Output::None => return None,
        Output::Policy => context.policy.diagrams().output().to_path_buf(),
        Output::Path(path) => path.clone(),
    };
    Some(context.workspace.root.join(relative))
}

/// Whether a target of these kinds gets module diagrams under the policy's selection.
pub(crate) fn selected(targets: DiagramTargets, kinds: &[TargetKind]) -> bool {
    match targets {
        DiagramTargets::All => true,
        DiagramTargets::Lib => kinds.iter().any(TargetKind::is_library),
        DiagramTargets::LibBin => kinds
            .iter()
            .any(|kind| *kind == TargetKind::Bin || kind.is_library()),
    }
}

fn build(context: &Context, analysis: &[TargetAnalysis], notices: Vec<String>) -> Archive {
    let input = diagram_input(context, analysis);
    let plans = xtask_diagram::plan(&input);
    let results = parallel::map(&plans, xtask_diagram::draw);
    let diagrams = plans
        .iter()
        .zip(results)
        .map(|(plan, result)| {
            result.unwrap_or_else(|panic| {
                xtask_diagram::failed(plan, format!("layout panicked: {panic}"))
            })
        })
        .collect();
    let root_name = context
        .workspace
        .root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    Archive {
        root_name,
        diagrams,
        notices,
    }
}

fn diagram_input<'a>(context: &'a Context, analysis: &'a [TargetAnalysis]) -> Input<'a> {
    let targets = context.policy.diagrams().targets();
    let mut inputs = Vec::new();
    for target in analysis {
        let drawn = selected(targets, &target.kinds);
        for (mode, outcome) in &target.outcomes {
            inputs.push(TargetInput {
                package: &target.package,
                target: &target.target,
                kinds: &target.kinds,
                mode: *mode,
                outcome: diagram_outcome(outcome),
                drawn,
            });
        }
    }
    Input {
        workspace: &context.workspace,
        targets: inputs,
    }
}

fn diagram_outcome(outcome: &TargetOutcome) -> Outcome<'_> {
    match outcome {
        TargetOutcome::Analyzed(graph) => Outcome::Analyzed(graph),
        TargetOutcome::FeatureGated => Outcome::Skipped("required features are inactive".into()),
        TargetOutcome::UnsupportedEdition(message) | TargetOutcome::Failed(message) => {
            Outcome::Failed(message.clone())
        }
    }
}

/// Write to a temporary file in the same directory, then move it over `path`. The replacement
/// is never preceded by a delete.
fn persist(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(directory)?;
    let mut file = NamedTempFile::new_in(directory)?;
    file.write_all(bytes)?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    //! Role: diagram targets の選択が、target の種類ごとに付属書どおりの図を選ぶことを確かめる。
    use xtask_policy::DiagramTargets::{All, Lib, LibBin};
    use xtask_workspace::TargetKind::{
        Bench, Bin, Cdylib, CustomBuild, Example, Lib as LibKind, ProcMacro, Rlib, Test,
    };

    use super::selected;

    #[test]
    fn selection_by_diagram_targets() {
        for kinds in [vec![LibKind], vec![Cdylib, Rlib], vec![ProcMacro]] {
            for targets in [All, Lib, LibBin] {
                assert!(selected(targets, &kinds), "{targets:?} {kinds:?}");
            }
        }
        assert!(selected(All, &[Bin]));
        assert!(!selected(Lib, &[Bin]));
        assert!(selected(LibBin, &[Bin]));
        for kinds in [vec![Test], vec![Bench], vec![Example], vec![CustomBuild]] {
            assert!(selected(All, &kinds), "{kinds:?}");
            assert!(!selected(Lib, &kinds), "{kinds:?}");
            assert!(!selected(LibBin, &kinds), "{kinds:?}");
        }
    }
}
