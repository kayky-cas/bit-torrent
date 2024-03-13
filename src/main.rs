mod handshake;
mod message;
mod torrent_file;
mod tracker;

use std::{
    fs::read,
    io::{BufRead, BufReader},
    path::PathBuf,
};

use anyhow::Context;

use clap::{Parser, Subcommand};
use serde_json::Map;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{unix::SocketAddr, TcpStream, ToSocketAddrs},
};
use tracker::InfoHash;

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Decode {
        encoded_value: String,
    },
    Info {
        file_path: PathBuf,
    },
    Peers {
        file_path: PathBuf,
    },
    Handshake {
        file_path: PathBuf,
        ip_addr: String,
    },
    #[command(name = "download_piece")]
    DownloadPiece {
        #[arg(short)]
        output_file_path: PathBuf,
        file_path: PathBuf,
        piece: usize,
    },
}

// Available if you need it!
// use serde_bencode

fn decode_bencoded_value(reader: &mut dyn BufRead) -> anyhow::Result<serde_json::Value> {
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

async fn get_tracker_response(
    url: &str,
    tracker: tracker::Tracker,
) -> anyhow::Result<tracker::TrackerResponse> {
    let client = reqwest::Client::new();

    let response = client
        .get(url)
        .query(&tracker)
        .send()
        .await
        .context("failed to do the request")?;

    let response = response
        .bytes()
        .await
        .context("faild to parse the response")?;

    serde_bencode::from_bytes(&response).context("faild to parse the response")
}

async fn handshake<A: ToSocketAddrs>(
    ip_addr: A,
    info_hash: InfoHash,
) -> anyhow::Result<handshake::Handshake> {
    let mut client = TcpStream::connect(ip_addr)
        .await
        .context("not possible to connect")?;

    let handshake = handshake::Handshake {
        name: String::from("BitTorrent protocol"),
        info_hash,
        peer_id: *b"00112233445566778899",
    };

    let mut handshake_bytes: Vec<u8> = handshake.into();

    client
        .write_all(&handshake_bytes[..])
        .await
        .context("send handshake to client")?;

    let len = handshake_bytes.len();

    client
        .read_exact(&mut handshake_bytes[..len])
        .await
        .context("reading from client")?;

    serde_bytes::serialize(&handshake, serializer);

    handshake_bytes
        .try_into()
        .context("converting response into handsheke")
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

            let meta: torrent_file::TorrentFile =
                serde_bencode::from_bytes(&content).context("not a valid meta file")?;

            let hash = meta.info_hash()?;

            println!("Tracker URL: {}", meta.announce,);
            println!("Length: {}", meta.info.length);
            println!("Info Hash: {}", hex::encode(hash));
            println!("Piece Length: {}", meta.info.piece_length);
            println!("Piece Hashes:");

            for hash in meta.info.pieces.0 {
                println!("{}", hex::encode(hash));
            }
        }
        Commands::Peers { file_path } => {
            let content = read(&file_path)
                .with_context(|| format!("file {} does not exists", file_path.display()))?;

            let meta: torrent_file::TorrentFile =
                serde_bencode::from_bytes(&content).context("not a valid meta file")?;

            let hash = meta.info_hash()?;

            let tracker = {
                tracker::Tracker {
                    info_hash: hash.into(),
                    peer_id: "00112233445566778899".to_owned(),
                    port: 6881,
                    uploaded: 0,
                    downloaded: 0,
                    left: meta.info.length,
                    compact: 1,
                }
            };

            let response = get_tracker_response(&meta.announce, tracker).await?;

            for peer in response.peers.0 {
                println!("{}", peer);
            }
        }
        Commands::Handshake { file_path, ip_addr } => {
            let content = read(&file_path)
                .with_context(|| format!("file {} does not exists", file_path.display()))?;

            let meta: torrent_file::TorrentFile =
                serde_bencode::from_bytes(&content).context("not a valid meta file")?;

            let hash = meta.info_hash()?;

            let response_handshake = handshake(ip_addr, hash.into()).await?;

            println!("Peer ID: {}", response_handshake.peer_id_string());
        }
        Commands::DownloadPiece {
            output_file_path: _,
            file_path,
            piece: _,
        } => {
            let torrent = torrent_file::TorrentFile::try_from(file_path)?;
            let hash = torrent.info_hash()?;

            let tracker = {
                tracker::Tracker {
                    info_hash: hash.into(),
                    peer_id: "00112233445566778899".to_owned(),
                    port: 6881,
                    uploaded: 0,
                    downloaded: 0,
                    left: torrent.info.length,
                    compact: 1,
                }
            };

            let response = get_tracker_response(&torrent.announce, tracker).await?;
            let peer = response.peers.0[0];

            let handshake = handshake(peer, hash.into());
        }
    };

    Ok(())
}
