//! Role: リポジトリ共通ルールの設定・検査・図の出力APIを公開する。
//! Shared rules for repository-local xtask executables.
//!
//! Call [`main`] with an explicit repository root, or use [`run`] to obtain a
//! structured report without printing. Checks write the architecture diagram file
//! only when [`Config::output`] is not [`Output::None`].

mod cli;
mod config;
mod model;
mod parallel;
mod run;

pub use cli::{Command, main, parse_args};
pub use config::{Config, Output};
pub use model::{Analysis, Report, SkippedTarget, StyleDiagnostic, TargetGraph};
pub use run::{analyze, run};
pub use xtask_modules::ModuleGraph;
pub use xtask_policy::{
    AnalysisSettings, DiagramTargets, Diagrams, Limits, Policy, StyleException, StyleRule,
    StyleSettings,
};
