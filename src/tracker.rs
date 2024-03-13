use std::{
    fmt,
    net::{Ipv4Addr, SocketAddrV4},
};

use serde::{de::Visitor, Deserialize, Deserializer, Serialize, Serializer};

#[derive(Serialize)]
pub struct Tracker {
    pub info_hash: InfoHash,
    pub peer_id: String,
    pub port: u16,
    pub uploaded: usize,
    pub downloaded: usize,
    pub left: usize,
    pub compact: usize,
}

pub struct InfoHash(pub [u8; 20]);

impl InfoHash {
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_slice()
    }
}

impl From<[u8; 20]> for InfoHash {
    fn from(value: [u8; 20]) -> Self {
        Self(value)
    }
}

impl Serialize for InfoHash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // I hate myself
        let s = unsafe { String::from_utf8_unchecked(self.0.to_vec()) };
        serializer.serialize_str(&s)
    }
}

#[derive(Deserialize)]
pub struct TrackerResponse {
    pub peers: Peers,
}

pub struct Peers(pub Vec<SocketAddrV4>);

impl<'de> Deserialize<'de> for Peers {
    fn deserialize<D>(deserializer: D) -> Result<Peers, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_bytes(PeersVisitor)
    }
}

struct PeersVisitor;

impl<'de> Visitor<'de> for PeersVisitor {
    type Value = Peers;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("expecting a sequency of bytes % 6")
    }

    fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        if v.len() % 6 != 0 {
            return Err(E::custom(""));
        }

        let peers = v
            .chunks_exact(6)
            .map(|chunk| {
                SocketAddrV4::new(
                    Ipv4Addr::new(chunk[0], chunk[1], chunk[2], chunk[3]),
                    u16::from_be_bytes([chunk[4], chunk[5]]),
                )
            })
            .collect();

        Ok(Peers(peers))
    }
}
