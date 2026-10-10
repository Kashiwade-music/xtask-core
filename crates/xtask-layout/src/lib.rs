//! ELK owns node placement and orthogonal edge routes; SVG decoration is separate.

mod model;
mod prepare;
mod runtime;
mod validate;

pub use model::{
    Direction, EDGE_FONT_SIZE, EdgeRequest, EdgeStyle, FONT_FAMILY, LINE_HEIGHT, LayoutRequest,
    NODE_FONT_SIZE, NodeRequest, Point, REGION_FONT_SIZE, REGION_LINE_HEIGHT, RegionRequest,
    Request, Scene, SceneEdge, SceneLabel, SceneNode, SceneRegion, TITLE_FONT_SIZE,
};
pub use runtime::{layout, layout_with_node};
