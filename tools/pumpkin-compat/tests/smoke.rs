#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use pumpkin_compat::{
    pumpkin_binary, run_config_packet_checks, run_milestone_diff, run_pumpkin_smoke,
};

#[tokio::test]
#[ignore = "starts Pumpkin; the compat workflow runs this"]
async fn pumpkin_smoke() {
    let binary = pumpkin_binary().expect("pumpkin binary");
    let report = run_pumpkin_smoke(&binary)
        .await
        .expect("login, spawn, chunk, place, and break");
    assert!(report.chunks > 0);
    assert!(report.status_json.contains("pumpkin-compat"));
}

#[tokio::test]
#[ignore = "starts Pumpkin; the compat workflow runs this"]
async fn config_packets() {
    let binary = pumpkin_binary().expect("pumpkin binary");
    run_config_packet_checks(&binary)
        .await
        .expect("configuration keep-alive reaches play and an unknown id is rejected");
}

#[tokio::test]
#[ignore = "downloads the vanilla jar; set PUMPKIN_COMPAT_VANILLA=1 and Java 25"]
async fn vanilla_milestones() {
    if std::env::var("PUMPKIN_COMPAT_VANILLA").ok().as_deref() != Some("1") {
        return;
    }
    let binary = pumpkin_binary().expect("pumpkin binary");
    let report = run_milestone_diff(&binary)
        .await
        .expect("pumpkin and vanilla login milestones");
    assert!(report.contains("pumpkin"));
    assert!(report.contains("vanilla"));
}
