//! Role: 基準の全件で、配置・不変条件・決定性を確かめる。
mod common;

use common::baseline;
use xtask_layout::{layout, verify};

#[test]
fn every_baseline_input_lays_out() {
    let figures = baseline::load_all();
    assert_eq!(figures.len(), 146, "the baseline holds 146 figures");
    let mut failures = Vec::new();
    for figure in &figures {
        let label = format!("{}/{}", figure.group(), figure.name());
        match layout(&figure.graph) {
            Ok(result) => {
                if let Err(message) = verify(&figure.graph, &result) {
                    failures.push(format!("{label}: {message}"));
                }
            }
            Err(error) => failures.push(format!("{label}: {error}")),
        }
    }
    assert!(
        failures.is_empty(),
        "{} figures failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn layout_is_deterministic() {
    for figure in baseline::load_all() {
        assert_eq!(
            layout(&figure.graph),
            layout(&figure.graph),
            "{}/{} differs between two runs",
            figure.group(),
            figure.name()
        );
    }
}
