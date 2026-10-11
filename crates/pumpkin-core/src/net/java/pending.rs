use std::{
    net::SocketAddr,
    num::NonZero,
    sync::{Arc, Weak},
};

use bytes::Bytes;
use crossbeam::atomic::AtomicCell;
use pumpkin_config::networking::compression::CompressionInfo;
use pumpkin_data::packet::CURRENT_MC_VERSION;
use pumpkin_protocol::{
    ClientPacket, ConnectionState, PacketDecodeError, RawPacket, ServerPacket,
    codec::var_int::VarInt,
    java::{
        client::config::CConfigDisconnect,
        client::login::CLoginDisconnect,
        client::play::CPlayDisconnect,
        packet_decoder::TCPNetworkDecoder,
        packet_encoder::TCPNetworkEncoder,
        server::config::{
            SAcceptCodeOfConduct, SAcknowledgeFinishConfig, SClientInformationConfig,
            SConfigCookieResponse, SConfigPong, SConfigResourcePack,
            SKeepAlive as SConfigKeepAlive, SKnownPacks, SPluginMessage,
        },
    },
    packet::MultiVersionJavaPacket,
    ser::{NetworkReadExt, NetworkWriteExt, ReadingError},
};
use pumpkin_util::{Hand, text::TextComponent, version::JavaMinecraftVersion};
use tokio::{
    io::{BufReader, BufWriter},
    net::{
        TcpStream,
        tcp::{OwnedReadHalf, OwnedWriteHalf},
    },
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, warn};

use crate::{
    entity::player::ChatMode,
    net::{
        EncryptionError, GameProfile, PacketHandlerResult, PacketRateLimiter, PlayerConfig,
        can_not_join,
    },
    plugin::server::packet::{ConnectionPacketReceivedEvent, ConnectionPacketSentEvent},
    server::Server,
};

use super::JavaClient;

const BRAND_CHANNEL_PREFIX: &str = "minecraft:brand";

/// How long a connection may stay silent before login finishes.
///
/// Once a player is in game, [`JavaClient::progress_player_packets`] keeps the
/// connection honest with keep-alives. Nothing plays that role beforehand, and
/// accepted sockets have no TCP keep-alive either, so a peer that stops talking
/// without closing would otherwise hold its descriptor for the lifetime of the
/// server. The timer covers silence rather than the whole handshake: it is reset
/// on every packet, so a slow but progressing login is never cut off.
const HANDSHAKE_IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Serverbound configuration packet, by id.
///
/// `Unknown` is every id this phase does not dispatch. That still includes
/// custom click; A-12 owns that packet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConfigPacket {
    ClientInformation,
    PluginMessage,
    AcknowledgeFinish,
    KnownPacks,
    ResourcePack,
    CookieResponse,
    Pong,
    AcceptCodeOfConduct,
    KeepAlive,
    Unknown,
}

fn classify_config_packet(id: i32, version: JavaMinecraftVersion) -> ConfigPacket {
    if id == SClientInformationConfig::to_id(version) {
        ConfigPacket::ClientInformation
    } else if id == SPluginMessage::to_id(version) {
        ConfigPacket::PluginMessage
    } else if id == SAcknowledgeFinishConfig::to_id(version) {
        ConfigPacket::AcknowledgeFinish
    } else if id == SKnownPacks::to_id(version) {
        ConfigPacket::KnownPacks
    } else if id == SConfigResourcePack::to_id(version) {
        ConfigPacket::ResourcePack
    } else if id == SConfigCookieResponse::to_id(version) {
        ConfigPacket::CookieResponse
    } else if id == SConfigPong::to_id(version) {
        ConfigPacket::Pong
    } else if id == SAcceptCodeOfConduct::to_id(version) {
        ConfigPacket::AcceptCodeOfConduct
    } else if id == SConfigKeepAlive::to_id(version) {
        ConfigPacket::KeepAlive
    } else {
        ConfigPacket::Unknown
    }
}

fn unknown_config_packet(id: i32) -> ReadingError {
    ReadingError::Message(format!("Failed to handle packet id {id} in Config State"))
}

pub struct PendingConnection {
    pub id: u64,
    pub address: SocketAddr,
    pub server_address: String,
    pub version: AtomicCell<JavaMinecraftVersion>,
    pub connection_state: AtomicCell<ConnectionState>,
    pub close_token: CancellationToken,
    pub network_writer: TCPNetworkEncoder<BufWriter<OwnedWriteHalf>>,
    pub network_reader: TCPNetworkDecoder<BufReader<OwnedReadHalf>>,
    pub gameprofile: Option<GameProfile>,
    pub config: Option<PlayerConfig>,
    pub brand: Option<String>,
    pub packet_limiter: PacketRateLimiter,
    pub verify_token: Option<[u8; 4]>,
    pub vine_challenge: Option<[u8; 16]>,
    /// Whether a login was started. Used to reject a second `SLoginStart`, which
    /// would otherwise replace an already authenticated profile.
    pub login_started: bool,
    /// Whether `CLoginSuccess` has been sent, mirroring vanilla's
    /// `State::PROTOCOL_SWITCHING`. Login acknowledgement is only valid after it.
    pub login_success_sent: bool,
    /// For the connection packet events.
    server: Weak<Server>,
}

impl PendingConnection {
    #[must_use]
    pub fn new(
        tcp_stream: TcpStream,
        address: SocketAddr,
        id: u64,
        packet_limiter: PacketRateLimiter,
        server: Weak<Server>,
    ) -> Self {
        let (read, write) = tcp_stream.into_split();
        Self {
            id,
            address,
            server_address: String::new(),
            version: AtomicCell::new(CURRENT_MC_VERSION),
            connection_state: AtomicCell::new(ConnectionState::HandShake),
            close_token: CancellationToken::new(),
            network_writer: TCPNetworkEncoder::new(BufWriter::new(write)),
            network_reader: TCPNetworkDecoder::new(BufReader::new(read)),
            gameprofile: None,
            config: None,
            brand: None,
            packet_limiter,
            login_started: false,
            login_success_sent: false,
            verify_token: None,
            vine_challenge: None,
            server,
        }
    }

    pub fn close(&self) {
        self.close_token.cancel();
    }

    pub fn is_closed(&self) -> bool {
        self.close_token.is_cancelled()
    }

    pub async fn await_close_interrupt(&self) {
        self.close_token.cancelled().await;
    }

    pub fn set_encryption(&mut self, shared_secret: &[u8]) -> Result<(), EncryptionError> {
        let crypt_key: [u8; 16] = shared_secret
            .try_into()
            .map_err(|_| EncryptionError::SharedWrongLength)?;
        self.network_reader
            .set_encryption(&crypt_key)
            .map_err(|_| EncryptionError::AlreadyEncrypted)?;
        self.network_writer
            .set_encryption(&crypt_key)
            .map_err(|_| EncryptionError::AlreadyEncrypted)?;
        Ok(())
    }

    pub fn set_compression(&mut self, compression: &CompressionInfo) {
        if compression.level > 9 {
            error!("Invalid compression level! Clients will not be able to read this!");
        }

        self.network_reader
            .set_compression(compression.threshold as usize);

        self.network_writer
            .set_compression((compression.threshold as usize, compression.level));
    }

    pub async fn get_packet(&mut self) -> Option<RawPacket> {
        let close_token = self.close_token.clone();
        let packet_result = tokio::select! {
            () = close_token.cancelled() => {
                debug!("Canceling pending connection packet processing");
                return None;
            },
            () = tokio::time::sleep(HANDSHAKE_IDLE_TIMEOUT) => {
                debug!(
                    "Client {} sent nothing for {}s before finishing login, dropping it",
                    self.id,
                    HANDSHAKE_IDLE_TIMEOUT.as_secs()
                );
                return None;
            },
            res = self.network_reader.get_raw_packet() => res,
        };

        match packet_result {
            Ok(packet) => Some(packet),
            Err(err) => {
                if !matches!(err, PacketDecodeError::ConnectionClosed) {
                    debug!("Failed to decode packet from client {}: {}", self.id, err);
                    let text = format!("Error while reading incoming packet {err}");
                    self.kick(TextComponent::text(text)).await;
                }
                None
            }
        }
    }

    /// Server for the connection packet events, only for clients below 26.3 after handshake.
    fn translating_server(&self) -> Option<Arc<Server>> {
        if self.version.load() == CURRENT_MC_VERSION
            || self.connection_state.load() == ConnectionState::HandShake
        {
            return None;
        }
        self.server.upgrade()
    }

    /// Encoded as 26.3. `ConnectionPacketSentEvent` can rewrite it.
    pub async fn send_packet_now<P: ClientPacket>(&mut self, packet: &P) {
        let mut packet_buf = Vec::new();
        if let Err(err) =
            JavaClient::write_packet_for_version(packet, CURRENT_MC_VERSION, &mut packet_buf)
        {
            error!("Failed to write packet: {err:?}");
            return;
        }
        let Some(payload) = self.translate_outgoing(Bytes::from(packet_buf)).await else {
            return;
        };
        if let Err(err) = self.network_writer.write_packet(payload).await {
            warn!("Failed to send packet to client {}: {}", self.id, err);
        }
        let _ = self.network_writer.flush().await;
    }

    /// `ConnectionPacketSentEvent` with the 26.3 packet. `None` when cancelled.
    async fn translate_outgoing(&self, packet_data: Bytes) -> Option<Bytes> {
        let Some(server) = self.translating_server() else {
            return Some(packet_data);
        };
        if !server
            .plugin_manager
            .has_handlers::<ConnectionPacketSentEvent>()
        {
            return Some(packet_data);
        }

        let mut reader = &packet_data[..];
        let Ok(packet_id) = reader.get_var_int() else {
            return Some(packet_data);
        };
        let payload = packet_data.slice(packet_data.len() - reader.len()..);
        let mut event = ConnectionPacketSentEvent::new(
            self.id,
            self.version.load(),
            self.connection_state.load(),
            packet_id.0,
            payload,
        );
        server.plugin_manager.fire(&server, &mut event).await;
        if event.cancelled {
            return None;
        }

        let mut framed = Vec::with_capacity(5 + event.payload.len());
        framed.write_var_int(&VarInt(event.packet_id)).ok()?;
        framed.extend_from_slice(&event.payload);
        Some(framed.into())
    }

    /// `ConnectionPacketReceivedEvent` with the client's packet; handlers rewrite it to 26.3.
    /// `None` when cancelled.
    async fn translate_incoming(&self, packet: &RawPacket) -> Option<RawPacket> {
        let unchanged = || RawPacket {
            id: packet.id,
            payload: packet.payload.clone(),
        };
        let Some(server) = self.translating_server() else {
            return Some(unchanged());
        };
        if !server
            .plugin_manager
            .has_handlers::<ConnectionPacketReceivedEvent>()
        {
            return Some(unchanged());
        }

        let mut event = ConnectionPacketReceivedEvent::new(
            self.id,
            self.version.load(),
            self.connection_state.load(),
            packet.id,
            packet.payload.clone(),
        );
        server.plugin_manager.fire(&server, &mut event).await;
        (!event.cancelled).then(|| RawPacket {
            id: event.packet_id,
            payload: event.payload,
        })
    }

    pub async fn kick(&mut self, reason: TextComponent) {
        match self.connection_state.load() {
            ConnectionState::Login => {
                self.send_packet_now(&CLoginDisconnect::new(
                    serde_json::to_string(&reason.0).unwrap_or_else(|_| String::new()),
                ))
                .await;
            }
            ConnectionState::Config => {
                self.send_packet_now(&CConfigDisconnect::new(&reason.get_text()))
                    .await;
            }
            ConnectionState::Play => {
                self.send_packet_now(&CPlayDisconnect::new(&reason)).await;
            }
            _ => {}
        }
        debug!("Closing connection for {}", self.id);
        self.close();
    }

    pub async fn handle_login_sequence(&mut self, server: &Arc<Server>) -> PacketHandlerResult {
        while let Some(packet) = self.get_packet().await {
            if !self.packet_limiter.check_packet() {
                warn!(
                    "Pending client {} exceeded packet rate limit (rate: {}/s)",
                    self.id,
                    self.packet_limiter.max_rate()
                );
                self.kick(TextComponent::text(
                    server
                        .advanced_config
                        .networking
                        .java
                        .packet_limiter
                        .kick_message
                        .clone(),
                ))
                .await;
                return PacketHandlerResult::Stop;
            }

            match self.handle_packet(server, &packet).await {
                Ok(result) => {
                    if let Some(result) = result {
                        return result;
                    }
                }
                Err(error) => {
                    let text = format!("Error while reading incoming packet {error}");
                    debug!(
                        "Failed to read incoming packet with id {}: {}",
                        packet.id, error
                    );
                    self.kick(TextComponent::text(text)).await;
                }
            }
        }
        PacketHandlerResult::Stop
    }

    pub async fn handle_packet(
        &mut self,
        server: &Arc<Server>,
        packet: &RawPacket,
    ) -> Result<Option<PacketHandlerResult>, ReadingError> {
        let Some(packet) = self.translate_incoming(packet).await else {
            return Ok(None);
        };
        let packet = &packet;
        match self.connection_state.load() {
            ConnectionState::HandShake => self.handle_handshake_packet(server, packet).await,
            ConnectionState::Status => self.handle_status_packet(server, packet).await,
            ConnectionState::Login | ConnectionState::Transfer => {
                self.handle_login_packet(server, packet).await
            }
            ConnectionState::Config => self.handle_config_packet(server, packet).await,
            ConnectionState::Play => Ok(None),
        }
    }

    async fn handle_handshake_packet(
        &mut self,
        server: &Arc<Server>,
        packet: &RawPacket,
    ) -> Result<Option<PacketHandlerResult>, ReadingError> {
        debug!("Handling handshake group");
        let mut payload = &packet.payload[..];
        match packet.id {
            0 => {
                self.handle_handshake(
                    server,
                    pumpkin_protocol::java::server::handshake::SHandShake::read(
                        &mut payload,
                        &CURRENT_MC_VERSION,
                    )?,
                )
                .await;
                Ok(None)
            }
            _ => Err(ReadingError::Message(format!(
                "Failed to handle packet id {} in Handshake State",
                packet.id
            ))),
        }
    }

    async fn handle_status_packet(
        &mut self,
        server: &Arc<Server>,
        packet: &RawPacket,
    ) -> Result<Option<PacketHandlerResult>, ReadingError> {
        debug!("Handling status group");
        let mut payload = &packet.payload[..];
        let version = CURRENT_MC_VERSION;

        match packet.id {
            id if id == pumpkin_protocol::java::server::status::SStatusRequest::to_id(version) => {
                self.handle_status_request(server).await;
                Ok(None)
            }
            id if id
                == pumpkin_protocol::java::server::status::SStatusPingRequest::to_id(version) =>
            {
                self.handle_ping_request(
                    pumpkin_protocol::java::server::status::SStatusPingRequest::read(
                        &mut payload,
                        &version,
                    )?,
                )
                .await;
                Ok(None)
            }
            _ => Err(ReadingError::Message(format!(
                "Failed to handle java client packet id {} in Status State",
                packet.id
            ))),
        }
    }

    async fn handle_login_packet(
        &mut self,
        server: &Arc<Server>,
        packet: &RawPacket,
    ) -> Result<Option<PacketHandlerResult>, ReadingError> {
        debug!("Handling login group");
        let mut payload = &packet.payload[..];
        let version = CURRENT_MC_VERSION;

        match packet.id {
            id if id == pumpkin_protocol::java::server::login::SLoginStart::to_id(version) => {
                Ok(self
                    .handle_login_start(
                        server,
                        pumpkin_protocol::java::server::login::SLoginStart::read(
                            &mut payload,
                            &version,
                        )?,
                    )
                    .await)
            }
            id if id
                == pumpkin_protocol::java::server::login::SEncryptionResponse::to_id(version) =>
            {
                Ok(self
                    .handle_encryption_response(
                        server,
                        pumpkin_protocol::java::server::login::SEncryptionResponse::read(
                            &mut payload,
                            &version,
                        )?,
                    )
                    .await)
            }
            id if id
                == pumpkin_protocol::java::server::login::SLoginPluginResponse::to_id(version) =>
            {
                Ok(self
                    .handle_plugin_response(
                        server,
                        pumpkin_protocol::java::server::login::SLoginPluginResponse::read(
                            &mut payload,
                            &version,
                        )?,
                    )
                    .await)
            }
            id if id
                == pumpkin_protocol::java::server::login::SLoginCookieResponse::to_id(version) =>
            {
                self.handle_login_cookie_response(
                    &pumpkin_protocol::java::server::login::SLoginCookieResponse::read(
                        &mut payload,
                        &version,
                    )?,
                );
                Ok(None)
            }
            id if id
                == pumpkin_protocol::java::server::login::SLoginAcknowledged::to_id(version) =>
            {
                Ok(self.handle_login_acknowledged(server).await)
            }
            _ => Err(ReadingError::Message(format!(
                "Failed to handle packet id {} in Login State",
                packet.id
            ))),
        }
    }

    async fn handle_config_packet(
        &mut self,
        server: &Arc<Server>,
        packet: &RawPacket,
    ) -> Result<Option<PacketHandlerResult>, ReadingError> {
        debug!("Handling config group");
        let mut payload = &packet.payload[..];
        let version = CURRENT_MC_VERSION;

        match classify_config_packet(packet.id, version) {
            ConfigPacket::ClientInformation => {
                self.handle_client_information_config(SClientInformationConfig::read(
                    &mut payload,
                    &version,
                )?)
                .await;
                Ok(None)
            }
            ConfigPacket::PluginMessage => {
                self.handle_plugin_message(SPluginMessage::read(&mut payload, &version)?)
                    .await;
                Ok(None)
            }
            ConfigPacket::AcknowledgeFinish => {
                let Some(profile) = self.gameprofile.clone() else {
                    return Ok(Some(PacketHandlerResult::Stop));
                };
                let config = self.config.clone().unwrap_or_default();
                self.connection_state.store(ConnectionState::Play);
                if let Some(reason) = can_not_join(&profile, &self.address, server).await {
                    self.kick(reason).await;
                    Ok(Some(PacketHandlerResult::Stop))
                } else {
                    Ok(Some(PacketHandlerResult::ReadyToPlay(profile, config)))
                }
            }
            ConfigPacket::KnownPacks => {
                self.handle_known_packs(server).await;
                Ok(None)
            }
            ConfigPacket::ResourcePack => {
                self.handle_resource_pack_response(
                    server,
                    SConfigResourcePack::read(&mut payload, &version)?,
                )
                .await;
                Ok(None)
            }
            ConfigPacket::CookieResponse => {
                self.handle_config_cookie_response(&SConfigCookieResponse::read(
                    &mut payload,
                    &version,
                )?);
                Ok(None)
            }
            ConfigPacket::Pong => {
                let _pong = SConfigPong::read(&mut payload, &version)?;
                Ok(None)
            }
            ConfigPacket::AcceptCodeOfConduct => {
                let _accept = SAcceptCodeOfConduct::read(&mut payload, &version)?;
                Ok(None)
            }
            // Play answers this id in `JavaClient::handle_keep_alive` and does not
            // kick. There is no player yet, so there is no ping to store. The
            // connection stays in configuration.
            ConfigPacket::KeepAlive => {
                self.handle_config_keep_alive(&SConfigKeepAlive::read(&mut payload, &version)?);
                Ok(None)
            }
            ConfigPacket::Unknown => Err(unknown_config_packet(packet.id)),
        }
    }

    /// Serverbound keep-alive during configuration.
    ///
    /// Play matches an id the server sent and ignores any other id. Configuration
    /// does not send a challenge. Either way the packet is consumed and the
    /// client is not kicked.
    pub fn handle_config_keep_alive(&self, keep_alive: &SConfigKeepAlive) {
        debug!(
            "Configuration keep alive {} from {}",
            keep_alive.keep_alive_id, self.address
        );
    }

    pub async fn handle_client_information_config(
        &mut self,
        client_information: SClientInformationConfig<'_>,
    ) {
        debug!("Handling client settings");
        if client_information.view_distance <= 0 {
            self.kick(TextComponent::text(
                "Cannot have zero or negative view distance!",
            ))
            .await;
            return;
        }

        if let (Ok(main_hand), Ok(chat_mode)) = (
            Hand::try_from(client_information.main_hand.0),
            ChatMode::try_from(client_information.chat_mode.0),
        ) {
            self.config = Some(PlayerConfig {
                locale: client_information.locale.to_string(),
                view_distance: NonZero::new(client_information.view_distance as u8)
                    .unwrap_or(NonZero::<u8>::MIN),
                chat_mode,
                chat_colors: client_information.chat_colors,
                skin_parts: client_information.skin_parts,
                main_hand,
                text_filtering: client_information.text_filtering,
                server_listing: client_information.server_listing,
            });
        } else {
            self.kick(TextComponent::text("Invalid hand or chat type"))
                .await;
        }
    }

    pub async fn handle_plugin_message(&mut self, plugin_message: SPluginMessage<'_>) {
        debug!("Handling plugin message");
        if plugin_message.channel.starts_with(BRAND_CHANNEL_PREFIX) {
            debug!("Got a client brand");
            match core::str::from_utf8(plugin_message.data) {
                Ok(brand) => self.brand = Some(brand.to_string()),
                Err(e) => self.kick(TextComponent::text(e.to_string())).await,
            }
        }
    }

    pub async fn handle_resource_pack_response(
        &mut self,
        server: &Server,
        packet: SConfigResourcePack,
    ) {
        let resource_config = &server.advanced_config.resource_pack.java;
        if resource_config.enabled {
            use pumpkin_protocol::java::server::config::ResourcePackResponseResult;
            match packet.response_result() {
                ResourcePackResponseResult::Downloaded
                | ResourcePackResponseResult::DownloadSuccess
                | ResourcePackResponseResult::Discarded
                | ResourcePackResponseResult::Unknown(_) => {
                    self.send_known_packs(server).await;
                }
                ResourcePackResponseResult::Accepted => {}
                ResourcePackResponseResult::Declined => {
                    if resource_config.force {
                        self.kick(TextComponent::text("Required resource pack was declined"))
                            .await;
                    } else {
                        self.send_known_packs(server).await;
                    }
                }
                ResourcePackResponseResult::DownloadFail => {
                    if resource_config.force {
                        self.kick(TextComponent::text("Failed to download resource pack"))
                            .await;
                    } else {
                        self.send_known_packs(server).await;
                    }
                }
                ResourcePackResponseResult::InvalidUrl => {
                    self.kick(TextComponent::text("Invalid resource pack URL"))
                        .await;
                }
                ResourcePackResponseResult::ReloadFailed => {
                    self.kick(TextComponent::text("Failed to reload resource pack"))
                        .await;
                }
            }
        }
    }

    pub fn handle_config_cookie_response(&self, packet: &SConfigCookieResponse<'_>) {
        debug!(
            "Received cookie_response[config]: key: \"{}\", payload_length: \"{:?}\"",
            packet.key,
            packet.payload.as_ref().map(|p| p.len())
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuration_keep_alive_is_dispatched_and_unrelated_id_is_rejected() {
        let version = CURRENT_MC_VERSION;
        assert_eq!(
            classify_config_packet(SConfigKeepAlive::to_id(version), version),
            ConfigPacket::KeepAlive
        );
        assert_eq!(classify_config_packet(127, version), ConfigPacket::Unknown);
        assert!(matches!(
            unknown_config_packet(127),
            ReadingError::Message(message) if message == "Failed to handle packet id 127 in Config State"
        ));

        for id in [
            SClientInformationConfig::to_id(version),
            SPluginMessage::to_id(version),
            SAcknowledgeFinishConfig::to_id(version),
            SKnownPacks::to_id(version),
            SConfigResourcePack::to_id(version),
            SConfigCookieResponse::to_id(version),
            SConfigPong::to_id(version),
            SAcceptCodeOfConduct::to_id(version),
        ] {
            assert_ne!(
                classify_config_packet(id, version),
                ConfigPacket::Unknown,
                "configuration id {id} used to be dispatched"
            );
        }
    }

    #[tokio::test]
    async fn configuration_keep_alive_stays_in_configuration() {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let client = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (server, peer) = listener.accept().await.unwrap();
        let pending = PendingConnection::new(
            server,
            peer,
            1,
            crate::net::PacketRateLimiter::new(false, 1.0, 1.0),
            std::sync::Weak::new(),
        );
        pending.connection_state.store(ConnectionState::Config);

        pending.handle_config_keep_alive(&SConfigKeepAlive { keep_alive_id: 1 });

        assert_eq!(pending.connection_state.load(), ConnectionState::Config);
        assert!(!pending.close_token.is_cancelled());
        drop(client);
    }
}
