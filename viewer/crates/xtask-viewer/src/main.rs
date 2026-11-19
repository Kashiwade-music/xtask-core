//! Role: viewer の起動。コマンドラインの最初の引数の図のファイルを開く。
fn main() -> kivact::Result<()> {
    kivact::prelude::App::new("xtask viewer")
        .window_size(1280, 800)
        .run(|| xtask_viewer::app::root(std::env::args().nth(1).map(std::path::PathBuf::from)))
}
