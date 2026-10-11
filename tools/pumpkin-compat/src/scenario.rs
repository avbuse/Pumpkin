//! Scripted sessions: status, login through chunk receipt, and a block place/break.
//!
//! Named scenarios share one dispatch point. A connected scenario is a function
//! of the logged-in client and the running Pumpkin process, so a new check does
//! not copy the process launcher. Smoke is the first entry.

use std::future::Future;
use std::net::SocketAddr;
use std::path::Path;
use std::pin::Pin;
use std::time::Duration;

use pumpkin_data::Block;
use pumpkin_data::packet::CURRENT_MC_VERSION;

use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;

use crate::client::{self, Bot, SpawnInfo};
use crate::error::Error;
use crate::pumpkin_proc::PumpkinProcess;
use crate::trace::{self, PacketRecord};
use crate::vanilla::VanillaProcess;

const LOGIN_BUDGET: Duration = Duration::from_secs(180);
const BLOCK_BUDGET: Duration = Duration::from_secs(20);
const BOT_NAME: &str = "CompatBot";

const SMOKE: &str = "smoke";
const SPAWN: &str = "spawn";
const CONFIG: &str = "config";
const DIFF: &str = "diff";

/// What a successful Pumpkin smoke run observed.
#[derive(Clone, Debug)]
pub struct SmokeReport {
    /// Raw status JSON.
    pub status_json: String,
    /// Chunk columns seen before the player's chunk arrived.
    pub chunks: usize,
    /// State id of the block the bot placed.
    pub placed_state: i32,
    /// State id observed after the bot broke that block.
    pub broken_state: i32,
    /// Packet trace from the play session (not the status ping).
    pub trace: Vec<PacketRecord>,
}

/// Outcome of one named scenario.
#[derive(Debug)]
pub struct ScenarioResult {
    /// One line for the harness binary.
    pub summary: String,
    /// Status JSON when the scenario fetched it, otherwise empty.
    pub status_json: String,
    /// Chunk columns seen before the player's chunk arrived.
    pub chunks: usize,
    /// State id of a placed block, or 0 when the scenario did not place one.
    pub placed_state: i32,
    /// State id left after a dig, or 0 when the scenario did not dig.
    pub broken_state: i32,
    /// Packet trace from the play session.
    pub trace: Vec<PacketRecord>,
}

/// Connected scenario: logged-in client, running server, status JSON from before login.
type SessionFn =
    for<'a> fn(
        &'a mut Bot,
        &'a mut PumpkinProcess,
        &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<ScenarioResult, Error>> + Send + 'a>>;

/// Scenario that opens its own connections.
type DirectFn =
    for<'a> fn(
        &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<ScenarioResult, Error>> + Send + 'a>>;

#[derive(Clone, Copy)]
enum Launch {
    /// Receives the connected client and the running server. One launcher.
    Connected(SessionFn),
    /// Starts whatever processes it needs. Config probes and the vanilla diff.
    Direct(DirectFn),
}

/// A named harness scenario.
pub struct Scenario {
    name: &'static str,
    description: &'static str,
    launch: Launch,
}

impl Scenario {
    /// Registry name used by the binary and by [`run_scenario`](crate::run_scenario).
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// One line for the usage text.
    #[must_use]
    pub const fn description(&self) -> &'static str {
        self.description
    }
}

/// Smoke stays first. Later checks append a name here instead of a new launcher.
static SCENARIOS: [Scenario; 4] = [
    Scenario {
        name: SMOKE,
        description: "start Pumpkin and check login, spawn, chunks, place, and break",
        launch: Launch::Connected(smoke_entry),
    },
    Scenario {
        name: SPAWN,
        description: "log in and confirm the spawn chunk arrived",
        launch: Launch::Connected(spawn_entry),
    },
    Scenario {
        name: CONFIG,
        description: "reject an unknown configuration packet, then join after a keep-alive",
        launch: Launch::Direct(config_entry),
    },
    Scenario {
        name: DIFF,
        description: "compare login milestones with a vanilla server downloaded from Mojang",
        launch: Launch::Direct(diff_entry),
    },
];

/// Scenarios in registry order. Smoke is first.
#[must_use]
pub const fn scenarios() -> &'static [Scenario] {
    &SCENARIOS
}

/// Look up a scenario by its registry name.
#[must_use]
pub fn find_scenario(name: &str) -> Option<&'static Scenario> {
    scenarios().iter().find(|scenario| scenario.name == name)
}

/// Run the named scenario.
///
/// Connected scenarios (`smoke`, `spawn`) share one Pumpkin process launcher:
/// status, login, the scenario function, then shutdown. An unknown name fails
/// before that launcher starts.
pub async fn run_scenario(binary: &Path, name: &str) -> Result<ScenarioResult, Error> {
    let scenario = find_scenario(name).ok_or_else(|| unknown_scenario(name))?;
    match scenario.launch {
        Launch::Connected(run) => launch_connected(binary, run).await,
        Launch::Direct(run) => run(binary).await,
    }
}

/// Status ping, then a logged-in client, then `run`, then shutdown.
///
/// The status connection happens before login so `smoke` keeps its current
/// order: status, login, chunks, place, dig.
async fn launch_connected(binary: &Path, run: SessionFn) -> Result<ScenarioResult, Error> {
    let mut server = PumpkinProcess::start(binary).await?;
    let addr = server.addr();
    let result = async {
        let status_json = client::fetch_status(addr).await?;
        let mut bot = Bot::connect(addr, BOT_NAME).await?;
        run(&mut bot, &mut server, &status_json).await
    }
    .await;
    if let Err(err) = &result {
        let log = server.log_tail();
        let _ = server.shutdown().await;
        return Err(Error::scenario(format!(
            "{err}\n--- pumpkin log ---\n{log}"
        )));
    }
    server.shutdown().await?;
    result
}

fn unknown_scenario(name: &str) -> Error {
    let mut known = String::new();
    for (index, scenario) in scenarios().iter().enumerate() {
        if index > 0 {
            known.push_str(", ");
        }
        known.push_str(scenario.name());
    }
    Error::scenario(format!("unknown scenario '{name}'. known: {known}"))
}

fn text_result(summary: impl Into<String>) -> ScenarioResult {
    ScenarioResult {
        summary: summary.into(),
        status_json: String::new(),
        chunks: 0,
        placed_state: 0,
        broken_state: 0,
        trace: Vec::new(),
    }
}

fn smoke_entry<'a>(
    bot: &'a mut Bot,
    server: &'a mut PumpkinProcess,
    status_json: &'a str,
) -> Pin<Box<dyn Future<Output = Result<ScenarioResult, Error>> + Send + 'a>> {
    Box::pin(smoke_session(bot, server, status_json))
}

fn spawn_entry<'a>(
    bot: &'a mut Bot,
    server: &'a mut PumpkinProcess,
    status_json: &'a str,
) -> Pin<Box<dyn Future<Output = Result<ScenarioResult, Error>> + Send + 'a>> {
    Box::pin(spawn_session(bot, server, status_json))
}

fn config_entry<'a>(
    binary: &'a Path,
) -> Pin<Box<dyn Future<Output = Result<ScenarioResult, Error>> + Send + 'a>> {
    Box::pin(async move {
        config_packet_checks(binary).await?;
        Ok(text_result(
            "config ok: unknown configuration id rejected, keep-alive reached play",
        ))
    })
}

fn diff_entry<'a>(
    binary: &'a Path,
) -> Pin<Box<dyn Future<Output = Result<ScenarioResult, Error>> + Send + 'a>> {
    Box::pin(async move {
        let summary = diff_milestones(binary).await?;
        Ok(text_result(summary))
    })
}

/// Send one configuration keep-alive before finish acknowledgement, then join play.
pub async fn config_keep_alive_reaches_play(addr: SocketAddr) -> Result<SpawnInfo, Error> {
    let mut bot = Bot::connect_with_config_keep_alive(addr, "KeepAliveBot").await?;
    let spawn = bot.await_spawn(LOGIN_BUDGET).await?;
    if spawn.chunk_count == 0 {
        return Err(Error::scenario(
            "configuration keep-alive joined play but no chunk arrived",
        ));
    }
    Ok(spawn)
}

/// An id outside the configuration set still disconnects the client.
pub async fn unknown_config_id_is_rejected(addr: SocketAddr) -> Result<(), Error> {
    Bot::expect_unknown_config_rejected(addr, "UnknownBot").await
}

async fn config_packet_checks(binary: &Path) -> Result<(), Error> {
    let server = PumpkinProcess::start(binary).await?;
    let addr = server.addr();
    let result: Result<(), Error> = async {
        unknown_config_id_is_rejected(addr).await?;
        config_keep_alive_reaches_play(addr).await?;
        Ok(())
    }
    .await;
    if let Err(err) = result {
        let log = server.log_tail();
        let _ = server.shutdown().await;
        return Err(Error::scenario(format!(
            "{err}\n--- pumpkin log ---\n{log}"
        )));
    }
    server.shutdown().await
}

/// One server: unknown configuration ids still kick, and a keep-alive reaches play.
pub async fn run_config_packet_checks(binary: &Path) -> Result<(), Error> {
    run_scenario(binary, CONFIG).await?;
    Ok(())
}

/// Login far enough to record a milestone trace. Does not place blocks.
pub async fn login_spawn(
    addr: SocketAddr,
    username: &str,
) -> Result<(SpawnInfo, Vec<PacketRecord>), Error> {
    let mut bot = Bot::connect(addr, username).await?;
    let spawn = bot.await_spawn(LOGIN_BUDGET).await?;
    if spawn.chunk_count == 0 {
        return Err(Error::scenario(
            "joined play but no chunk packet arrived before the player's chunk",
        ));
    }
    Ok((spawn, bot.trace().to_vec()))
}

/// Place and break one stone block. The launcher has already status-pinged and logged in.
///
/// The console lays a gold block in the loaded chunk so the click has a solid
/// face even when the surface at spawn is air or water.
async fn smoke_session(
    bot: &mut Bot,
    server: &mut PumpkinProcess,
    status_json: &str,
) -> Result<ScenarioResult, Error> {
    let status_json = status_json.to_owned();
    let parsed: serde_json::Value = serde_json::from_str(&status_json)
        .map_err(|err| Error::scenario(format!("status body is not json: {err}")))?;
    if parsed.get("version").is_none() {
        return Err(Error::scenario(format!(
            "status json has no version field: {status_json}"
        )));
    }
    if !status_json.contains("pumpkin-compat") {
        return Err(Error::scenario(format!(
            "status json did not contain the smoke motd: {status_json}"
        )));
    }

    let spawn = bot.await_spawn(LOGIN_BUDGET).await?;
    if spawn.chunk_count == 0 {
        return Err(Error::scenario("no chunk packet before spawn completed"));
    }

    let (ground, above) = fixture_near(spawn.feet);
    server
        .command(&format!(
            "setblock {} {} {} gold_block",
            ground.0.x, ground.0.y, ground.0.z
        ))
        .await?;
    bot.wait_for_state(ground, state_of(&Block::GOLD_BLOCK), BLOCK_BUDGET)
        .await
        .map_err(|err| Error::scenario(format!("gold fixture was not applied: {err}")))?;

    bot.hold_stone().await?;
    bot.place_on_top(ground).await?;
    let placed_state = state_of(&Block::STONE);
    bot.wait_for_state(above, placed_state, BLOCK_BUDGET)
        .await
        .map_err(|err| Error::scenario(format!("stone place was not observed: {err}")))?;

    bot.dig(above).await?;
    let broken_state = state_of(&Block::AIR);
    bot.wait_for_state(above, broken_state, BLOCK_BUDGET)
        .await
        .map_err(|err| Error::scenario(format!("stone break was not observed: {err}")))?;

    let chunks = spawn.chunk_count;
    Ok(ScenarioResult {
        summary: format!(
            "smoke ok: {chunks} chunks, placed state {placed_state}, air state {broken_state}"
        ),
        status_json,
        chunks,
        placed_state,
        broken_state,
        trace: bot.trace().to_vec(),
    })
}

/// Login through the spawn chunk. Does not place or break.
#[expect(
    clippy::needless_pass_by_ref_mut,
    reason = "connected scenarios share one signature; smoke writes console commands through it"
)]
async fn spawn_session(
    bot: &mut Bot,
    server: &mut PumpkinProcess,
    status_json: &str,
) -> Result<ScenarioResult, Error> {
    let spawn = bot.await_spawn(LOGIN_BUDGET).await?;
    if spawn.chunk_count == 0 {
        return Err(Error::scenario(
            "joined play but no chunk packet arrived before the player's chunk",
        ));
    }
    let chunks = spawn.chunk_count;
    let addr = server.addr();
    Ok(ScenarioResult {
        summary: format!("spawn ok: {chunks} chunks from {addr}"),
        status_json: status_json.to_owned(),
        chunks,
        placed_state: 0,
        broken_state: 0,
        trace: bot.trace().to_vec(),
    })
}

/// Start Pumpkin, run the smoke scenario, and stop the server.
pub async fn run_pumpkin_smoke(binary: &Path) -> Result<SmokeReport, Error> {
    let result = run_scenario(binary, SMOKE).await?;
    Ok(SmokeReport {
        status_json: result.status_json,
        chunks: result.chunks,
        placed_state: result.placed_state,
        broken_state: result.broken_state,
        trace: result.trace,
    })
}

async fn diff_milestones(binary: &Path) -> Result<String, Error> {
    let pumpkin_trace = {
        let server = PumpkinProcess::start(binary).await?;
        let traced = login_spawn(server.addr(), BOT_NAME).await;
        finish_pumpkin(server, traced).await?
    };
    let vanilla_trace = {
        let server = VanillaProcess::start().await?;
        let traced = login_spawn(server.addr(), BOT_NAME).await;
        finish_vanilla(server, traced).await?
    };
    let version = CURRENT_MC_VERSION;
    let report = trace::milestone_diff(
        "pumpkin",
        &pumpkin_trace,
        "vanilla",
        &vanilla_trace,
        version,
    );
    if trace::milestones_match(&pumpkin_trace, &vanilla_trace, version) {
        Ok(report)
    } else {
        Err(Error::scenario(report))
    }
}

/// Log into Pumpkin and into a freshly downloaded vanilla server, then compare login milestones.
pub async fn run_milestone_diff(binary: &Path) -> Result<String, Error> {
    Ok(run_scenario(binary, DIFF).await?.summary)
}

async fn finish_pumpkin(
    server: PumpkinProcess,
    traced: Result<(SpawnInfo, Vec<PacketRecord>), Error>,
) -> Result<Vec<PacketRecord>, Error> {
    match traced {
        Ok((_spawn, packets)) => {
            server.shutdown().await?;
            Ok(packets)
        }
        Err(err) => {
            let log = server.log_tail();
            drop(server.shutdown().await);
            Err(Error::scenario(format!(
                "{err}\n--- pumpkin log ---\n{log}"
            )))
        }
    }
}

async fn finish_vanilla(
    server: VanillaProcess,
    traced: Result<(SpawnInfo, Vec<PacketRecord>), Error>,
) -> Result<Vec<PacketRecord>, Error> {
    match traced {
        Ok((_spawn, packets)) => {
            server.shutdown().await?;
            Ok(packets)
        }
        Err(err) => {
            let log = server.log_tail();
            drop(server.shutdown().await);
            Err(Error::scenario(format!(
                "{err}\n--- vanilla log ---\n{log}"
            )))
        }
    }
}

/// A gold block a few blocks away from the player, still inside the same chunk,
/// and the air cell above it. Three blocks is inside creative reach.
#[must_use]
pub const fn fixture_near(feet: Vector3<f64>) -> (BlockPos, BlockPos) {
    let x = feet.x.floor() as i32;
    let y = feet.y.floor() as i32;
    let z = feet.z.floor() as i32;
    let local_x = x.rem_euclid(16);
    let dx = if local_x <= 8 { 3 } else { -3 };
    let ground = BlockPos::new(x + dx, y - 1, z);
    let above = BlockPos::new(ground.0.x, ground.0.y + 1, ground.0.z);
    (ground, above)
}

fn state_of(block: &Block) -> i32 {
    i32::from(block.default_state.id.as_u16())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{Launch, find_scenario, run_scenario, scenarios};

    fn is_connected(scenario: &super::Scenario) -> bool {
        matches!(scenario.launch, Launch::Connected(_))
    }

    #[test]
    fn smoke_is_the_first_scenario_and_spawn_shares_its_launcher() {
        let names: Vec<_> = scenarios().iter().map(super::Scenario::name).collect();
        assert_eq!(names, ["smoke", "spawn", "config", "diff"]);

        let smoke = find_scenario("smoke").expect("smoke");
        let spawn = find_scenario("spawn").expect("spawn");
        assert!(is_connected(smoke));
        assert!(is_connected(spawn));
        match (smoke.launch, spawn.launch) {
            (Launch::Connected(smoke_run), Launch::Connected(spawn_run)) => {
                assert!(
                    !std::ptr::fn_addr_eq(smoke_run, spawn_run),
                    "spawn must be its own scenario function"
                );
            }
            _ => panic!("smoke and spawn must use the shared launcher"),
        }

        let config = find_scenario("config").expect("config");
        let diff = find_scenario("diff").expect("diff");
        assert!(!is_connected(config));
        assert!(!is_connected(diff));
        assert!(find_scenario("nope").is_none());
    }

    #[tokio::test]
    async fn unknown_scenario_is_rejected_before_launch() {
        let err = run_scenario(Path::new("/no/such/pumpkin-compat-binary"), "nope")
            .await
            .expect_err("unknown name");
        let message = err.to_string();
        assert!(message.contains("unknown scenario"));
        assert!(message.contains("smoke"));
        assert!(!message.contains("Pumpkin binary not found"));
    }

    #[tokio::test]
    async fn registered_names_reach_a_pumpkin_launch() {
        for name in ["smoke", "spawn", "config", "diff"] {
            let err = run_scenario(Path::new("/no/such/pumpkin-compat-binary"), name)
                .await
                .expect_err("missing binary");
            let message = err.to_string();
            assert!(
                message.contains("Pumpkin binary not found"),
                "{name} did not reach a pumpkin launch: {message}"
            );
            assert!(
                !message.contains("unknown scenario"),
                "{name} was not registered"
            );
        }
    }
}
