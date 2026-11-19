//! Role: 配置の結果の指標（交差・折れ・面積・総延長）を計算する。

/// 配置の指標。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Metrics {
    /// 異なる辺の、水平な線分と垂直な線分の内部どうしの交差の数。
    pub crossings: usize,
    /// 折れの数の和（簡約した点の数 - 2）。
    pub bends: usize,
    pub width: f64,
    pub height: f64,
    /// 線分の長さの和。
    pub length: f64,
}

pub(crate) fn compute(width: f64, height: f64, routes: &[Vec<(f64, f64)>]) -> Metrics {
    todo!("L6")
}
