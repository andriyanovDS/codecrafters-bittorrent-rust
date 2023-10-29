use anyhow::Result;
use sha1::{Digest, Sha1};
use std::{
    fmt::{self, Display},
    marker::PhantomData,
};

use serde::{de::Visitor, Deserialize, Deserializer, Serialize, Serializer};

#[derive(Deserialize)]
pub struct TorrentFile {
    pub announce: String,
    pub info: Info,
}

const PIECE_LEN: usize = 20;

#[derive(Debug, Serialize)]
pub struct Piece([u8; PIECE_LEN]);

#[derive(Deserialize, Serialize, Debug)]
pub struct Info {
    pub length: usize,
    pub name: String,
    #[serde(rename = "piece length")]
    pub piece_length: usize,
    #[serde(deserialize_with = "deserialize_piece")]
    #[serde(serialize_with = "serialize_piece")]
    pub pieces: Vec<Piece>,
}

impl<'a> IntoIterator for &'a Piece {
    type Item = &'a u8;
    type IntoIter = std::slice::Iter<'a, u8>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl Display for TorrentFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Tracker URL: {}", self.announce)?;
        writeln!(f, "Length: {}", self.info.length)?;
        writeln!(
            f,
            "Info Hash: {}",
            hex::encode(self.info.hash().expect("Unable to hash info").0)
        )?;
        writeln!(f, "Piece Length: {}", self.info.piece_length)?;
        writeln!(f, "Piece Hashes:")?;
        for piece in &self.info.pieces {
            writeln!(f, "{}", hex::encode(piece.0))?;
        }
        Ok(())
    }
}

const INFO_HASH_SIZE: usize = 20;
pub struct InfoHash(pub [u8; INFO_HASH_SIZE]);

impl Info {
    pub fn hash(&self) -> Result<InfoHash> {
        let encoded_info = serde_bencode::to_bytes(self)?;
        let mut hasher = Sha1::new();
        hasher.update(&encoded_info);
        let result = hasher.finalize();
        Ok(InfoHash(result.try_into().expect("Unable to hash info")))
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

fn serialize_piece<S>(piece: &[Piece], s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let bytes = piece.iter().flatten().copied().collect::<Vec<u8>>();
    s.serialize_bytes(&bytes)
}
