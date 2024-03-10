use std::{
    fs::read,
    io::{BufRead, BufReader},
    path::PathBuf,
};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use serde_json::Map;

use clap::{Parser, Subcommand};
use sha1::{Digest, Sha1};

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Decode { encoded_value: String },
    Info { file_path: PathBuf },
    Peers { file_path: PathBuf },
}

// Available if you need it!
// use serde_bencode

fn decode_bencoded_value(reader: &mut dyn BufRead) -> anyhow::Result<serde_json::Value> {
    // If encoded_value starts with a digit, it's a number

    let mut header_buff = [0; 1];
    let _ = reader.read_exact(&mut header_buff);

    Ok(match header_buff[0] {
        ch if ch.is_ascii_digit() => {
            let mut buf = Vec::new();
            let _ = reader
                .read_until(b':', &mut buf)
                .context("not a valid string")?;

            buf.insert(0, ch);

            let size: usize = std::str::from_utf8(&buf[..buf.len() - 1])
                .context("the size is not a valid UTF-8")?
                .parse()
                .context("the size is not a number")?;

            buf.resize(size, 0);

            let buf = &mut buf[..size];

            reader
                .read_exact(buf)
                .context("not possible to read the string")?;

            if let Ok(text) = std::str::from_utf8(buf) {
                serde_json::Value::String(text.to_string())
            } else {
                serde_json::Value::Array(
                    buf.iter()
                        .map(|&x| serde_json::Value::Number(x.into()))
                        .collect(),
                )
            }
        }
        b'i' => {
            let mut buf = Vec::new();

            let _ = reader
                .read_until(b'e', &mut buf)
                .context("not a valid integer")?;

            let num: i64 = std::str::from_utf8(&buf[..buf.len() - 1])
                .context("the integer is not a valid UTF-8")?
                .parse()
                .context("it's not an integer")?;

            serde_json::Value::Number(num.into())
        }
        b'l' => {
            let mut list: Vec<serde_json::Value> = Vec::new();

            while let Ok(value) = decode_bencoded_value(reader) {
                list.push(value);
            }

            list.into()
        }
        b'd' => {
            let mut list: Map<String, serde_json::Value> = Map::new();

            while let Ok(serde_json::Value::String(value)) = decode_bencoded_value(reader) {
                list.insert(value, decode_bencoded_value(reader)?);
            }

            serde_json::Value::Object(list)
        }
        _ => {
            anyhow::bail!("This is not a valid bencode value")
        }
    })
}

#[derive(Deserialize, Serialize)]
struct MetaFile {
    announce: String,
    info: MetaFileInfo,
}

#[derive(Deserialize, Serialize)]
struct MetaFileInfo {
    length: usize,
    #[allow(dead_code)]
    name: String,
    #[serde(rename = "piece length")]
    #[allow(dead_code)]
    piece_length: usize,
    #[allow(dead_code)]
    pieces: ByteBuf,
}

#[derive(Serialize)]
struct Tracker {
    info_hash: String,
    peer_id: String,
    port: u16,
    uploaded: usize,
    downloaded: usize,
    left: usize,
    compact: usize,
}

#[derive(Deserialize)]
struct TrackerResponse {
    peers: Vec<Peer>,
}

#[derive(Deserialize)]
struct Peer {
    port: u16,
    ip: String,
}

// Usage: your_bittorrent.sh decode "<encoded_value>"
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Commands::Decode { encoded_value } => {
            let mut reader = BufReader::new(encoded_value.as_bytes());
            let decoded_value = decode_bencoded_value(&mut reader)
                .with_context(|| format!("was not possible to decode: {}", encoded_value))?;
            println!("{}", decoded_value);
        }
        Commands::Info { file_path } => {
            let content = read(&file_path)
                .with_context(|| format!("file {} does not exists", file_path.display()))?;

            let meta: MetaFile =
                serde_bencode::from_bytes(&content).context("not a valid meta file")?;
            let meta_encoded = serde_bencode::to_bytes(&meta.info).context("encode meta info")?;

            let mut hasher = Sha1::new();
            hasher.update(&meta_encoded);
            let hash = hasher.finalize();

            println!("Tracker URL: {}", meta.announce,);
            println!("Length: {}", meta.info.length);
            println!("Info Hash: {}", hex::encode(hash));
            println!("Piece Length: {}", meta.info.piece_length);
            println!("Piece Hashes:");

            for hash in meta.info.pieces.chunks_exact(20) {
                println!("{}", hex::encode(hash));
            }
        }
        Commands::Peers { file_path } => {
            let content = read(&file_path)
                .with_context(|| format!("file {} does not exists", file_path.display()))?;

            let meta: MetaFile =
                serde_bencode::from_bytes(&content).context("not a valid meta file")?;
            let meta_encoded = serde_bencode::to_bytes(&meta.info).context("encode meta info")?;

            let mut hasher = Sha1::new();
            hasher.update(&meta_encoded);
            let hash = hasher.finalize().to_vec();

            let tracker = unsafe {
                Tracker {
                    info_hash: String::from_utf8_unchecked(hash),
                    peer_id: "00112233445566778899".to_owned(),
                    port: 6881,
                    uploaded: 0,
                    downloaded: 0,
                    left: meta.info.length,
                    compact: 0,
                }
            };

            let client = reqwest::Client::new();

            let response = client
                .get(meta.announce)
                .query(&tracker)
                .send()
                .await
                .context("failed to do the request")?;

            let response = response
                .bytes()
                .await
                .context("faild to parse the response")?;

            let response: TrackerResponse =
                serde_bencode::from_bytes(&response).context("faild to parse the response")?;

            for peer in response.peers {
                println!("{}:{}", peer.ip, peer.port);
            }
        }
    };

    Ok(())
}
