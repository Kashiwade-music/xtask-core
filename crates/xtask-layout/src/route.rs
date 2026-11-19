//! Role: 通路の track を割り当て、辺の折れ線を組み立てる（付属書 L の 8 章）。
use crate::level::{Chains, LevelInput, LevelOutput, Order};
use crate::model::LayoutError;

pub(crate) fn finish(
    input: &LevelInput,
    chains: &Chains,
    order: &Order,
    y: &[i64],
) -> Result<LevelOutput, LayoutError> {
    todo!("L5")
}
