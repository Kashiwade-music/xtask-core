//! Role: Cargo metadataからworkspaceの構造・依存の辺・featureを検証して公開する。
//! Cargo workspace structure: packages, targets, declared dependency edges, and features.
//!
//! Edges include every declared normal, development, build, and target-specific
//! dependency on a workspace member, including optional dependencies. Identity
//! comes from canonical manifest paths or from Cargo's resolved package IDs, never
//! from a dependency's alias.

mod metadata;
mod model;

#[cfg(test)]
mod tests;

pub use metadata::needs_selection_run;
pub use model::{Dependency, DependencyKind, Edition, Package, Target, TargetKind, Workspace};
