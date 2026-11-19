//! Role: workspaceのパッケージ、target、依存の辺の型を定義し、library種別の判定を持つ。
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

/// A Cargo workspace, restricted to the members named by `workspace_members`.
#[derive(Clone, Debug)]
pub struct Workspace {
    pub root: PathBuf,
    pub packages: Vec<Package>,
    /// Declared member-to-member dependencies, from the manifests.
    pub edges: Vec<Dependency>,
    /// Features that Cargo activates for each member under `--all-features`, by package name.
    pub all_features: BTreeMap<String, BTreeSet<String>>,
}

#[derive(Clone, Debug)]
pub struct Package {
    /// The Cargo package ID, as in the JSON `package_id` of compiler messages.
    pub id: String,
    pub name: String,
    pub manifest_path: PathBuf,
    pub targets: Vec<Target>,
}

#[derive(Clone, Debug)]
pub struct Target {
    pub name: String,
    pub kinds: Vec<TargetKind>,
    pub src_path: PathBuf,
    pub required_features: Vec<String>,
    /// Whether Cargo enables the target's test harness by default.
    pub test: bool,
    pub edition: Edition,
}

/// The kind of a Cargo target.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TargetKind {
    Lib,
    Rlib,
    Staticlib,
    Cdylib,
    Dylib,
    ProcMacro,
    Bin,
    Example,
    Test,
    Bench,
    CustomBuild,
    Other(String),
}

impl TargetKind {
    /// Whether the target is a library, which other packages may depend on.
    pub fn is_library(&self) -> bool {
        matches!(
            self,
            Self::Lib | Self::Rlib | Self::Staticlib | Self::Cdylib | Self::Dylib | Self::ProcMacro
        )
    }

    /// The name Cargo uses for this kind in its metadata.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Lib => "lib",
            Self::Rlib => "rlib",
            Self::Staticlib => "staticlib",
            Self::Cdylib => "cdylib",
            Self::Dylib => "dylib",
            Self::ProcMacro => "proc-macro",
            Self::Bin => "bin",
            Self::Example => "example",
            Self::Test => "test",
            Self::Bench => "bench",
            Self::CustomBuild => "custom-build",
            Self::Other(kind) => kind,
        }
    }

    pub(crate) fn from_cargo(kind: &str) -> Self {
        match kind {
            "lib" => Self::Lib,
            "rlib" => Self::Rlib,
            "staticlib" => Self::Staticlib,
            "cdylib" => Self::Cdylib,
            "dylib" => Self::Dylib,
            "proc-macro" => Self::ProcMacro,
            "bin" => Self::Bin,
            "example" => Self::Example,
            "test" => Self::Test,
            "bench" => Self::Bench,
            "custom-build" => Self::CustomBuild,
            other => Self::Other(other.into()),
        }
    }
}

/// The Rust edition of a target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edition {
    E2015,
    E2018,
    E2021,
    E2024,
    Other,
}

impl Edition {
    pub(crate) fn from_cargo(edition: &str) -> Self {
        match edition {
            "2015" => Self::E2015,
            "2018" => Self::E2018,
            "2021" => Self::E2021,
            "2024" => Self::E2024,
            _ => Self::Other,
        }
    }
}

/// A declared member-to-member dependency. Endpoints are unique package names.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Dependency {
    pub from: String,
    pub to: String,
    pub kind: DependencyKind,
    /// The `cfg(...)` or target triple of a target-specific dependency.
    pub target: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DependencyKind {
    Normal,
    Dev,
    Build,
}

impl DependencyKind {
    pub(crate) fn from_cargo(kind: &str) -> Result<Self, String> {
        match kind {
            "normal" => Ok(Self::Normal),
            "dev" => Ok(Self::Dev),
            "build" => Ok(Self::Build),
            other => Err(format!("invalid Cargo dependency kind `{other}`")),
        }
    }

    /// The name Cargo uses for this kind; a missing kind in metadata means `normal`.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Dev => "dev",
            Self::Build => "build",
        }
    }
}
