//! Run a compatibility scenario against a local Pumpkin binary.

use std::io::Write;
use std::process::ExitCode;

fn stderr_line(text: &str) {
    std::io::stderr().write_all(text.as_bytes()).ok();
}

use pumpkin_compat::{pumpkin_binary, run_milestone_diff, run_pumpkin_smoke};

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
    match args.next().as_deref() {
        Some("smoke") => {
            let report = run_pumpkin_smoke(&pumpkin_binary()?).await?;
            stderr_line(&format!(
                "smoke ok: {} chunks, placed state {}, air state {}\n",
                report.chunks, report.placed_state, report.broken_state
            ));
            Ok(ExitCode::SUCCESS)
        }
        Some("diff") => {
            let report = run_milestone_diff(&pumpkin_binary()?).await?;
            stderr_line(&format!("{report}\n"));
            Ok(ExitCode::SUCCESS)
        }
        _ => {
            stderr_line(
                "usage: pumpkin-compat <smoke|diff>\n\
                 smoke  start Pumpkin and check login, spawn, chunks, place, and break\n\
                 diff   compare login milestones with a vanilla server downloaded from Mojang\n\
                 Set PUMPKIN_BIN to the pumpkin binary. Set JAVA_HOME to Java 25 for diff.\n",
            );
            Ok(ExitCode::from(2))
        }
    }
}
