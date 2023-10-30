use anyhow::Result;
use clap::{Parser, Subcommand};
use peer::handshake;
use std::{net::SocketAddrV4, path::PathBuf};
use torrent_file::TorrentFile;

mod decode;
mod peer;
mod torrent_file;
mod tracker;

#[derive(Parser, Debug)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
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
        peer: SocketAddrV4,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match &cli.command {
        Command::Decode { encoded_value } => {
            let (_, decoded_value) = decode::decode_bencoded_value(encoded_value)?;
            println!("{}", decoded_value);
        }
        Command::Info { file_path } => {
            let file = std::fs::read(file_path)?;
            let torrent = serde_bencode::from_bytes::<TorrentFile>(&file)?;
            println!("{torrent}");
        }
        Command::Peers { file_path } => {
            let file = std::fs::read(file_path)?;
            let torrent = serde_bencode::from_bytes::<TorrentFile>(&file)?;
            let peers = tracker::discover_peers(
                torrent.announce.as_str(),
                torrent.info.hash()?,
                torrent.info.length,
            )
            .await?;
            for peer in peers {
                println!("{}", peer.0);
            }
        }
        Command::Handshake { file_path, peer } => {
            let file = std::fs::read(file_path)?;
            let torrent = serde_bencode::from_bytes::<TorrentFile>(&file)?;
            let peer_id = handshake(&torrent.info.hash()?, *b"00112233445566778899", peer).await?;
            println!("{peer_id}");
        }
    }
    Ok(())
}
