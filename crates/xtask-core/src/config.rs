//! Role: リポジトリのパス・feature・cfgと実行コマンドを定義する。
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The adopted rules use at most 1000 code lines per physical source file.
#[derive(Clone, Debug)]
pub struct Config {
    pub root: PathBuf,
    pub policy: PathBuf,
    pub output: PathBuf,
    /// Override the policy's role-description limit in Unicode characters.
    /// `None` uses the policy value, or 80 when no policy is available.
    pub max_role_chars: Option<usize>,
    /// Rust target triple. `None` uses the host compiler's configuration.
    pub target: Option<String>,
    /// Feature names or `package/feature` selections.
    pub features: Vec<String>,
    pub all_features: bool,
    pub default_features: bool,
    /// Additional `--cfg` values normally supplied by the consumer's build.
    pub cfg: Vec<String>,
    /// Analyze a separate cfg(test) configuration for library/binary targets.
    pub tests: bool,
    /// Explicit assumptions for macros that preserve the source module tree.
    /// Their generated code is outside this source-level analysis.
    pub trusted_attributes: BTreeSet<String>,
    pub trusted_derives: BTreeSet<String>,
}

impl Config {
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();
        Self {
            policy: root.join("dev/xtask/policy/workspace.toml"),
            output: root.join("architecture"),
            root,
            max_role_chars: None,
            target: None,
            features: Vec::new(),
            all_features: false,
            default_features: true,
            cfg: Vec::new(),
            tests: true,
            trusted_attributes: BTreeSet::new(),
            trusted_derives: BTreeSet::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Command {
    Check,
    Dependencies,
    Modules,
    Graphs,
}
