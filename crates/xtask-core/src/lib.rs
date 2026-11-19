//! Role: リポジトリ共通ルールの設定・検査・図の出力APIを公開する。
//! Shared rules for repository-local xtask executables.
//!
//! Call [`main`] with an explicit repository root, or use [`run`] to obtain a
//! structured report without printing. Checks write the architecture diagram file
//! only when [`Config::output`] is not [`Output::None`].

mod analysis;
mod archive;
mod cli;
mod config;
mod context;
mod model;
mod parallel;
mod run;
mod sources;
mod visibility;

pub use cli::{Command, main, parse_args};
pub use config::{Config, Output};
pub use model::{Report, TargetAnalysis, TargetOutcome};
pub use run::{analyze, run};
pub use xtask_archive::Mode;
pub use xtask_modules::ModuleGraph;
pub use xtask_policy::{AnalysisSettings, DiagramTargets, Features, Policy};
pub use xtask_style::StyleDiagnostic;
