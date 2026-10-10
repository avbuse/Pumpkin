//! Headless Java Edition client built on Pumpkin's own packet codecs.
//!
//! Azalea targets protocol 776 (26.2). This server speaks 777 (26.3), so the
//! harness uses `pumpkin-protocol` and stays on the same packet ids as the
//! server under test.

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use pumpkin_data::packet::CURRENT_MC_VERSION;
use pumpkin_data::packet::clientbound::config::{
    DISCONNECT as CONFIG_DISCONNECT, FINISH_CONFIGURATION, REGISTRY_DATA, SELECT_KNOWN_PACKS,
};
use pumpkin_data::packet::clientbound::login::{
    ENCRYPTION_REQUEST, LOGIN_COMPRESSION, LOGIN_DISCONNECT, LOGIN_FINISHED,
};
use pumpkin_data::packet::clientbound::play::{
    BLOCK_UPDATE, CHUNK_BATCH_FINISHED, DISCONNECT as PLAY_DISCONNECT,
    KEEP_ALIVE as PLAY_KEEP_ALIVE, LEVEL_CHUNK_WITH_LIGHT, LOGIN, PLAYER_POSITION,
    SECTION_BLOCKS_UPDATE,
};
use pumpkin_data::packet::clientbound::status::STATUS_RESPONSE;
use pumpkin_protocol::java::client::login::CSetCompression;
use pumpkin_protocol::java::client::play::{CKeepAlive, CMultiBlockUpdate, CPlayerPosition};
use pumpkin_protocol::java::client::status::CStatusResponse;
use pumpkin_protocol::java::packet_decoder::TCPNetworkDecoder;
use pumpkin_protocol::java::packet_encoder::TCPNetworkEncoder;
use pumpkin_protocol::java::server::config::{
    PARTICLE_STATUS_ALL, SAcknowledgeFinishConfig, SClientInformationConfig,
    SKeepAlive as SConfigKeepAlive, SKnownPacks,
};
use pumpkin_protocol::java::server::handshake::SHandShake;
use pumpkin_protocol::java::server::login::{SLoginAcknowledged, SLoginStart};
use pumpkin_protocol::java::server::play::{
    SChunkBatch, SConfirmTeleport, SKeepAlive, SPlayerAction, SPlayerLoaded, SPlayerPosition,
    SSetCreativeSlot, SUseItemOn, Status as DigStatus,
};
use pumpkin_protocol::java::server::status::SStatusRequest;
use pumpkin_protocol::ser::NetworkReadExt;
use pumpkin_protocol::ser::NetworkWriteExt;
use pumpkin_protocol::{ClientPacket, ConnectionState, ServerPacket, VarInt};
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_util::version::JavaMinecraftVersion;
use tokio::net::TcpStream;
use tokio::time::timeout;
use uuid::Uuid;

use crate::error::Error;
use crate::trace::{Direction, PacketRecord, Phase};

const CHUNK_ACK_PER_TICK: f32 = 32.0;
/// Challenge id on the keep-alive sent during configuration. The server has not
/// issued one; the packet only has to be accepted.
const CONFIG_KEEP_ALIVE_ID: i64 = 1;
/// Outside the 26.3 serverbound configuration set (ids 0 through 9).
const UNKNOWN_CONFIG_PACKET_ID: i32 = 127;

/// Extra configuration packet sent by a scenario, on top of the normal login.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConfigProbe {
    /// Login used by the smoke session and by vanilla milestones.
    None,
    /// One serverbound keep-alive before finish acknowledgement.
    KeepAlive,
    /// An id the configuration phase does not dispatch.
    Unknown,
}

/// A packet the scenario layer cares about. Protocol obligations (keep-alive,
/// teleport confirm, chunk-batch ack, compression) are handled before this is
/// returned.
#[derive(Debug)]
pub enum Incoming {
    /// Packet with no scenario-level meaning.
    Other {
        /// Packet id.
        id: i32,
    },
    /// Login finished. The client has already acknowledged it.
    LoginSuccess,
    /// Server's known-pack list. The caller answers it.
    KnownPacks,
    /// Server asked the client to leave configuration.
    FinishConfiguration,
    /// One registry blob during configuration.
    RegistryData,
    /// Play login ("join game").
    JoinGame,
    /// Forced teleport. Already confirmed.
    Teleport {
        /// Teleport id echoed back to the server.
        id: i32,
        /// Feet position from the packet.
        position: Vector3<f64>,
    },
    /// One chunk column.
    Chunk {
        /// Chunk X.
        x: i32,
        /// Chunk Z.
        z: i32,
    },
    /// A block state the server says changed.
    BlockChange {
        /// World position.
        pos: BlockPos,
        /// Block state id.
        state: i32,
    },
}

/// Spawn observation after the play login.
#[derive(Clone, Copy, Debug)]
pub struct SpawnInfo {
    /// Feet position from the teleport the server sent.
    pub feet: Vector3<f64>,
    /// Chunk columns received before the player's own chunk arrived.
    pub chunk_count: usize,
}

/// Connected client in the play phase, or mid-handshake while `connect` runs.
pub struct Bot {
    encoder: TCPNetworkEncoder<tokio::net::tcp::OwnedWriteHalf>,
    decoder: TCPNetworkDecoder<tokio::net::tcp::OwnedReadHalf>,
    version: JavaMinecraftVersion,
    phase: Phase,
    trace: Vec<PacketRecord>,
    sequence: i32,
    feet: Option<Vector3<f64>>,
    loaded_sent: bool,
    recent: VecDeque<(Phase, i32)>,
    pending: VecDeque<Incoming>,
}

impl Bot {
    /// Offline-mode login through configuration, stopping at the start of play.
    pub async fn connect(addr: SocketAddr, username: &str) -> Result<Self, Error> {
        Self::open(addr, username, ConfigProbe::None).await
    }

    /// Same login as `connect`, plus one configuration keep-alive before the
    /// finish acknowledgement.
    pub async fn connect_with_config_keep_alive(
        addr: SocketAddr,
        username: &str,
    ) -> Result<Self, Error> {
        Self::open(addr, username, ConfigProbe::KeepAlive).await
    }

    /// Log in far enough to send an unknown configuration id, and succeed when
    /// the server disconnects instead of entering play.
    pub async fn expect_unknown_config_rejected(
        addr: SocketAddr,
        username: &str,
    ) -> Result<(), Error> {
        match Self::open(addr, username, ConfigProbe::Unknown).await {
            Ok(_) => Err(Error::scenario(
                "unknown configuration packet did not disconnect the client",
            )),
            Err(err) if err.to_string().contains("disconnected during") => Ok(()),
            Err(err) => Err(err),
        }
    }

    async fn open(addr: SocketAddr, username: &str, probe: ConfigProbe) -> Result<Self, Error> {
        let stream = timeout(Duration::from_secs(10), TcpStream::connect(addr))
            .await
            .map_err(|_| Error::scenario(format!("timed out connecting to {addr}")))?
            .map_err(|err| Error::io("connect", err))?;
        stream
            .set_nodelay(true)
            .map_err(|err| Error::io("set nodelay", err))?;
        let (read, write) = stream.into_split();
        let mut bot = Self {
            encoder: TCPNetworkEncoder::new(write),
            decoder: TCPNetworkDecoder::new(read),
            version: CURRENT_MC_VERSION,
            phase: Phase::Handshake,
            trace: Vec::new(),
            sequence: 1,
            feet: None,
            loaded_sent: false,
            recent: VecDeque::with_capacity(16),
            pending: VecDeque::new(),
        };
        bot.send_packet(&SHandShake {
            protocol_version: VarInt(bot.version.protocol_version()),
            server_address: "127.0.0.1".into(),
            server_port: addr.port(),
            next_state: ConnectionState::Login,
        })
        .await?;
        bot.phase = Phase::Login;
        let uuid = Uuid::new_v3(&Uuid::nil(), format!("OfflinePlayer:{username}").as_bytes());
        bot.send_packet(&SLoginStart {
            name: username.into(),
            uuid,
        })
        .await?;
        bot.finish_login().await?;
        bot.finish_config(probe).await?;
        Ok(bot)
    }

    /// Read until the server has teleported us and sent the chunk we stand in.
    pub async fn await_spawn(&mut self, budget: Duration) -> Result<SpawnInfo, Error> {
        let deadline = Instant::now() + budget;
        // Chunks can arrive before the teleport. Remember every column until the
        // one that contains the feet position shows up.
        let mut columns = Vec::new();
        loop {
            if let Some(feet) = self.feet
                && columns.contains(&chunk_coords(feet))
            {
                return Ok(SpawnInfo {
                    feet,
                    chunk_count: columns.len(),
                });
            }
            match self.recv_until(deadline).await? {
                Incoming::Chunk { x, z } => columns.push((x, z)),
                Incoming::Teleport { .. }
                | Incoming::Other { .. }
                | Incoming::JoinGame
                | Incoming::RegistryData
                | Incoming::LoginSuccess
                | Incoming::KnownPacks
                | Incoming::FinishConfiguration
                | Incoming::BlockChange { .. } => {}
            }
        }
    }

    /// Put one stone in hotbar slot 0. The player must be in creative mode.
    pub async fn hold_stone(&mut self) -> Result<(), Error> {
        let stack = pumpkin_data::item_stack::ItemStack::new(1, &pumpkin_data::item::Item::STONE);
        self.send_packet(&SSetCreativeSlot::new(
            36i16,
            pumpkin_protocol::codec::item_stack_seralizer::ItemStackSerializer(
                std::borrow::Cow::Owned(stack),
            ),
        ))
        .await
    }

    /// Right-click the top face of `target`, which places into the block above it.
    pub async fn place_on_top(&mut self, target: BlockPos) -> Result<(), Error> {
        let sequence = self.next_sequence();
        self.send_packet(&SUseItemOn {
            hand: VarInt(0),
            position: target,
            face: VarInt(1),
            cursor_pos: Vector3::new(0.5f32, 1.0f32, 0.5f32),
            inside_block: false,
            is_against_world_border: false,
            sequence,
        })
        .await
    }

    /// Creative instant-break. Sends the start-digging action only, matching a creative client.
    pub async fn dig(&mut self, pos: BlockPos) -> Result<(), Error> {
        let sequence = self.next_sequence();
        self.send_packet(&SPlayerAction {
            status: VarInt(DigStatus::StartedDigging as i32),
            position: pos,
            face: 1,
            sequence,
        })
        .await
    }

    /// Read until a block update sets `pos` to `state`, or the deadline passes.
    pub async fn wait_for_state(
        &mut self,
        pos: BlockPos,
        state: i32,
        budget: Duration,
    ) -> Result<(), Error> {
        let deadline = Instant::now() + budget;
        let mut seen = Vec::new();
        loop {
            match self.recv_until(deadline).await {
                Ok(Incoming::BlockChange {
                    pos: changed,
                    state: got,
                }) if changed == pos => {
                    if got == state {
                        return Ok(());
                    }
                    seen.push(got);
                }
                Ok(_) => {}
                Err(err) => {
                    return Err(Error::scenario(format!(
                        "{err}; states seen at {} {} {}: {:?}",
                        pos.0.x, pos.0.y, pos.0.z, seen
                    )));
                }
            }
        }
    }

    /// Packet trace accumulated since connect.
    #[must_use]
    pub fn trace(&self) -> &[PacketRecord] {
        &self.trace
    }

    /// Feet position from the last confirmed teleport.
    #[must_use]
    pub const fn feet(&self) -> Option<Vector3<f64>> {
        self.feet
    }

    async fn finish_login(&mut self) -> Result<(), Error> {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            match self.recv_until(deadline).await? {
                Incoming::LoginSuccess => {
                    self.send_packet(&SLoginAcknowledged).await?;
                    self.phase = Phase::Config;
                    return Ok(());
                }
                Incoming::Other { .. } => {}
                other => {
                    return Err(Error::scenario(format!(
                        "unexpected packet during login: {other:?}"
                    )));
                }
            }
        }
    }

    async fn send_client_information(&mut self) -> Result<(), Error> {
        self.send_packet(&SClientInformationConfig {
            locale: "en_us",
            view_distance: 8i8,
            chat_mode: VarInt(0),
            chat_colors: true,
            skin_parts: 0x7f,
            main_hand: VarInt(1),
            text_filtering: false,
            server_listing: true,
            particle_status: VarInt(PARTICLE_STATUS_ALL),
        })
        .await
    }

    async fn finish_config(&mut self, probe: ConfigProbe) -> Result<(), Error> {
        self.send_client_information().await?;
        if probe == ConfigProbe::KeepAlive {
            self.send_packet(&SConfigKeepAlive {
                keep_alive_id: CONFIG_KEEP_ALIVE_ID,
            })
            .await?;
        }
        if probe == ConfigProbe::Unknown {
            self.send_unknown_config_packet().await?;
        }
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut answered_packs = false;
        loop {
            match self.recv_until(deadline).await? {
                Incoming::KnownPacks if !answered_packs && probe != ConfigProbe::Unknown => {
                    // An empty list asks the server to send full registry data.
                    self.send_packet(&SKnownPacks {
                        known_packs: Vec::new(),
                    })
                    .await?;
                    answered_packs = true;
                }
                Incoming::FinishConfiguration => {
                    if probe == ConfigProbe::Unknown {
                        return Err(Error::scenario(
                            "unknown configuration packet was accepted; the server asked the client to finish",
                        ));
                    }
                    self.send_packet(&SAcknowledgeFinishConfig).await?;
                    self.phase = Phase::Play;
                    return Ok(());
                }
                Incoming::KnownPacks | Incoming::RegistryData | Incoming::Other { .. } => {}
                other => {
                    return Err(Error::scenario(format!(
                        "unexpected packet during configuration: {other:?}"
                    )));
                }
            }
        }
    }

    async fn recv_until(&mut self, deadline: Instant) -> Result<Incoming, Error> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(Error::scenario(self.timeout_message()));
        }
        timeout(remaining, self.read_next())
            .await
            .unwrap_or_else(|_| Err(Error::scenario(self.timeout_message())))
    }

    async fn read_next(&mut self) -> Result<Incoming, Error> {
        if let Some(pending) = self.pending.pop_front() {
            return Ok(pending);
        }
        let packet = self
            .decoder
            .get_raw_packet()
            .await
            .map_err(|err| Error::scenario(format!("read packet: {err}")))?;
        self.note_in(packet.id, packet.payload.len());
        self.obligations(packet.id, &packet.payload).await?;
        self.classify(packet.id, &packet.payload)
    }

    async fn obligations(&mut self, id: i32, payload: &[u8]) -> Result<(), Error> {
        let version = self.version;
        if self.phase == Phase::Login && id == LOGIN_COMPRESSION.to_id(version) {
            self.enable_compression(payload)?;
        }
        if self.phase == Phase::Play && id == PLAY_KEEP_ALIVE.to_id(version) {
            self.answer_keep_alive(payload).await?;
        }
        if self.phase == Phase::Play && id == CHUNK_BATCH_FINISHED.to_id(version) {
            // Without this ack the chunk sender leaves a batch in flight and
            // slows or stops further chunk sends.
            self.send_packet(&SChunkBatch {
                chunks_per_tick: CHUNK_ACK_PER_TICK,
            })
            .await?;
        }
        if self.phase == Phase::Play && id == PLAYER_POSITION.to_id(version) {
            self.confirm_teleport(payload).await?;
        }
        Ok(())
    }

    fn enable_compression(&mut self, payload: &[u8]) -> Result<(), Error> {
        let mut slice = payload;
        let packet = CSetCompression::read(&mut slice, &self.version)
            .map_err(|err| Error::scenario(format!("set compression: {err}")))?;
        if packet.threshold.0 >= 0 {
            let threshold = packet.threshold.0 as usize;
            self.decoder.set_compression(threshold);
            self.encoder.set_compression((threshold, 1u32));
        }
        Ok(())
    }

    async fn answer_keep_alive(&mut self, payload: &[u8]) -> Result<(), Error> {
        let mut slice = payload;
        let packet = CKeepAlive::read(&mut slice, &self.version)
            .map_err(|err| Error::scenario(format!("keep alive: {err}")))?;
        self.send_packet(&SKeepAlive {
            keep_alive_id: packet.keep_alive_id,
        })
        .await
    }

    async fn confirm_teleport(&mut self, payload: &[u8]) -> Result<(), Error> {
        let mut slice = payload;
        let packet = CPlayerPosition::read(&mut slice, &self.version)
            .map_err(|err| Error::scenario(format!("teleport: {err}")))?;
        self.feet = Some(packet.position);
        self.send_packet(&SConfirmTeleport {
            teleport_id: packet.teleport_id,
            position: packet.position,
            yaw: packet.yaw,
            pitch: packet.pitch,
        })
        .await?;
        if !self.loaded_sent {
            self.send_packet(&SPlayerLoaded).await?;
            self.send_packet(&SPlayerPosition {
                position: packet.position,
                collision: 1,
            })
            .await?;
            self.loaded_sent = true;
        }
        Ok(())
    }

    fn classify(&mut self, id: i32, payload: &[u8]) -> Result<Incoming, Error> {
        let version = self.version;
        if self.is_disconnect(id) {
            return Err(Error::scenario(format!(
                "server disconnected during {:?}: {}",
                self.phase,
                payload_hint(payload)
            )));
        }
        if self.phase == Phase::Login && id == ENCRYPTION_REQUEST.to_id(version) {
            return Err(Error::scenario(
                "server requested encryption; the smoke config must set online_mode and encryption to false",
            ));
        }
        if self.phase == Phase::Login && id == LOGIN_FINISHED.to_id(version) {
            return Ok(Incoming::LoginSuccess);
        }
        if self.phase == Phase::Config && id == SELECT_KNOWN_PACKS.to_id(version) {
            return Ok(Incoming::KnownPacks);
        }
        if self.phase == Phase::Config && id == REGISTRY_DATA.to_id(version) {
            return Ok(Incoming::RegistryData);
        }
        if self.phase == Phase::Config && id == FINISH_CONFIGURATION.to_id(version) {
            return Ok(Incoming::FinishConfiguration);
        }
        if self.phase == Phase::Play && id == LOGIN.to_id(version) {
            return Ok(Incoming::JoinGame);
        }
        if self.phase == Phase::Play && id == PLAYER_POSITION.to_id(version) {
            let position = self.feet.unwrap_or(Vector3::new(0.0, 0.0, 0.0));
            return Ok(Incoming::Teleport { id, position });
        }
        if self.phase == Phase::Play && id == LEVEL_CHUNK_WITH_LIGHT.to_id(version) {
            let (x, z) = chunk_xz(payload)?;
            return Ok(Incoming::Chunk { x, z });
        }
        if self.phase == Phase::Play && id == BLOCK_UPDATE.to_id(version) {
            let (pos, state) = one_block_update(payload, version)?;
            return Ok(Incoming::BlockChange { pos, state });
        }
        if self.phase == Phase::Play && id == SECTION_BLOCKS_UPDATE.to_id(version) {
            return self.queue_multi_update(payload);
        }
        Ok(Incoming::Other { id })
    }

    const fn is_disconnect(&self, id: i32) -> bool {
        let version = self.version;
        match self.phase {
            Phase::Login => id == LOGIN_DISCONNECT.to_id(version),
            Phase::Config => id == CONFIG_DISCONNECT.to_id(version),
            Phase::Play => id == PLAY_DISCONNECT.to_id(version),
            Phase::Handshake | Phase::Status => false,
        }
    }

    fn queue_multi_update(&mut self, payload: &[u8]) -> Result<Incoming, Error> {
        let mut slice = payload;
        let packet = CMultiBlockUpdate::read(&mut slice, &self.version)
            .map_err(|err| Error::scenario(format!("multi block update: {err}")))?;
        let mut changes = packet
            .updates
            .into_iter()
            .map(|(pos, state)| Incoming::BlockChange {
                pos,
                state: i32::from(state.as_u16()),
            });
        let Some(first) = changes.next() else {
            return Ok(Incoming::Other {
                id: SECTION_BLOCKS_UPDATE.to_id(self.version),
            });
        };
        self.pending.extend(changes);
        Ok(first)
    }

    async fn send_unknown_config_packet(&mut self) -> Result<(), Error> {
        let mut body = Vec::new();
        body.write_var_int(&VarInt(UNKNOWN_CONFIG_PACKET_ID))
            .map_err(|err| Error::scenario(format!("encode unknown config packet: {err}")))?;
        let payload_len = body.len();
        self.encoder
            .write_packet(bytes::Bytes::from(body))
            .await
            .map_err(|err| Error::scenario(format!("write unknown config packet: {err}")))?;
        self.encoder
            .flush()
            .await
            .map_err(|err| Error::scenario(format!("flush unknown config packet: {err}")))?;
        self.trace.push(PacketRecord {
            direction: Direction::Out,
            phase: self.phase,
            id: UNKNOWN_CONFIG_PACKET_ID,
            payload_len,
        });
        Ok(())
    }

    async fn send_packet<P: ClientPacket>(&mut self, packet: &P) -> Result<(), Error> {
        let id = P::to_id(self.version);
        let bytes = packet
            .serialize_packet(&self.version)
            .map_err(|err| Error::scenario(format!("encode packet {id}: {err}")))?;
        let payload_len = bytes.len();
        self.encoder
            .write_packet(bytes)
            .await
            .map_err(|err| Error::scenario(format!("write packet {id}: {err}")))?;
        self.encoder
            .flush()
            .await
            .map_err(|err| Error::scenario(format!("flush packet {id}: {err}")))?;
        self.trace.push(PacketRecord {
            direction: Direction::Out,
            phase: self.phase,
            id,
            payload_len,
        });
        Ok(())
    }

    const fn next_sequence(&mut self) -> VarInt {
        let value = self.sequence;
        self.sequence = self.sequence.saturating_add(1);
        VarInt(value)
    }

    fn note_in(&mut self, id: i32, payload_len: usize) {
        self.trace.push(PacketRecord {
            direction: Direction::In,
            phase: self.phase,
            id,
            payload_len,
        });
        if self.recent.len() == 12 {
            self.recent.pop_front();
        }
        self.recent.push_back((self.phase, id));
    }

    fn timeout_message(&self) -> String {
        format!(
            "timed out in phase {:?} waiting for the next packet; recent inbound {:?}",
            self.phase, self.recent
        )
    }
}

/// Status-state ping. Returns the JSON body.
pub async fn fetch_status(addr: SocketAddr) -> Result<String, Error> {
    let stream = timeout(Duration::from_secs(10), TcpStream::connect(addr))
        .await
        .map_err(|_| Error::scenario(format!("timed out connecting to {addr} for status")))?
        .map_err(|err| Error::io("status connect", err))?;
    let (read, write) = stream.into_split();
    let mut encoder = TCPNetworkEncoder::new(write);
    let mut decoder = TCPNetworkDecoder::new(read);
    let version = CURRENT_MC_VERSION;
    let handshake = SHandShake {
        protocol_version: VarInt(version.protocol_version()),
        server_address: "127.0.0.1".into(),
        server_port: addr.port(),
        next_state: ConnectionState::Status,
    };
    write_framed(&mut encoder, &handshake, &version).await?;
    write_framed(&mut encoder, &SStatusRequest, &version).await?;
    let packet = decoder
        .get_raw_packet()
        .await
        .map_err(|err| Error::scenario(format!("status response: {err}")))?;
    if packet.id != STATUS_RESPONSE.to_id(version) {
        return Err(Error::scenario(format!(
            "status reply had packet id {}, expected {}",
            packet.id,
            STATUS_RESPONSE.to_id(version)
        )));
    }
    let mut slice = packet.payload.as_ref();
    let parsed = CStatusResponse::read(&mut slice, &version)
        .map_err(|err| Error::scenario(format!("status json: {err}")))?;
    Ok(parsed.json_response)
}

async fn write_framed<P: ClientPacket>(
    encoder: &mut TCPNetworkEncoder<tokio::net::tcp::OwnedWriteHalf>,
    packet: &P,
    version: &JavaMinecraftVersion,
) -> Result<(), Error> {
    let bytes = packet
        .serialize_packet(version)
        .map_err(|err| Error::scenario(format!("encode status packet: {err}")))?;
    encoder
        .write_packet(bytes)
        .await
        .map_err(|err| Error::scenario(format!("write status packet: {err}")))?;
    encoder
        .flush()
        .await
        .map_err(|err| Error::scenario(format!("flush status packet: {err}")))
}

const fn chunk_coords(feet: Vector3<f64>) -> (i32, i32) {
    let x = feet.x.floor() as i32;
    let z = feet.z.floor() as i32;
    (x.div_euclid(16), z.div_euclid(16))
}

fn chunk_xz(payload: &[u8]) -> Result<(i32, i32), Error> {
    let bytes: &[u8; 8] = payload
        .get(..8)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| Error::scenario("chunk packet shorter than 8 bytes"))?;
    let x = i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let z = i32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    Ok((x, z))
}

fn one_block_update(
    payload: &[u8],
    version: JavaMinecraftVersion,
) -> Result<(BlockPos, i32), Error> {
    let mut slice = payload;
    let pos = slice
        .get_block_pos(&version)
        .map_err(|err| Error::scenario(format!("block update position: {err}")))?;
    let state = slice
        .get_var_int()
        .map_err(|err| Error::scenario(format!("block update state: {err}")))?;
    Ok((pos, state.0))
}

fn payload_hint(payload: &[u8]) -> String {
    let text: String = String::from_utf8_lossy(payload)
        .chars()
        .filter(|ch| !ch.is_control())
        .take(160)
        .collect();
    if text.is_empty() {
        format!("{} bytes", payload.len())
    } else {
        text
    }
}
