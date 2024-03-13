use anyhow::Context;

use crate::tracker::InfoHash;

pub struct Handshake {
    pub name: String,
    pub info_hash: InfoHash,
    pub peer_id: [u8; 20],
}

impl Handshake {
    pub fn peer_id_string(&self) -> String {
        hex::encode(self.peer_id)
    }
}

impl From<Handshake> for Vec<u8> {
    fn from(val: Handshake) -> Self {
        let capacity = 1 + val.name.len() + 8 + val.info_hash.len() + val.peer_id.len();
        let mut buf = Vec::with_capacity(capacity);

        buf.extend(&[val.name.len() as u8]);
        buf.extend(val.name.as_bytes());
        buf.extend(&[0; 8]);
        buf.extend(val.info_hash.as_bytes());
        buf.extend(&val.peer_id);

        assert_eq!(buf.len(), capacity);

        buf
    }
}

impl TryInto<Handshake> for Vec<u8> {
    type Error = anyhow::Error;

    fn try_into(self) -> anyhow::Result<Handshake> {
        let len = self[0] as usize;

        if self.len() != len + 49 {
            anyhow::bail!("Invalid buffer");
        }

        let name_end = len + 1;

        let name = String::from_utf8(self[1..name_end].to_vec()).context("Expected UTF-8")?;

        let mut offset = name_end + 8;

        let info_hash = InfoHash(self[offset..offset + 20].to_vec().try_into().unwrap());

        offset += 20;

        let peer_id = self[offset..offset + 20].to_vec().try_into().unwrap();

        Ok(Handshake {
            name,
            info_hash,
            peer_id,
        })
    }
}
