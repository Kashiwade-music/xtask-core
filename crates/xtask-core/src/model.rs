//! Role: 検査レポートと、target・cfg別のモジュール解析結果を表現する。
use std::fmt;
use std::path::PathBuf;

use xtask_modules::ModuleGraph;
use xtask_workspace::Workspace;

/// A style violation with a stable policy rule ID and a one-based source location.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct StyleDiagnostic {
    pub rule: xtask_workspace::StyleRule,
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
    pub message: String,
    /// Exact item identity when known; compiler spans are resolved from source.
    pub item: Option<String>,
}

impl fmt::Display for StyleDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}:{}:{}: [style/{}] {}",
            self.path.display(),
            self.line,
            self.column,
            self.rule,
            self.message
        )
    }
}

#[derive(Debug, Default)]
pub struct Report {
    pub errors: Vec<String>,
    pub notes: Vec<String>,
    pub artifacts: Vec<PathBuf>,
    /// Unsuppressed style violations, also rendered in `errors` for CLI consumers.
    pub style_diagnostics: Vec<crate::StyleDiagnostic>,
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
