//! Role: 実際のworkspaceで検査と図のファイルへの書出しの実行時間を測定する。
use std::time::Instant;
use xtask_core::{Config, Output, run};

fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [root, mode] = args.as_slice() else {
        return Err("usage: benchmark ROOT check|write".into());
    };
    let mut config = Config::new(root);
    config.output = match mode.as_str() {
        "check" => Output::None,
        "write" => Output::Policy,
        _ => return Err("mode must be check or write".into()),
    };
    let start = Instant::now();
    let report = run(&config)?;
    println!(
        "seconds={:.6} errors={} archive={}",
        start.elapsed().as_secs_f64(),
        report.errors.len() + report.style.len(),
        report.archive.is_some()
    );
    for note in report
        .notes
        .iter()
        .filter(|note| note.starts_with("analyzed "))
    {
        println!("{note}");
    }
    for error in &report.errors {
        eprintln!("error: {error}");
    }
    for diagnostic in &report.style {
        eprintln!("error: {diagnostic}");
    }
    if !report.is_success() {
        return Err(format!(
            "benchmark found {} check errors",
            report.errors.len() + report.style.len()
        ));
    }
    Ok(())
}
