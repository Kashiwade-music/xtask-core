//! Role: 検査の設定（policy）を読み、検証済みの値だけを読出し専用で公開する。
//! Repository policy: TOML loading and validation behind read-only accessors.
//!
//! Every setting is checked once, when the policy is read. Callers receive values
//! that cannot be changed, so later code never repeats those checks.

mod analysis;
mod checks;
mod policy;
mod style;

#[cfg(test)]
mod tests;

pub use analysis::{AnalysisSettings, Features};
pub use policy::{DiagramTargets, Diagrams, Limits, Policy};
pub use style::{StyleException, StyleRule, StyleSettings};
