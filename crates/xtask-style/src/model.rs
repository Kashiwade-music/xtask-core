//! Role: スタイル検査の診断と報告の型を表現する。
use std::fmt;
use std::path::PathBuf;

use xtask_policy::StyleRule;

/// A style violation with a stable policy rule ID and a one-based source location.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct StyleDiagnostic {
    pub rule: StyleRule,
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

/// The outcome of a style check. `diagnostics` holds only violations that no
/// exception suppresses; `errors` holds failures to run a check or read a file.
#[derive(Debug, Default)]
pub struct StyleReport {
    pub diagnostics: Vec<StyleDiagnostic>,
    pub errors: Vec<String>,
    pub notes: Vec<String>,
}

impl StyleReport {
    /// Append another report's entries after this report's entries, keeping their order.
    pub(crate) fn append(&mut self, other: Self) {
        self.diagnostics.extend(other.diagnostics);
        self.errors.extend(other.errors);
        self.notes.extend(other.notes);
    }
}
