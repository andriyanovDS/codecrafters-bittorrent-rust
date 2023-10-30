use crate::torrent_file::InfoHash;
use anyhow::Result;
use std::net::SocketAddrV4;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[repr(C)]
struct Handshake {
    protocol_len: u8,
    protocol: [u8; 19],
    reserved: [u8; 8],
    info_hash: [u8; 20],
    peer_id: [u8; 20],
}

impl Handshake {
    fn new(info_hash: &InfoHash, peer_id: [u8; 20]) -> Self {
        Self {
            protocol_len: 19,
            protocol: *b"BitTorrent protocol",
            reserved: [0; 8],
            info_hash: info_hash.0,
            peer_id,
        }
    }

    fn as_bytes_mut(&mut self) -> &mut [u8] {
        let bytes = self as *mut Self as *mut [u8; std::mem::size_of::<Self>()];
        // Safety: Handshake is a POD with repr(c)
        let bytes: &mut [u8; std::mem::size_of::<Self>()] = unsafe { &mut *bytes };
        bytes
    }
}

pub async fn handshake(
    info_hash: &InfoHash,
    peer_id: [u8; 20],
    address: &SocketAddrV4,
) -> Result<String> {
    let mut handshake = Handshake::new(info_hash, peer_id);
    let bytes = handshake.as_bytes_mut();

    let mut stream = tokio::net::TcpStream::connect(address).await?;
    stream.write_all(bytes).await?;
    stream.read_exact(bytes).await?;

    Ok(hex::encode(handshake.peer_id))
}
