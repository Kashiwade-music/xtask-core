//! Shared rules for repository-local xtask executables.
//!
//! Call [`main`] with an explicit repository root, or use [`run`] to obtain a
//! structured report without printing. Checks do not write graph artifacts.

mod cli;
mod config;
mod graphs;
mod model;
mod run;

pub use cli::{main, parse_args};
pub use config::{Command, Config};
pub use model::{Analysis, Report, TargetGraph};
pub use run::{analyze, run};
pub use xtask_modules::{AnalysisOptions, ModuleGraph};
pub use xtask_workspace::{Limits, Policy};
