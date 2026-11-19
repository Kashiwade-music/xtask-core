//! Role: 層ごとの並びと port の順を決め、交差を数える（付属書 L の 5 章）。
use crate::level::{Chains, ItemId, Order};
use crate::model::LayoutError;

pub(crate) fn arrange(chains: &Chains) -> Result<Order, LayoutError> {
    todo!("L3")
}

pub(crate) fn crossings(chains: &Chains, layers: &[Vec<ItemId>]) -> usize {
    todo!("L3")
}
