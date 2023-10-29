use std::{
    fmt::{self, Display},
    marker::PhantomData,
};

use serde::{de::Visitor, Deserialize, Deserializer};

#[derive(Deserialize)]
pub struct TorrentFile {
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