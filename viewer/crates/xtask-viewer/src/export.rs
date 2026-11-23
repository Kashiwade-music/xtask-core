//! Role: 表示中の図を SVG に書き出す（保存の dialog と、UI の外での描画と書込み）。
use std::path::Path;

use kivact::draw::Offscreen;
use kivact::platform::{FileDialog, save_file};
use kivact::prelude::spawn;
use kivact::task::blocking;
use xtask_render::Picture;

use crate::paint;
use crate::state::AppState;

/// 表示中の図を SVG に書き出す。書出しの対象は、押した時の図。
pub fn export_svg(state: AppState) {
    let Some((loaded, index)) = state.current() else {
        return;
    };
    let Some(picture) = loaded.pictures.get(index).cloned().flatten() else {
        return;
    };
    let title = loaded
        .archive
        .diagrams
        .get(index)
        .map_or("diagram", |diagram| diagram.title.as_str());
    let dialog = FileDialog {
        title: "Export SVG".into(),
        file_name: Some(format!("{}.svg", file_stem(title))),
        filters: vec![("SVG".into(), vec!["svg".into()])],
        ..Default::default()
    };
    spawn(async move {
        match save_file(dialog).await {
            Ok(Some(path)) => {
                if let Err(error) = blocking(move || write_svg(&picture, &path)).await {
                    state.note.set(Some(error));
                }
            }
            Ok(None) => {}
            Err(error) => state.note.set(Some(format!("Export SVG: {error}"))),
        }
    });
}

/// 題を、ファイル名に使える形にする（英数字と '-' 以外は '_'）。
fn file_stem(title: &str) -> String {
    title
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// SVG の生成と書込み。Offscreen は `!Send` なので、この関数の中で作る（UI の外で呼ぶ）。
fn write_svg(picture: &Picture, path: &Path) -> Result<(), String> {
    let svg = Offscreen::new()
        .render_svg(picture.width, picture.height, |draw, _| {
            paint::picture(draw, picture, paint::Mode::Export)
        })
        .map_err(|error| error.to_string())?;
    std::fs::write(path, svg).map_err(|error| format!("{}: {error}", path.display()))
}
