//! Role: 描画の色・線の太さ・角の半径の定数（viewer と書出しで同じ値を使う）。
use crate::model::Rgb;

pub const STROKE: Rgb = Rgb(0x47, 0x55, 0x69);
pub const NODE_STROKE: Rgb = Rgb(0x33, 0x41, 0x55);
pub const REGION_STROKE: Rgb = Rgb(0x94, 0xa3, 0xb8);
pub const TEXT: Rgb = Rgb(0x0f, 0x17, 0x2a);
pub const ACCENT: Rgb = Rgb(0x25, 0x63, 0xeb);
pub const BACKGROUND: Rgb = Rgb(0xff, 0xff, 0xff);
pub const NODE_FILL: Rgb = Rgb(0xff, 0xff, 0xff);

pub const EDGE_WIDTH: f64 = 1.3;
pub const NODE_STROKE_WIDTH: f64 = 1.2;
pub const REGION_STROKE_WIDTH: f64 = 1.0;
pub const ACCENT_WIDTH: f64 = 2.6;
pub const NODE_RADIUS: f64 = 2.0;
pub const REGION_RADIUS: f64 = 4.0;
