use crate::{
    ServerPacket,
    ser::{NetworkReadExt, NetworkReadSliceExt, ReadingError},
};
use pumpkin_data::packet::serverbound::play::CLIENT_INFORMATION;
use pumpkin_macros::java_packet;
use pumpkin_util::version::JavaMinecraftVersion;

use crate::VarInt;
use crate::java::server::config::PARTICLE_STATUS_ALL;

#[java_packet(CLIENT_INFORMATION)]
pub struct SClientInformationPlay<'a> {
    pub locale: &'a str, // 16
    pub view_distance: i8,
    pub chat_mode: VarInt, // VarInt
    pub chat_colors: bool,
    pub skin_parts: u8,
    pub main_hand: VarInt,
    pub text_filtering: bool,
    pub server_listing: bool,
    /// Particle setting (`0` all, `1` decreased, `2` minimal), present from 1.21.2.
    pub particle_status: VarInt,
}

impl<'a> ServerPacket<'a> for SClientInformationPlay<'a> {
    fn read(bytebuf: &mut &'a [u8], version: &JavaMinecraftVersion) -> Result<Self, ReadingError> {
        let locale = bytebuf.get_str_borrowed()?;
        let view_distance = bytebuf.get_i8()?;
        let chat_mode = bytebuf.get_var_int()?;
        let chat_colors = bytebuf.get_bool()?;
        let skin_parts = bytebuf.get_u8()?;
        let main_hand = if version >= &JavaMinecraftVersion::V_1_9 {
            bytebuf.get_var_int()?
        } else {
            VarInt(1)
        };
        let text_filtering = if version >= &JavaMinecraftVersion::V_1_17 {
            bytebuf.get_bool()?
        } else {
            false
        };
        let server_listing = if version >= &JavaMinecraftVersion::V_1_18 {
            bytebuf.get_bool()?
        } else {
            true
        };
        let particle_status = if version >= &JavaMinecraftVersion::V_1_21_2 {
            bytebuf.get_var_int()?
        } else {
            VarInt(PARTICLE_STATUS_ALL)
        };

        Ok(Self {
            locale,
            view_distance,
            chat_mode,
            chat_colors,
            skin_parts,
            main_hand,
            text_filtering,
            server_listing,
            particle_status,
        })
    }
}

impl crate::ClientPacket for SClientInformationPlay<'_> {
    fn write_packet_data(
        &self,
        mut write: impl std::io::Write,
        version: &JavaMinecraftVersion,
    ) -> Result<(), crate::ser::WritingError> {
        use crate::ser::NetworkWriteExt;
        write.write_string(self.locale)?;
        write.write_i8(self.view_distance)?;
        write.write_var_int(&self.chat_mode)?;
        write.write_bool(self.chat_colors)?;
        write.write_u8(self.skin_parts)?;
        if version >= &JavaMinecraftVersion::V_1_9 {
            write.write_var_int(&self.main_hand)?;
        }
        if version >= &JavaMinecraftVersion::V_1_17 {
            write.write_bool(self.text_filtering)?;
        }
        if version >= &JavaMinecraftVersion::V_1_18 {
            write.write_bool(self.server_listing)?;
        }
        if version >= &JavaMinecraftVersion::V_1_21_2 {
            write.write_var_int(&self.particle_status)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ClientPacket;
    use crate::java::server::config::PARTICLE_STATUS_ALL;

    /// `en_us`, view distance 8, chat enabled, colors on, skin mask `0x7f`,
    /// right hand, no text filtering, listed. No particle-status byte.
    const WITHOUT_PARTICLE_STATUS: &[u8] = &[
        0x05, b'e', b'n', b'_', b'u', b's', 0x08, 0x00, 0x01, 0x7f, 0x01, 0x00, 0x01,
    ];

    fn sample(status: i32) -> SClientInformationPlay<'static> {
        SClientInformationPlay {
            locale: "en_us",
            view_distance: 8,
            chat_mode: VarInt(0),
            chat_colors: true,
            skin_parts: 0x7f,
            main_hand: VarInt(1),
            text_filtering: false,
            server_listing: true,
            particle_status: VarInt(status),
        }
    }

    #[test]
    fn particle_status_round_trip() {
        let version = JavaMinecraftVersion::V_26_3;
        for status in [0, 1, 2] {
            let mut written = Vec::new();
            sample(status)
                .write_packet_data(&mut written, &version)
                .unwrap();

            let mut expected = WITHOUT_PARTICLE_STATUS.to_vec();
            expected.push(status as u8);
            assert_eq!(written, expected);

            let mut slice = written.as_slice();
            let read = SClientInformationPlay::read(&mut slice, &version).unwrap();
            assert!(slice.is_empty());
            assert_eq!(read.particle_status.0, status);
            assert_eq!(read.locale, "en_us");
            assert!(read.server_listing);
        }
    }

    #[test]
    fn particle_status_required_on_26_3() {
        let version = JavaMinecraftVersion::V_26_3;
        let mut slice = WITHOUT_PARTICLE_STATUS;
        assert!(SClientInformationPlay::read(&mut slice, &version).is_err());
    }

    #[test]
    fn particle_status_absent_before_1_21_2() {
        let version = JavaMinecraftVersion::V_1_21;
        let mut written = Vec::new();
        sample(2).write_packet_data(&mut written, &version).unwrap();
        assert_eq!(written, WITHOUT_PARTICLE_STATUS);

        let mut slice = written.as_slice();
        let read = SClientInformationPlay::read(&mut slice, &version).unwrap();
        assert!(slice.is_empty());
        assert_eq!(read.particle_status.0, PARTICLE_STATUS_ALL);
    }
}
