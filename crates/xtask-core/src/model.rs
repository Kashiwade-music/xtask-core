//! Role: 検査レポートと、target・構成ごとのモジュール解析結果を表現する。
use std::path::PathBuf;

use xtask_archive::Mode;
use xtask_modules::ModuleGraph;
use xtask_style::StyleDiagnostic;
use xtask_workspace::TargetKind;

/// The findings of one run. Failures and rule violations are collected here rather than
/// stopping the run, so independent checks all report.
#[derive(Debug, Default)]
pub struct Report {
    pub errors: Vec<String>,
    pub notes: Vec<String>,
    /// Style violations that no exception suppresses. Kept apart from `errors`.
    pub style: Vec<StyleDiagnostic>,
    /// The diagram file written by this run, when the output asked for one.
    pub archive: Option<PathBuf>,
}

impl Report {
    pub fn is_success(&self) -> bool {
        self.errors.is_empty() && self.style.is_empty()
    }
}

/// The messages of one stage: errors that fail the run, and notes that only inform.
#[derive(Debug, Default)]
pub(crate) struct Findings {
    pub(crate) errors: Vec<String>,
    pub(crate) notes: Vec<String>,
}

impl Findings {
    /// Every message of the stage, errors first, as the diagram file records them.
    pub(crate) fn messages(&self) -> Vec<String> {
        self.errors.iter().chain(&self.notes).cloned().collect()
    }

    pub(crate) fn append(&mut self, other: Findings) {
        self.errors.extend(other.errors);
        self.notes.extend(other.notes);
    }

    pub(crate) fn into_report(self) -> Report {
        Report {
            errors: self.errors,
            notes: self.notes,
            ..Report::default()
        }
    }
}

/// The analysis of one Cargo target, in each configuration it is analyzed under.
#[derive(Debug)]
pub struct TargetAnalysis {
    pub package: String,
    pub target: String,
    pub kinds: Vec<TargetKind>,
    pub outcomes: Vec<(Mode, TargetOutcome)>,
}

/// What became of one target in one configuration.
#[derive(Debug)]
pub enum TargetOutcome {
    Analyzed(ModuleGraph),
    /// Not analyzed, because its `required-features` are inactive.
    FeatureGated,
    /// Not analyzed, because its Rust edition is not supported by module name resolution.
    UnsupportedEdition(String),
    /// Analysis failed, with the reason.
    Failed(String),
}
