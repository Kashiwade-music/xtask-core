//! Role: 解析結果から図の配置要求を作り、配置して図のファイルの型へ組み立てる。
//! `plan` lists the diagrams of one archive and their layout requests without laying them out;
//! `draw` lays out one planned diagram, so the caller may run `draw` on several plans in parallel.
mod crate_graph;
mod model;
mod module_graph;
mod scene;
#[cfg(test)]
mod tests;
mod text;

use xtask_archive::{Content, Diagram, DiagramKind, Mode};

use model::{Body, kinds_text, module_kind, open_indices};
pub use model::{Input, Outcome, Plan, TargetInput, primary_target};

/// 図を並べ、配置の要求を作る。結果の順は Archive.diagrams の順。配置はしない。
pub fn plan(input: &Input<'_>) -> Vec<Plan> {
    let mut drawn: Vec<(DiagramKind, &TargetInput<'_>)> = input
        .targets
        .iter()
        .filter(|target| target.drawn)
        .map(|target| (module_kind(target), target))
        .collect();
    drawn.sort_by(|left, right| left.0.cmp(&right.0));
    let opens = open_indices(&drawn);
    let mut plans = vec![crate_graph::plan(input, &opens)];
    plans.extend(
        drawn
            .iter()
            .map(|(kind, target)| module_plan(kind.clone(), target)),
    );
    plans
}

/// 一つの図を配置する。配置に失敗したら Content::Failed にする。
pub fn draw(plan: &Plan) -> Diagram {
    let content = match &plan.body {
        Body::Graph(drawing) => match scene::draw_graph(drawing) {
            Ok(scene) => Content::Drawn(scene),
            Err(message) => Content::Failed(message),
        },
        Body::Failed(message) => Content::Failed(message.clone()),
        Body::Skipped(reason) => Content::Skipped(reason.clone()),
    };
    Diagram {
        kind: plan.kind.clone(),
        title: plan.title.clone(),
        content,
    }
}

/// 配置の worker が panic したときの図（kind と title は plan のもの）。
pub fn failed(plan: &Plan, message: String) -> Diagram {
    Diagram {
        kind: plan.kind.clone(),
        title: plan.title.clone(),
        content: Content::Failed(message),
    }
}

fn module_plan(kind: DiagramKind, target: &TargetInput<'_>) -> Plan {
    let mode = match target.mode {
        Mode::Normal => "normal",
        Mode::Test => "test",
    };
    let title = format!(
        "{}::{} ({}, {mode}): module dependencies",
        target.package,
        target.target,
        kinds_text(target.kinds)
    );
    let body = match &target.outcome {
        Outcome::Analyzed(graph) => match module_graph::drawing(graph) {
            Ok(drawing) => Body::Graph(drawing),
            Err(message) => Body::Failed(message),
        },
        Outcome::Failed(message) => Body::Failed(message.clone()),
        Outcome::Skipped(reason) => Body::Skipped(reason.clone()),
    };
    Plan { kind, title, body }
}
