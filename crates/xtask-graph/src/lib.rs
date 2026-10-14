//! Role: グラフをELKで配置し、既存の成果物を保護しながらSVGを出力する。
//! Deterministic diagram requests and direct ELK-backed SVG export.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

mod adapter;
mod layout;
mod model;
pub use layout::{Cluster, Direction, EdgeStyle, Layout};
pub use model::{Edge, Graph, Node};

/// Render SVG with ELK and `XTASK_NODE`, or `node` on PATH.
///
/// A previous SVG is kept until ELK and the independent SVG renderer succeed.
pub fn write(graph: &Graph, svg_path: &Path) -> Result<(), String> {
    write_with_layout(graph, &Layout::default(), svg_path)
}

/// Export with an explicit Node executable, without changing process env.
pub fn write_with_node(graph: &Graph, svg_path: &Path, binary: &OsStr) -> Result<(), String> {
    write_with_node_and_layout(graph, &Layout::default(), svg_path, binary)
}

/// Export with explicit ranks, nested regions and dependency styles.
pub fn write_with_layout(graph: &Graph, layout: &Layout, svg_path: &Path) -> Result<(), String> {
    let binary = std::env::var_os("XTASK_NODE").unwrap_or_else(|| "node".into());
    write_with_node_and_layout(graph, layout, svg_path, &binary)
}

/// Export a layout with an injected Node executable.
pub fn write_with_node_and_layout(
    graph: &Graph,
    layout: &Layout,
    svg_path: &Path,
    binary: &OsStr,
) -> Result<(), String> {
    SvgWriter::with_node(binary).write(graph, layout, svg_path)
}

/// A sequence of SVG exports sharing one lazily initialized Node worker.
#[derive(Default)]
pub struct SvgWriter {
    session: xtask_layout::LayoutSession,
}

impl SvgWriter {
    pub fn with_node(binary: &OsStr) -> Self {
        Self {
            session: xtask_layout::LayoutSession::with_node(binary),
        }
    }

    /// Keep a previous SVG until layout and independent rendering succeed.
    pub fn write(&mut self, graph: &Graph, layout: &Layout, svg_path: &Path) -> Result<(), String> {
        let request = graph.layout_request(layout)?;
        let scene = self.session.layout(&request)?;
        let svg = xtask_svg::render(&scene)?;
        create_parent(svg_path)?;
        let temporary = TemporarySvg::new(svg_path)?;
        fs::write(&temporary.path, svg)
            .map_err(|error| format!("cannot write SVG {}: {error}", svg_path.display()))?;
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
