use pumpkin_data::packet::clientbound::{
    config::{FINISH_CONFIGURATION, REGISTRY_DATA},
    login::LOGIN_FINISHED,
    play::{LEVEL_CHUNK_WITH_LIGHT, LOGIN, PLAYER_POSITION},
};
use pumpkin_util::version::JavaMinecraftVersion;

/// Which side of the connection produced a packet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Client to server.
    Out,
    /// Server to client.
    In,
}

/// Java protocol phase the client believed it was in when the packet was seen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Before the handshake next-state is applied.
    Handshake,
    /// Server list ping.
    Status,
    /// Authentication.
    Login,
    /// Registry and tag sync.
    Config,
    /// In-world packets.
    Play,
}

/// One framed packet, without its body.
///
/// Bodies are not kept: chunk packets are large, and milestone checks only need
/// the id and the phase the client was in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PacketRecord {
    /// Which side sent it.
    pub direction: Direction,
    /// Phase at the time it was sent or received.
    pub phase: Phase,
    /// Packet id in that phase.
    pub id: i32,
    /// Length of the id plus the payload, before the length prefix.
    pub payload_len: usize,
}

/// A packet a finished login is expected to have seen, inbound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Milestone {
    /// Short name used in diffs.
    pub name: &'static str,
    /// Phase the packet belongs to.
    pub phase: Phase,
    /// Packet id.
    pub id: i32,
}

/// Inbound packets that mean a 1.20.2+ client finished login and received a world.
///
/// These are structural. Chunk contents and spawn coordinates are not compared,
/// because Pumpkin worldgen is not byte-identical to vanilla.
#[must_use]
pub fn login_milestones(version: JavaMinecraftVersion) -> Vec<Milestone> {
    vec![
        Milestone {
            name: "login_success",
            phase: Phase::Login,
            id: LOGIN_FINISHED.to_id(version),
        },
        Milestone {
            name: "registry_data",
            phase: Phase::Config,
            id: REGISTRY_DATA.to_id(version),
        },
        Milestone {
            name: "finish_configuration",
            phase: Phase::Config,
            id: FINISH_CONFIGURATION.to_id(version),
        },
        Milestone {
            name: "join_game",
            phase: Phase::Play,
            id: LOGIN.to_id(version),
        },
        Milestone {
            name: "player_position",
            phase: Phase::Play,
            id: PLAYER_POSITION.to_id(version),
        },
        Milestone {
            name: "chunk_data",
            phase: Phase::Play,
            id: LEVEL_CHUNK_WITH_LIGHT.to_id(version),
        },
    ]
}

/// Milestone names present in `expected` and absent from inbound `trace` records.
#[must_use]
pub fn missing_milestones(trace: &[PacketRecord], expected: &[Milestone]) -> Vec<&'static str> {
    expected
        .iter()
        .filter(|milestone| {
            !trace.iter().any(|record| {
                record.direction == Direction::In
                    && record.phase == milestone.phase
                    && record.id == milestone.id
            })
        })
        .map(|milestone| milestone.name)
        .collect()
}

/// Human-readable milestone diff. Empty means both traces contain every milestone.
#[must_use]
pub fn milestone_diff(
    left_name: &str,
    left: &[PacketRecord],
    right_name: &str,
    right: &[PacketRecord],
    version: JavaMinecraftVersion,
) -> String {
    let milestones = login_milestones(version);
    let left_missing = missing_milestones(left, &milestones);
    let right_missing = missing_milestones(right, &milestones);
    let mut lines = Vec::new();
    if left_missing.is_empty() && right_missing.is_empty() {
        lines.push(format!(
            "{left_name} and {right_name} both reached login, registry sync, join game, teleport, and a chunk."
        ));
    } else {
        if !left_missing.is_empty() {
            lines.push(format!("{left_name} missing: {}", left_missing.join(", ")));
        }
        if !right_missing.is_empty() {
            lines.push(format!(
                "{right_name} missing: {}",
                right_missing.join(", ")
            ));
        }
    }
    lines.push(format!(
        "inbound packet counts: {left_name}={}, {right_name}={}",
        inbound_count(left),
        inbound_count(right)
    ));
    lines.join("\n")
}

fn inbound_count(trace: &[PacketRecord]) -> usize {
    trace
        .iter()
        .filter(|record| record.direction == Direction::In)
        .count()
}

/// True when every login milestone is present on both sides.
#[must_use]
pub fn milestones_match(
    left: &[PacketRecord],
    right: &[PacketRecord],
    version: JavaMinecraftVersion,
) -> bool {
    let milestones = login_milestones(version);
    missing_milestones(left, &milestones).is_empty()
        && missing_milestones(right, &milestones).is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_data::packet::CURRENT_MC_VERSION;

    #[test]
    fn a_login_success_alone_does_not_satisfy_spawn_milestones() {
        let version = CURRENT_MC_VERSION;
        let milestones = login_milestones(version);
        let trace = [PacketRecord {
            direction: Direction::In,
            phase: Phase::Login,
            id: LOGIN_FINISHED.to_id(version),
            payload_len: 1,
        }];
        let missing = missing_milestones(&trace, &milestones);
        assert!(!missing.contains(&"login_success"));
        assert!(missing.contains(&"chunk_data"));
        assert!(missing.contains(&"join_game"));
        assert!(missing.contains(&"registry_data"));
    }
}
