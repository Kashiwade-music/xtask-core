//! Role: Rustソースを探索し、ファイルごとのコード行数の制限を検査する。
//! Rust source discovery and physical code-line limits.
//!
//! Comments and literal contents do not count. Literal opening and closing
//! delimiters do count, so each end of a multiline literal counts independently.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

mod lines;

pub use lines::code_line_count;

/// A source file whose physical code-line count exceeds the policy limit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LineViolation {
    pub path: PathBuf,
    pub count: usize,
    pub limit: usize,
}

/// Discover Rust files under `src`, `tests`, `benches`, `examples`, and `build.rs`.
///
/// Missing conventional source roots are allowed. Every other filesystem error
/// is returned. Canonical directory identities prevent symlink recursion.
/// Custom Cargo target paths can be appended by the caller before `check_files`.
pub fn rust_files(package_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let metadata =
        fs::metadata(package_dir).map_err(|error| format!("{}: {error}", package_dir.display()))?;
    if !metadata.is_dir() {
        return Err(format!(
            "{}: expected package directory",
            package_dir.display()
        ));
    }
    let mut files = Vec::new();
    let mut visited = BTreeSet::new();
    for root in ["src", "tests", "benches", "examples", "build.rs"] {
        let path = package_dir.join(root);
        match fs::symlink_metadata(&path) {
            Ok(_) => visit(&path, &mut visited, &mut files)?,
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(format!("{}: {error}", path.display())),
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}

fn visit(
    path: &Path,
    visited: &mut BTreeSet<PathBuf>,
    files: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let metadata = fs::metadata(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if metadata.is_dir() {
        let identity =
            fs::canonicalize(path).map_err(|error| format!("{}: {error}", path.display()))?;
        if !visited.insert(identity) {
            return Ok(());
        }
        let entries = fs::read_dir(path).map_err(|error| format!("{}: {error}", path.display()))?;
        let mut children = Vec::new();
        for entry in entries {
            children.push(
                entry
                    .map_err(|error| format!("{}: {error}", path.display()))?
                    .path(),
            );
        }
        children.sort();
        for child in children {
            visit(&child, visited, files)?;
        }
    } else if metadata.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
        files.push(path.to_owned());
    }
    Ok(())
}

/// Check explicitly supplied source files, returning violations in path order.
///
/// Unreadable files, invalid UTF-8, or Rust tokenization failures are errors;
/// they are never silently excluded from the check.
pub fn check_files(files: &[PathBuf], limit: usize) -> Result<Vec<LineViolation>, String> {
    let mut identities = BTreeMap::new();
    for path in files.iter().collect::<BTreeSet<_>>() {
        let identity =
            fs::canonicalize(path).map_err(|error| format!("{}: {error}", path.display()))?;
        identities.entry(identity).or_insert(path);
    }
    let mut violations = Vec::new();
    for path in identities.into_values() {
        let source =
            fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
        let count = lines::try_code_line_count(&source)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        if count > limit {
            violations.push(LineViolation {
                path: path.clone(),
                count,
                limit,
            });
        }
    }
    Ok(violations)
}
