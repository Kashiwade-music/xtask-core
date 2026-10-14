//! Role: 実際のworkspaceで解析とSVG出力の実行時間を測定する。
use std::path::PathBuf;
use std::time::Instant;
use xtask_core::{Config, analyze, run};

fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        return Err("usage: benchmark ROOT OUTPUT analyze|no-write|write".into());
    }
    let mut config = Config::new(&args[0]);
    config.output = Some(PathBuf::from(&args[1]));
    let start = Instant::now();
    let report = match args[2].as_str() {
        "analyze" => analyze(&config)?.report,
        "no-write" => {
            config.no_write = true;
            run(&config)?
        }
        "write" => run(&config)?,
        _ => return Err("mode must be analyze, no-write or write".into()),
    };
    println!(
        "seconds={:.6} errors={} artifacts={}",
        start.elapsed().as_secs_f64(),
        report.errors.len(),
        report.artifacts.len()
    );
    for note in report
        .notes
        .iter()
        .filter(|note| note.starts_with("analyzed "))
    {
        println!("{note}");
    }
    for error in &report.errors {
        eprintln!("{error}");
    }
    if !report.is_success() {
        return Err(format!(
            "benchmark found {} check errors",
            report.errors.len()
        ));
    }
    Ok(())
}
