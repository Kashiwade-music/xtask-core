//! Role: 検査と図出力のパス、およびpolicyを上書きする設定を定義する。
use std::path::{Path, PathBuf};
use xtask_workspace::AnalysisSettings;

/// Repository paths, policy overrides and write control for the shared checks.
#[derive(Clone, Debug)]
pub struct Config {
    pub root: PathBuf,
    pub policy: PathBuf,
    /// Override the policy's SVG destination. Relative paths use `root`.
    pub output: Option<PathBuf>,
    /// Validate rules and diagram structure without writing files or invoking Node.
    pub no_write: bool,
    /// Compatibility flag: writing checks now prune stale module SVGs by default.
    /// Incompatible with `no_write`. Diagrams for existing selected targets are
    /// preserved when analysis fails, and inactive required-feature targets are kept.
    pub prune: bool,
    /// Override the policy's role-description limit in Unicode characters.
    /// `None` uses the policy value, or 80 when no policy is available.
    pub max_role_chars: Option<usize>,
    /// Replace the policy's entire analysis section when set.
    pub analysis: Option<AnalysisSettings>,
}

impl Config {
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();
        Self {
            policy: root.join("dev/xtask/policy/workspace.toml"),
            output: None,
            no_write: false,
            prune: false,
            root,
            max_role_chars: None,
            analysis: None,
        }
    }

    pub(crate) fn output_dir(&self) -> PathBuf {
        self.root
            .join(self.output.as_deref().unwrap_or(Path::new("architecture")))
    }
}
