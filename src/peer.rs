use crate::torrent_file::{InfoHash, Piece as PieceHash, TorrentFile};
use crate::tracker;
use anyhow::{Error, Result};
use bytes::Buf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub const PEER_ID: [u8; 20] = *b"00112233445566778899";

#[repr(C)]
pub struct Handshake {
    protocol_len: u8,
    protocol: [u8; 19],
    reserved: [u8; 8],
    info_hash: [u8; 20],
    peer_id: [u8; 20],
}

#[repr(u8)]
#[derive(PartialEq, Debug)]
pub enum MessageType {
    Choke,
    Unchoke,
    Interested,
    NotInterested,
    Have,
    Bitfield,
    Request,
    Piece,
    Cancel,
}

impl From<u8> for MessageType {
    fn from(value: u8) -> Self {
        match value {
            0 => MessageType::Choke,
            1 => MessageType::Unchoke,
            2 => MessageType::Interested,
            3 => MessageType::NotInterested,
            4 => MessageType::Have,
            5 => MessageType::Bitfield,
            6 => MessageType::Request,
            7 => MessageType::Piece,
            8 => MessageType::Cancel,
            _ => panic!("Unsupported message type {value}"),
        }
    }
}

impl Into<u8> for MessageType {
    fn into(self) -> u8 {
        match self {
            MessageType::Choke => 0,
            MessageType::Unchoke => 1,
            MessageType::Interested => 2,
            MessageType::NotInterested => 3,
            MessageType::Have => 4,
            MessageType::Bitfield => 5,
            MessageType::Request => 6,
            MessageType::Piece => 7,
            MessageType::Cancel => 8,
        }
    }
}

pub trait BytesConvertible {
    fn as_bytes(&self) -> &[u8];
}

pub trait TryFromBytes: Sized {
    fn try_from_bytes(bytes: Vec<u8>) -> Result<Self>;
}

pub struct EmptyPayload;
const EMPTY_SLICE: &'static [u8; 0] = &[];

impl BytesConvertible for EmptyPayload {
    fn as_bytes(&self) -> &[u8] {
        EMPTY_SLICE
    }
}

impl TryFromBytes for EmptyPayload {
    fn try_from_bytes(bytes: Vec<u8>) -> Result<Self> {
        assert!(bytes.is_empty());
        Ok(Self)
    }
}

pub struct Message<Payload> {
    pub message_type: MessageType,
    pub payload: Payload,
}

#[repr(C)]
pub struct RequestPayload {
    index: [u8; 4],
    begin: [u8; 4],
    length: [u8; 4],
}

impl RequestPayload {
    pub fn new(index: usize, begin: usize, length: usize) -> Self {
        Self {
            index: (index as i32).to_be_bytes(),
            begin: (begin as i32).to_be_bytes(),
            length: (length as i32).to_be_bytes(),
        }
    }
}

impl BytesConvertible for RequestPayload {
    fn as_bytes(&self) -> &[u8] {
        let bytes = self as *const Self as *const [u8; std::mem::size_of::<Self>()];
        let bytes: &[u8; std::mem::size_of::<Self>()] = unsafe { &*bytes };
        bytes
    }
}

#[repr(C)]
pub struct Piece {
    index: [u8; 4],
    begin: [u8; 4],
    pub block: Vec<u8>,
}

impl TryFromBytes for Piece {
    fn try_from_bytes(mut bytes: Vec<u8>) -> Result<Self> {
        if bytes.len() < 8 {
            return Err(Error::msg("Bytes length must be at least 8 bytes."));
        }
        let index: [u8; 4] = bytes[0..4].try_into()?;
        let begin: [u8; 4] = bytes[4..8].try_into()?;
        bytes.drain(0..8);
        Ok(Piece {
            index,
            begin,
            block: bytes,
        })
    }
}

#[derive(Debug)]
pub struct Bitfield(Vec<u8>);

impl Bitfield {
    pub fn has_piece(&self, piece_index: usize) -> bool {
        let byte_index = piece_index / 8;
        let bit_index = piece_index % 8;
        assert!(byte_index < self.0.len());
        let byte = self.0[byte_index];
        (byte << bit_index) & 128 == 128
    }
}

impl TryFromBytes for Bitfield {
    fn try_from_bytes(bytes: Vec<u8>) -> Result<Self> {
        Ok(Self(bytes))
    }
}

impl Handshake {
    pub fn new(info_hash: &InfoHash, peer_id: [u8; 20]) -> Self {
        Self {
            protocol_len: 19,
            protocol: *b"BitTorrent protocol",
            reserved: [0; 8],
            info_hash: info_hash.0,
            peer_id,
        }
    }

    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        let bytes = self as *mut Self as *mut [u8; std::mem::size_of::<Self>()];
        // Safety: Handshake is a POD with repr(c)
        let bytes: &mut [u8; std::mem::size_of::<Self>()] = unsafe { &mut *bytes };
        bytes
    }
}

pub async fn handshake(info_hash: &InfoHash, stream: &mut TcpStream) -> Result<String> {
    let mut handshake = Handshake::new(info_hash, PEER_ID);
    let bytes = handshake.as_bytes_mut();

    stream.write_all(bytes).await?;
    stream.read_exact(bytes).await?;

    Ok(hex::encode(handshake.peer_id))
}

pub async fn download_peice(file: &TorrentFile, index: usize) -> Result<Vec<u8>> {
    assert!(index < file.info.pieces.len());

    let info_hash = file.info.hash()?;
    let peers =
        tracker::discover_peers(file.announce.as_str(), &info_hash, file.info.length).await?;
    let Some(peer) = peers.first() else {
        return Err(Error::msg("Peers are empty."));
    };
    let mut stream = tokio::net::TcpStream::connect(&peer.0).await?;
    _ = handshake(&info_hash, &mut stream).await?;

    let bitfield_mesasge = read_message::<Bitfield>(&mut stream).await?;
    assert_eq!(bitfield_mesasge.message_type, MessageType::Bitfield);

    send_message(
        Message {
            message_type: MessageType::Interested,
            payload: EmptyPayload,
        },
        &mut stream,
    )
    .await?;
    let unchoke_message = read_message::<EmptyPayload>(&mut stream).await?;
    assert_eq!(unchoke_message.message_type, MessageType::Unchoke);

    let hash = &file.info.pieces[index];
    request_peice(
        index,
        file.info.piece_length,
        hash,
        file.info.length,
        &mut stream,
    )
    .await
}

async fn read_message<P: TryFromBytes>(stream: &mut TcpStream) -> Result<Message<P>> {
    let mut header = [0u8; 4];
    stream.read_exact(header.as_mut()).await?;
    let length = u32::from_be_bytes(header) as usize;

    let mut message_id = [0u8; 1];
    stream.read_exact(message_id.as_mut()).await?;
    let message_type = MessageType::from(message_id[0]);

    let mut payload = vec![0; length - message_id.len()];
    if !payload.is_empty() {
        stream.read_exact(payload.as_mut()).await?;
    }

    // println!("read payload {payload:?}");
    let payload = P::try_from_bytes(payload)?;
    Ok(Message {
        message_type,
        payload,
    })
}

async fn send_message<P: BytesConvertible>(
    message: Message<P>,
    stream: &mut TcpStream,
) -> Result<()> {
    let mut payload = message.payload.as_bytes();
    let message_size = (payload.len() as i32 + 1).to_be_bytes();
    let message_id: u8 = message.message_type.into();
    let mut buffer = vec![0; payload.len() + 4 + 1];

    message_size.as_ref().copy_to_slice(&mut buffer[0..4]);
    [message_id].as_ref().copy_to_slice(&mut buffer[4..5]);
    payload.copy_to_slice(&mut buffer[5..]);

    stream.write_all(&buffer).await?;

    Ok(())
}

const CHUNK_SIZE: usize = 1 << 14;
async fn request_peice(
    piece_index: usize,
    size: usize,
    hash: &PieceHash,
    file_length: usize,
    stream: &mut TcpStream,
) -> Result<Vec<u8>> {
    let mut offset = 0;
    let piece_size = size.min(file_length - piece_index * size);
    let mut buffer = Vec::with_capacity(piece_size);
    while offset < piece_size {
        let block_size = (piece_size - offset).min(CHUNK_SIZE);
        let payload = RequestPayload::new(piece_index, offset, block_size);
        send_message(
            Message {
                message_type: MessageType::Request,
                payload,
            },
            stream,
        )
        .await?;
        let chunk = read_message::<Piece>(stream).await?;
        assert_eq!(chunk.message_type, MessageType::Piece);
        assert_eq!(chunk.payload.block.len(), block_size);

        buffer.extend(chunk.payload.block.as_slice());
        offset += block_size;
    }
    assert_eq!(hash, &PieceHash::from(buffer.as_slice()));
    Ok(buffer)
}
