use std::{fmt, fs::read, path::PathBuf};

use anyhow::Context;
use serde::{de::Visitor, Deserialize, Deserializer, Serialize, Serializer};

use sha1::{Digest, Sha1};

#[derive(Deserialize, Serialize, Debug)]
pub struct TorrentFile {
    pub announce: String,
    pub info: Info,
}

impl TryFrom<PathBuf> for TorrentFile {
    type Error = anyhow::Error;

    fn try_from(value: PathBuf) -> anyhow::Result<Self> {
        let content =
            read(&value).with_context(|| format!("file {} does not exists", value.display()))?;

        serde_bencode::from_bytes(&content).context("not a valid meta file")
    }
}

impl TorrentFile {
    pub fn info_hash(&self) -> anyhow::Result<[u8; 20]> {
        let meta_encoded = serde_bencode::to_bytes(&self.info).context("encode meta info")?;

        let mut hasher = Sha1::new();
        hasher.update(&meta_encoded);
        Ok(hasher.finalize().into())
    }
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Info {
    pub length: usize,
    name: String,
    #[serde(rename = "piece length")]
    pub piece_length: usize,
    #[allow(dead_code)]
    pub pieces: Pieces,
}

#[derive(Debug)]
pub struct Pieces(pub Vec<[u8; 20]>);

impl<'de> Deserialize<'de> for Pieces {
    fn deserialize<D>(deserializer: D) -> Result<Pieces, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_bytes(PiecesVisitor)
    }
}

struct PiecesVisitor;

impl<'de> Visitor<'de> for PiecesVisitor {
    type Value = Pieces;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("expecting a sequency of bytes % 20")
    }

    fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        if v.len() % 20 != 0 {
            return Err(E::custom(""));
        }

        let peers = v
            .chunks_exact(20)
            .map(|chuck| {
                let mut buf = [0u8; 20];

                for (idx, &x) in chuck.iter().enumerate() {
                    buf[idx] = x;
                }

                buf
            })
            .collect();

        Ok(Pieces(peers))
    }
}

impl Serialize for Pieces {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let b = self.0.concat();
        serializer.serialize_bytes(&b)
    }
}
