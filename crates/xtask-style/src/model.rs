//! Role: スタイル検査の入力・診断・報告の型と、規則の有効判定を表現する。
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

use xtask_policy::{AnalysisSettings, StyleRule, StyleSettings};
use xtask_workspace::Workspace;

/// The prepared inputs of one style check. The caller normalizes `root` and the
/// paths in `files` and `modules`, and removes duplicate files.
pub struct StyleInput<'a> {
    /// The repository root, normalized with `xtask_source::normalize`.
    pub root: &'a Path,
    pub workspace: &'a Workspace,
    pub analysis: &'a AnalysisSettings,
    /// The features that Cargo selects for each member (`Workspace::features`). An
    /// `Err` is reported once by the Default probe; the Clippy run does not depend on it.
    pub features: &'a Result<BTreeMap<String, BTreeSet<String>>, String>,
    pub style: &'a StyleSettings,
    /// The Rust files to check, normalized and without duplicates.
    pub files: &'a [PathBuf],
    /// File to the module path of its root module (`crate::a::b`), used to name items.
    pub modules: &'a BTreeMap<PathBuf, String>,
}

/// Whether the style settings enable the rule.
pub(crate) fn enabled(style: &StyleSettings, rule: StyleRule) -> bool {
    style.rules().contains(&rule)
}

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
