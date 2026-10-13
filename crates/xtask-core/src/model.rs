//! Role: 検査レポートと、target・cfg別のモジュール解析結果を表現する。
use std::path::PathBuf;

use xtask_modules::ModuleGraph;
use xtask_workspace::Workspace;

#[derive(Debug, Default)]
pub struct Report {
    pub errors: Vec<String>,
    pub notes: Vec<String>,
    pub artifacts: Vec<PathBuf>,
}

impl Report {
    pub fn is_success(&self) -> bool {
        self.errors.is_empty()
    }
}

pub struct TargetGraph {
    pub package: String,
    pub target: String,
    pub kind: String,
    pub test: bool,
    pub graph: ModuleGraph,
}

/// A target that exists but was not analyzed because its `required-features`
/// are inactive. Its existing diagrams are kept rather than reported as stale.
#[derive(Clone, Debug)]
pub struct SkippedTarget {
    pub package: String,
    pub target: String,
    pub kind: String,
}

pub struct Analysis {
    pub workspace: Workspace,
    pub targets: Vec<TargetGraph>,
    pub skipped: Vec<SkippedTarget>,
    pub report: Report,
}
