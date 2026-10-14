//! Role: リポジトリ共通ルールの設定・検査・図の出力APIを公開する。
//! Shared rules for repository-local xtask executables.
//!
//! Call [`main`] with an explicit repository root, or use [`run`] to obtain a
//! structured report without printing. Checks write architecture SVGs unless
//! [`Config::no_write`] requests validation only.

mod cli;
mod config;
mod graphs;
mod model;
mod parallel;
mod run;

pub use cli::{main, parse_args};
pub use config::Config;
pub use model::{Analysis, Report, SkippedTarget, TargetGraph};
pub use run::{analyze, run};
pub use xtask_modules::{AnalysisOptions, ModuleGraph};
pub use xtask_workspace::{AnalysisSettings, DiagramTargets, Diagrams, Limits, Policy};
