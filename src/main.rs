use anyhow::{Error, Result};
use clap::{Parser, Subcommand};
use serde::de::Visitor;
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Number, Value};
use std::fmt::{self, Display};
use std::marker::PhantomData;
use std::path::PathBuf;

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

#[derive(Deserialize)]
struct TorrentFile {
    announce: String,
    info: Info,
}

const PIECE_LEN: usize = 20;

#[derive(Debug)]
struct Piece([u8; PIECE_LEN]);

#[derive(Deserialize)]
struct Info {
    length: usize,
    name: String,
    #[serde(rename = "piece length")]
    piece_length: usize,
    #[serde(deserialize_with = "deserialize_piece")]
    pieces: Vec<Piece>,
}

impl Display for TorrentFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Tracker URL: {}", self.announce)?;
        writeln!(f, "Length: {}", self.info.length)?;
        Ok(())
    }
}

fn decode_bencoded_value(encoded_value: &str) -> Result<(&str, Value)> {
    let Some(first_char) = encoded_value.chars().next() else {
        return Err(Error::msg(format!("Empty encoded value {}", encoded_value)));
    };
    match first_char {
        'i' => encoded_value[1..]
            .split_once('e')
            .ok_or_else(|| Error::msg("Bencode integer must ends with e"))
            .and_then(|(num, rest)| {
                num.parse::<i64>()
                    .map_err(Error::from)
                    .map(|num| Value::Number(Number::from(num)))
                    .map(|num| (rest, num))
            }),
        'l' => {
            let mut values = vec![];
            let mut rest = &encoded_value[1..];
            while !rest.starts_with('e') {
                let (encoded_value, value) = decode_bencoded_value(rest)?;
                rest = encoded_value;
                values.push(value);
            }
            Ok((&rest[1..], Value::Array(values)))
        }
        'd' => {
            let mut map = Map::new();
            let mut rest = &encoded_value[1..];
            while !rest.starts_with('e') {
                let (encoded_value, key) = decode_bencoded_value(rest)?;
                rest = encoded_value;
                if let Value::String(key) = key {
                    let (encoded_value, value) = decode_bencoded_value(rest)?;
                    map.insert(key, value);
                    rest = encoded_value;
                } else {
                    return Err(Error::msg(format!(
                        "Bencode dictinary key must be a string. Value: {encoded_value}"
                    )));
                }
            }
            Ok((&rest[1..], Value::Object(map)))
        }
        _ if first_char.is_ascii_digit() => encoded_value
            .split_once(':')
            .and_then(|(length, rest)| {
                let length = length.parse::<usize>().ok()?;
                Some((
                    &rest[length..],
                    serde_json::Value::String(rest[0..length].to_string()),
                ))
            })
            .ok_or_else(|| Error::msg(format!("Invalid encoded string {encoded_value}"))),
        _ => Err(Error::msg(format!(
            "Unexpected bencode value {encoded_value}"
        ))),
    }
}

// Usage: your_bittorrent.sh decode "<encoded_value>"
fn main() -> Result<()> {
    let cli = Cli::parse();
    match &cli.command {
        Command::Decode { encoded_value } => {
            let (_, decoded_value) = decode_bencoded_value(encoded_value)?;
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

fn deserialize_piece<'de, D>(deserializer: D) -> Result<Vec<Piece>, D::Error>
where
    D: Deserializer<'de>,
{
    struct PieceVisitor(PhantomData<fn() -> Vec<Piece>>);

    impl<'de> Visitor<'de> for PieceVisitor {
        /// Return type of this visitor. This visitor computes the max of a
        /// sequence of values of type T, so the type of the maximum is T.
        type Value = Vec<Piece>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a nonempty sequence of numbers")
        }

        fn visit_bytes<E>(self, v: &[u8]) -> std::result::Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            let mut pieces = Vec::new();
            let mut start = 0;
            while v.len() - start >= PIECE_LEN {
                let mut buffer = [0u8; PIECE_LEN];
                v[start..start + PIECE_LEN]
                    .iter()
                    .enumerate()
                    .for_each(|(index, byte)| {
                        buffer[index] = *byte;
                    });
                pieces.push(Piece(buffer));
                start += PIECE_LEN;
            }
            Ok(pieces)
        }
    }

    let visitor = PieceVisitor(PhantomData);
    deserializer.deserialize_seq(visitor)
}
