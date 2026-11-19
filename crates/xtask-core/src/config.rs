//! Role: 検査と図出力のパス、およびpolicyを上書きする設定を定義する。
use std::path::{Path, PathBuf};
use xtask_policy::AnalysisSettings;

/// Repository paths, the diagram output choice and an optional analysis override.
#[derive(Clone, Debug)]
pub struct Config {
    pub root: PathBuf,
    pub policy: PathBuf,
    /// Where `check` writes the architecture diagrams.
    pub output: Output,
    /// Replace the policy's entire analysis section when set.
    pub analysis: Option<AnalysisSettings>,
}

/// Where the architecture diagrams are written, if anywhere.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Output {
    /// Write no diagrams (the default).
    None,
    /// Write to the path in the policy's `[diagrams] output`.
    Policy,
    /// Write to the given path. A relative path is resolved from `root`.
    Path(PathBuf),
}

impl Config {
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();
        Self {
            policy: root.join("dev/xtask/policy/workspace.toml"),
            output: Output::None,
            root,
            analysis: None,
        }
    }
}
