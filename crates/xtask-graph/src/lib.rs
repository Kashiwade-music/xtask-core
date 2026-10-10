//! Deterministic DOT generation and Graphviz SVG export.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

mod layout;
mod model;
mod wave;
pub use layout::{Cluster, Direction, EdgeStyle, Layout};
pub use model::{Edge, Graph, Node};

/// Write DOT and render SVG with `XTASK_GRAPHVIZ_DOT`, or `dot` on PATH.
///
/// DOT remains available for inspection when rendering fails. A previous SVG
/// is kept until Graphviz has successfully generated a nonempty replacement.
pub fn write(graph: &Graph, dot_path: &Path, svg_path: &Path) -> Result<(), String> {
    let binary = std::env::var_os("XTASK_GRAPHVIZ_DOT").unwrap_or_else(|| "dot".into());
    write_with_dot(graph, dot_path, svg_path, &binary)
}

/// Export with an explicit Graphviz executable, without changing process env.
pub fn write_with_dot(
    graph: &Graph,
    dot_path: &Path,
    svg_path: &Path,
    binary: &OsStr,
) -> Result<(), String> {
    write_with_dot_and_layout(graph, &Layout::default(), dot_path, svg_path, binary)
}

/// Export with explicit ranks, nested regions and dependency styles.
pub fn write_with_layout(
    graph: &Graph,
    layout: &Layout,
    dot_path: &Path,
    svg_path: &Path,
) -> Result<(), String> {
    let binary = std::env::var_os("XTASK_GRAPHVIZ_DOT").unwrap_or_else(|| "dot".into());
    write_with_dot_and_layout(graph, layout, dot_path, svg_path, &binary)
}

/// Export a layout with an injected Graphviz executable.
pub fn write_with_dot_and_layout(
    graph: &Graph,
    layout: &Layout,
    dot_path: &Path,
    svg_path: &Path,
    binary: &OsStr,
) -> Result<(), String> {
    if dot_path == svg_path {
        return Err("DOT and SVG output paths must be different".into());
    }
    let dot = graph.dot_with_layout(layout)?;
    create_parent(dot_path)?;
    create_parent(svg_path)?;
    fs::write(dot_path, dot)
        .map_err(|error| format!("cannot write DOT {}: {error}", dot_path.display()))?;
    let temporary = TemporarySvg::new(svg_path)?;
    let output = Command::new(binary)
        .arg("-Tsvg")
        .arg(dot_path)
        .arg("-o")
        .arg(&temporary.path)
        .output()
        .map_err(|error| {
            format!(
                "cannot run Graphviz {:?}: {error}; install Graphviz or set XTASK_GRAPHVIZ_DOT to its dot executable. DOT retained at {}",
                binary,
                dot_path.display()
            )
        })?;
    if !output.status.success() {
        return Err(format!(
            "Graphviz {:?} failed ({}): {}; DOT retained at {}",
            binary,
            output.status,
            String::from_utf8_lossy(&output.stderr).trim(),
            dot_path.display()
        ));
    }
    let metadata = fs::metadata(&temporary.path)
        .map_err(|error| format!("cannot read Graphviz SVG output: {error}"))?;
    if metadata.len() == 0 {
        return Err(format!(
            "Graphviz succeeded but produced an empty SVG; DOT retained at {}",
            dot_path.display()
        ));
    }
    let dev_ids = graph.dev_edge_ids(layout);
    if !dev_ids.is_empty() {
        let svg = fs::read_to_string(&temporary.path)
            .map_err(|error| format!("cannot read SVG for wave rendering: {error}"))?;
        let svg = wave::render(&svg, &dev_ids)?;
        fs::write(&temporary.path, svg)
            .map_err(|error| format!("cannot save SVG wave rendering: {error}"))?;
    }
    // Windows cannot rename over an existing file. Remove it only after the
    // new output has been produced and validated.
    #[cfg(windows)]
    if svg_path.exists() {
        fs::remove_file(svg_path)
            .map_err(|error| format!("cannot replace SVG {}: {error}", svg_path.display()))?;
    }
    fs::rename(&temporary.path, svg_path)
        .map_err(|error| format!("cannot save SVG {}: {error}", svg_path.display()))?;
    Ok(())
}

fn create_parent(path: &Path) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create directory {}: {error}", parent.display()))?;
    }
    Ok(())
}

struct TemporarySvg {
    path: PathBuf,
}

impl TemporarySvg {
    fn new(target: &Path) -> Result<Self, String> {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let parent = target.parent().unwrap_or_else(|| Path::new("."));
        loop {
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!(".xtask-graph-{}-{id}.svg.tmp", std::process::id()));
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(_) => return Ok(Self { path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(format!("cannot create temporary SVG: {error}")),
            }
        }
    }
}

impl Drop for TemporarySvg {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod layout_tests;
#[cfg(test)]
mod tests;
