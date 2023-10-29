use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use torrent_info::TorrentFile;

mod decode;
mod torrent_info;

#[derive(Parser, Debug)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    Decode { encoded_value: String },
    Info { file_path: PathBuf },
}

// Usage: your_bittorrent.sh decode "<encoded_value>"
fn main() -> Result<()> {
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
    }
    Ok(())
}
