use anyhow::Result;
use clap::{Parser, Subcommand};
use magnet_link::MagnetLink;
use peer::{download_peice, handshake};
use std::{net::SocketAddrV4, path::PathBuf};
use torrent_file::TorrentFile;

use crate::file_download::download_file;

mod decode;
mod file_download;
mod peer;
mod torrent_file;
mod tracker;
mod magnet_link;

#[derive(Parser, Debug)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
#[clap(rename_all = "snake_case")]
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
    DownloadPiece {
        #[arg(short)]
        output: PathBuf,
        file_path: PathBuf,
        piece: usize,
    },
    Download {
        #[arg(short)]
        output: PathBuf,
        file_path: PathBuf,
    },
    MagnetParse {
        link: String,
    }
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
            let info_hash = torrent.info.hash()?;
            let peers =
                tracker::discover_peers(torrent.announce.as_str(), &info_hash, torrent.info.length)
                    .await?;
            for peer in peers {
                println!("{}", peer.0);
            }
        }
        Command::Handshake { file_path, peer } => {
            let file = std::fs::read(file_path)?;
            let torrent = serde_bencode::from_bytes::<TorrentFile>(&file)?;
            let mut stream = tokio::net::TcpStream::connect(peer).await?;
            let peer_id = handshake(&torrent.info.hash()?, &mut stream).await?;
            println!("Peer ID: {peer_id}");
        }
        Command::DownloadPiece {
            output,
            file_path,
            piece: piece_index,
        } => {
            let file = std::fs::read(file_path)?;
            let torrent = serde_bencode::from_bytes::<TorrentFile>(&file)?;
            let piece = download_peice(&torrent, *piece_index).await?;
            std::fs::write(output, &piece)?;
            println!("Piece {piece_index} downloaded to {output:?}");
        }
        Command::Download { output, file_path } => {
            let file = std::fs::read(file_path)?;
            let torrent = serde_bencode::from_bytes::<TorrentFile>(&file)?;
            download_file(torrent, output).await?;
            println!("Downloaded {file_path:?} to {output:?}");
        },
        Command::MagnetParse { link } => {
            let magnet_link = MagnetLink::parse(link.as_str())?;
            for tracker in magnet_link.tracker_address {
                println!("Tracker URL: {tracker}");
            }
            println!("Info Hash: {}", magnet_link.info_hash.hash);
        }
    }
    Ok(())
}
