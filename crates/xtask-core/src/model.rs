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
