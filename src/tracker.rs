use crate::torrent_file::InfoHash;
use anyhow::Result;
use serde::{de::Visitor, Deserialize, Deserializer, Serialize};
use std::{
    fmt,
    marker::PhantomData,
    net::{Ipv4Addr, SocketAddrV4},
};

#[derive(Debug, Serialize)]
struct TrackerRequest {
    peer_id: String,
    port: usize,
    uploaded: usize,
    downloaded: usize,
    left: usize,
    compact: u8,
}

#[derive(Debug, Deserialize)]
struct TrackerResponse {
    #[serde(deserialize_with = "deserialize_peers")]
    peers: Vec<Peer>,
}

pub async fn discover_peers(
    announce: &str,
    info_hash: InfoHash,
    file_size: usize,
) -> Result<Vec<Peer>> {
    let request = TrackerRequest {
        port: 6881,
        peer_id: "00112233445566778899".to_string(),
        uploaded: 0,
        downloaded: 0,
        left: file_size,
        compact: 1,
    };

    let url_params = serde_urlencoded::to_string(&request)?;
    let tracker_url = format!(
        "{}?{}&info_hash={}",
        announce,
        url_params,
        &urlencode(&info_hash)
    );

    let bytes = reqwest::get(tracker_url).await?.bytes().await?;
    let response = serde_bencode::from_bytes::<TrackerResponse>(&bytes)?;
    Ok(response.peers)
}

const PEER_SIZE: usize = 6;

#[derive(Debug)]
pub struct Peer(pub SocketAddrV4);

fn deserialize_peers<'de, D>(deserializer: D) -> Result<Vec<Peer>, D::Error>
where
    D: Deserializer<'de>,
{
    struct PieceVisitor(PhantomData<fn() -> Vec<Peer>>);

    impl<'de> Visitor<'de> for PieceVisitor {
        type Value = Vec<Peer>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a nonempty sequence of numbers")
        }

        fn visit_bytes<E>(self, v: &[u8]) -> std::result::Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            let peer = v
                .chunks_exact(PEER_SIZE)
                .map(|chunk| {
                    let port = u16::from_be_bytes([chunk[4], chunk[5]]);
                    SocketAddrV4::new(Ipv4Addr::new(chunk[0], chunk[1], chunk[2], chunk[3]), port)
                })
                .map(Peer)
                .collect();
            Ok(peer)
        }
    }

    deserializer.deserialize_seq(PieceVisitor(PhantomData))
}

fn urlencode(hash: &InfoHash) -> String {
    let mut encoded = String::with_capacity(3 * hash.0.len());
    for &byte in &hash.0 {
        encoded.push('%');
        encoded.push_str(&hex::encode([byte]));
    }
    encoded
}
