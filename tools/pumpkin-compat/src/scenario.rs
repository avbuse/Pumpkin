//! Scripted sessions: status, login through chunk receipt, and a block place/break.

use std::net::SocketAddr;
use std::time::Duration;

use pumpkin_data::Block;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;

use pumpkin_data::packet::CURRENT_MC_VERSION;

use crate::client::{self, Bot, SpawnInfo};
use crate::error::Error;
use crate::pumpkin_proc::PumpkinProcess;
use crate::trace::{self, PacketRecord};
use crate::vanilla::VanillaProcess;

const LOGIN_BUDGET: Duration = Duration::from_secs(180);
const BLOCK_BUDGET: Duration = Duration::from_secs(20);

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

/// Status ping, login, chunk receipt, then place and break one stone block.
///
/// The console lays a gold block in the loaded chunk so the click has a solid
/// face even when the surface at spawn is air or water.
pub async fn smoke_session(
    addr: SocketAddr,
    server: &mut PumpkinProcess,
) -> Result<SmokeReport, Error> {
    let status_json = client::fetch_status(addr).await?;
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

    let mut bot = Bot::connect(addr, "CompatBot").await?;
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

    Ok(SmokeReport {
        status_json,
        chunks: spawn.chunk_count,
        placed_state,
        broken_state,
        trace: bot.trace().to_vec(),
    })
}

/// Start Pumpkin, run the smoke session, and stop the server.
pub async fn run_pumpkin_smoke(binary: &std::path::Path) -> Result<SmokeReport, Error> {
    let mut server = PumpkinProcess::start(binary).await?;
    let addr = server.addr();
    let result = smoke_session(addr, &mut server).await;
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

/// Log into Pumpkin and into a freshly downloaded vanilla server, then compare login milestones.
pub async fn run_milestone_diff(binary: &std::path::Path) -> Result<String, Error> {
    let pumpkin_trace = {
        let server = PumpkinProcess::start(binary).await?;
        let traced = login_spawn(server.addr(), "CompatBot").await;
        finish_pumpkin(server, traced).await?
    };
    let vanilla_trace = {
        let server = VanillaProcess::start().await?;
        let traced = login_spawn(server.addr(), "CompatBot").await;
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
