//! Role: 実際のworkspaceで検査と図のファイルへの書出しの実行時間を測定する。
use std::time::Instant;
use xtask_core::{Config, Output, run};

fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [root] = args.as_slice() else {
        return Err("usage: benchmark ROOT".into());
    };
    let mut success = true;
    for (label, output) in [("check", Output::None), ("check --write", Output::Policy)] {
        let mut config = Config::new(root);
        config.output = output;
        success &= measure(label, &config)?;
    }
    if success {
        Ok(())
    } else {
        Err("benchmark found check errors".into())
    }
}

/// Run the checks once and print the time they take. Returns whether they found no errors.
fn measure(label: &str, config: &Config) -> Result<bool, String> {
    let start = Instant::now();
    let report = run(config)?;
    println!(
        "{label}: seconds={:.6} errors={} archive={}",
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
    Ok(report.is_success())
}
