//! Run a named compatibility scenario against a local Pumpkin binary.

use std::io::Write;
use std::process::ExitCode;

use pumpkin_compat::{find_scenario, pumpkin_binary, run_scenario, scenarios};

fn stderr_line(text: &str) {
    std::io::stderr().write_all(text.as_bytes()).ok();
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(code) => code,
        Err(err) => {
            stderr_line(&format!("pumpkin-compat: {err}\n"));
            ExitCode::from(1)
        }
    }
}

async fn run() -> Result<ExitCode, pumpkin_compat::Error> {
    let mut args = std::env::args().skip(1);
    let Some(name) = args.next() else {
        stderr_line(&usage());
        return Ok(ExitCode::from(2));
    };
    if find_scenario(&name).is_none() {
        stderr_line(&usage());
        return Ok(ExitCode::from(2));
    }
    let report = run_scenario(&pumpkin_binary()?, &name).await?;
    stderr_line(&format!("{}\n", report.summary));
    Ok(ExitCode::SUCCESS)
}

fn usage() -> String {
    use std::fmt::Write;

    let mut text = String::from("usage: pumpkin-compat <scenario>\n");
    for scenario in scenarios() {
        let _ = writeln!(text, "  {:<8}{}", scenario.name(), scenario.description());
    }
    text.push_str("Set PUMPKIN_BIN to the pumpkin binary. Set JAVA_HOME to Java 25 for diff.\n");
    text
}
