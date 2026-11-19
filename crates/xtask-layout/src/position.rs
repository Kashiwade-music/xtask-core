//! Role: 層ごとの縦の座標を network simplex で決める（付属書 L の 7 章）。
use crate::level::{Chains, Order};
use crate::model::LayoutError;

pub(crate) fn vertical(chains: &Chains, order: &Order) -> Result<Vec<i64>, LayoutError> {
    todo!("L4")
}
