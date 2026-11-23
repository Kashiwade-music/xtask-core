//! Role: viewer の画面の組立て（root、view、読込みの開始と監視、toolbar、通知）。
use std::path::PathBuf;

use kivact::platform::{FileDialog, pick_file};
use kivact::prelude::{
    Element, Handled, IntoView, Key, KeyEvent, ListItem, binding, button, column, pct, row, select,
    spawn, style, text,
};
use kivact::view::{FileEvent, FilePhase};
use xtask_archive::{Content, DiagramKind, Mode};

use crate::canvas;
use crate::document;
use crate::export;
use crate::navigate;
use crate::sidebar;
use crate::state::{AppState, Document, Reason};

pub fn root(initial: Option<PathBuf>) -> impl IntoView {
    view(AppState::new(), initial)
}

pub fn view(state: AppState, initial: Option<PathBuf>) -> impl IntoView {
    if let Some(path) = initial {
        document::open(state, path, Reason::Open);
    }
    document::watch(state);
    let side = column(sidebar::sidebar(state)).style(style().width(300));
    let main = column((
        toolbar(state),
        column(canvas::diagram_view(state)).style(style().flex_grow(1).min_height(0)),
        notices(state),
    ))
    .style(style().flex_grow(1).min_width(0));
    row((side, main))
        .style(style().width(pct(100)).height(pct(100)))
        .on_file(move |event: FileEvent| {
            if event.phase == FilePhase::Drop
                && let Some(path) = event.files.into_iter().next()
            {
                document::open(state, path, Reason::Open);
            }
        })
        .on_key(move |event: KeyEvent| on_key(state, event.key()))
}

fn toolbar(state: AppState) -> impl IntoView {
    row((
        text(move || heading(state)),
        button("Open…").on_press(move || open_dialog(state)),
        button("Reload").on_press(move || reload(state)),
        button("Back").on_press(move || navigate::go_back(state)),
        button("Fit").on_press(move || navigate::fit(state)),
        button("Export SVG").on_press(move || export::export_svg(state)),
        select(
            move || package_items(state),
            binding(
                move || selected(state),
                move |index: Option<usize>| {
                    if let Some(index) = index {
                        navigate::go(state, index);
                    }
                },
            ),
        )
        .label("Diagram"),
    ))
}

fn notices(state: AppState) -> impl IntoView {
    column((
        text(move || archive_notices(state)),
        text(move || diagram_status(state)),
        text(move || state.note.get().unwrap_or_default()),
    ))
}

/// 今の図の題。文書が Loaded でなければ、その状態の文。
fn heading(state: AppState) -> String {
    match state.document.get() {
        Document::Empty => "No file open".into(),
        Document::Failed { message, .. } => message,
        Document::Loaded { .. } => state
            .current()
            .and_then(|(loaded, index)| loaded.archive.diagrams.get(index).map(|d| d.title.clone()))
            .unwrap_or_default(),
    }
}

fn archive_notices(state: AppState) -> String {
    state
        .current()
        .map(|(loaded, _)| loaded.archive.notices.join("\n"))
        .unwrap_or_default()
}

/// 今の図が Failed・Skipped なら、その理由。
fn diagram_status(state: AppState) -> String {
    let Some((loaded, index)) = state.current() else {
        return String::new();
    };
    match loaded
        .archive
        .diagrams
        .get(index)
        .map(|diagram| &diagram.content)
    {
        Some(Content::Failed(message) | Content::Skipped(message)) => message.clone(),
        Some(Content::Drawn(_)) | None => String::new(),
    }
}

/// 今の図と同じ package の module 図の項目。今の図が crate 図なら空。
fn package_items(state: AppState) -> Vec<ListItem<usize>> {
    let Some((loaded, index)) = state.current() else {
        return Vec::new();
    };
    let package = match loaded
        .archive
        .diagrams
        .get(index)
        .map(|diagram| &diagram.kind)
    {
        Some(DiagramKind::Modules { package, .. }) => package.clone(),
        _ => return Vec::new(),
    };
    loaded
        .archive
        .diagrams
        .iter()
        .enumerate()
        .filter_map(|(index, diagram)| match &diagram.kind {
            DiagramKind::Modules {
                package: other,
                target,
                kind,
                mode,
            } if *other == package => {
                let mode = match mode {
                    Mode::Normal => "normal",
                    Mode::Test => "test",
                };
                Some(ListItem::new(index, format!("{target} ({kind}, {mode})")))
            }
            DiagramKind::Modules { .. } | DiagramKind::Crates => None,
        })
        .collect()
}

/// select に示す今の図の index。module 図のときだけ選ばれている。
fn selected(state: AppState) -> Option<usize> {
    let (loaded, index) = state.current()?;
    matches!(
        loaded.archive.diagrams.get(index)?.kind,
        DiagramKind::Modules { .. }
    )
    .then_some(index)
}

fn open_dialog(state: AppState) {
    spawn(async move {
        let dialog = FileDialog {
            title: "Open".into(),
            ..Default::default()
        };
        match pick_file(dialog).await {
            Ok(Some(path)) => document::open(state, path, Reason::Open),
            Ok(None) => {}
            Err(error) => state.note.set(Some(format!("Open: {error}"))),
        }
    });
}

/// 今のファイルを読み直す（F5 と Reload）。
fn reload(state: AppState) {
    let path = match state.document.get_untracked() {
        Document::Empty => return,
        Document::Loaded { loaded, .. } => loaded.path.clone(),
        Document::Failed { path, .. } => path,
    };
    document::open(state, path, Reason::Reload);
}

fn on_key(state: AppState, key: Key) -> Handled {
    match key {
        Key::Escape => state.nav.update(|nav| *nav = navigate::unpin(nav)),
        Key::F(5) => reload(state),
        Key::Char('0') => navigate::fit(state),
        Key::Backspace => navigate::go_back(state),
        _ => return Handled::No,
    }
    Handled::Yes
}
