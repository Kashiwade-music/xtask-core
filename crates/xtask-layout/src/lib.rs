//! Role: 直交の階層配置の公開 API を提供する。
//! Native layered layout: regions bottom-up, network simplex, orthogonal channels.
mod acyclic;
mod chains;
mod check;
mod hierarchy;
mod layering;
mod level;
mod metrics;
mod model;
mod order;
mod position;
mod route;
mod simplex;

pub use metrics::Metrics;
pub use model::{
    Edge, Graph, Layering, Layout, LayoutError, Node, Point, Rect, Region, Route, Size,
};

/// 要求を検証して配置し、出力の不変条件を確かめて返す。
pub fn layout(graph: &Graph) -> Result<Layout, LayoutError> {
    check::input(graph)?;
    let layout = hierarchy::layout(graph)?;
    check::output(graph, &layout).map_err(LayoutError::Internal)?;
    Ok(layout)
}

/// 出力の不変条件を確かめる。
pub fn verify(graph: &Graph, layout: &Layout) -> Result<(), String> {
    check::output(graph, layout)
}

/// 折れ線の指標。ELK の結果にも使えるよう f64 で受け取る。
pub fn metrics(width: f64, height: f64, routes: &[Vec<(f64, f64)>]) -> Metrics {
    metrics::compute(width, height, routes)
}
