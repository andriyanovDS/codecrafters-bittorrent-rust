use std::{
    io::{Read, Write},
    net::TcpStream,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use crate::{
    peer::{
        Bitfield, BytesConvertible, EmptyPayload, Handshake, Message, MessageType, Piece,
        RequestPayload, TryFromBytes, PEER_ID,
    },
    torrent_file::{InfoHash, Piece as PieceHash, TorrentFile},
    tracker::{self, Peer},
};
use anyhow::Result;
use bytes::Buf;

pub async fn download_file(file: TorrentFile, output: &PathBuf) -> Result<()> {
    let info_hash = file.info.hash()?;
    let peers =
        tracker::discover_peers(file.announce.as_str(), &info_hash, file.info.length).await?;
    let file_length = file.info.length;
    let piece_length = file.info.piece_length;
    let info_hash = file.info.hash()?;
    let peers = Arc::new(Mutex::new(peers));
    let file_buffer = Arc::new(Mutex::new(vec![0; file.info.length]));
    let pieces = Arc::new(Mutex::new(
        file.info.pieces.into_iter().enumerate().collect(),
    ));
    let handles = (0..5)
        .map(|_| {
            let peers = peers.clone();
            let pieces = pieces.clone();
            let file_buffer = file_buffer.clone();
            let info_hash = info_hash.clone();
            std::thread::spawn(move || {
                run(
                    peers,
                    pieces,
                    file_buffer,
                    info_hash,
                    file_length,
                    piece_length,
                );
            })
        })
        .collect::<Vec<_>>();
    for handle in handles {
        handle.join().unwrap();
    }
    let file = file_buffer.lock().unwrap();
    std::fs::write(output, file.as_slice())?;
    Ok(())
}

fn run(
    peers: Arc<Mutex<Vec<Peer>>>,
    pieces: Arc<Mutex<Vec<(usize, PieceHash)>>>,
    file_buffer: Arc<Mutex<Vec<u8>>>,
    info_hash: InfoHash,
    file_length: usize,
    piece_length: usize,
) {
    loop {
        let Some(peer) = peers.lock().unwrap().pop() else {
            return;
        };
        match download_from_peer(
            &peer,
            pieces.clone(),
            file_buffer.clone(),
            info_hash.clone(),
            file_length,
            piece_length,
        ) {
            Ok(_) => {}
            Err(error) => {
                eprintln!(
                    "Failed to download piece from peer {} with error: {:?}",
                    peer.0, error
                );
                peers.lock().unwrap().push(peer);
            }
        }
    }
}

fn download_from_peer(
    peer: &Peer,
    pieces: Arc<Mutex<Vec<(usize, PieceHash)>>>,
    file_buffer: Arc<Mutex<Vec<u8>>>,
    info_hash: InfoHash,
    file_length: usize,
    piece_length: usize,
) -> Result<()> {
    let mut stream = std::net::TcpStream::connect(peer.0)?;
    handshake(&info_hash, &mut stream)?;

    let bitfield_mesasge = read_message::<Bitfield>(&mut stream)?;
    assert_eq!(bitfield_mesasge.message_type, MessageType::Bitfield);

    send_message(
        Message {
            message_type: MessageType::Interested,
            payload: EmptyPayload,
        },
        &mut stream,
    )?;
    let unchoke_message = read_message::<EmptyPayload>(&mut stream)?;
    assert_eq!(unchoke_message.message_type, MessageType::Unchoke);

    loop {
        let mut pieces = pieces.lock().unwrap();
        let index = pieces
            .iter()
            .position(|(index, _)| bitfield_mesasge.payload.has_piece(*index));

        let Some(piece_index) = index else {
            return Ok(());
        };

        let (piece_index, piece_hash) = pieces.remove(piece_index);
        drop(pieces);

        let piece_buffer = request_peice(
            piece_index,
            piece_length,
            &piece_hash,
            file_length,
            &mut stream,
        )?;
        let mut file_buffer = file_buffer.lock().unwrap();
        let offset = piece_index * piece_length;
        piece_buffer
            .as_slice()
            .copy_to_slice(&mut file_buffer.as_mut_slice()[offset..offset + piece_buffer.len()]);
    }
}

fn handshake(info_hash: &InfoHash, stream: &mut TcpStream) -> Result<()> {
    let mut handshake = Handshake::new(info_hash, PEER_ID);
    let bytes = handshake.as_bytes_mut();

    stream.write_all(bytes)?;
    stream.read_exact(bytes)?;
    Ok(())
}

fn read_message<P: TryFromBytes>(stream: &mut TcpStream) -> Result<Message<P>> {
    let mut header = [0u8; 4];
    stream.read_exact(header.as_mut())?;
    let length = u32::from_be_bytes(header) as usize;

    let mut message_id = [0u8; 1];
    stream.read_exact(message_id.as_mut())?;
    let message_type = MessageType::from(message_id[0]);

    let mut payload = vec![0; length - message_id.len()];
    if !payload.is_empty() {
        stream.read_exact(payload.as_mut())?;
    }

    let payload = P::try_from_bytes(payload)?;
    Ok(Message {
        message_type,
        payload,
    })
}

fn send_message<P: BytesConvertible>(message: Message<P>, stream: &mut TcpStream) -> Result<()> {
    let mut payload = message.payload.as_bytes();
    let message_size = (payload.len() as i32 + 1).to_be_bytes();
    let message_id: u8 = message.message_type.into();
    let mut buffer = vec![0; payload.len() + 4 + 1];

    message_size.as_ref().copy_to_slice(&mut buffer[0..4]);
    [message_id].as_ref().copy_to_slice(&mut buffer[4..5]);
    payload.copy_to_slice(&mut buffer[5..]);

    stream.write_all(&buffer)?;

    Ok(())
}

const CHUNK_SIZE: usize = 1 << 14;
fn request_peice(
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
        )?;
        let chunk = read_message::<Piece>(stream)?;
        assert_eq!(chunk.message_type, MessageType::Piece);
        assert_eq!(chunk.payload.block.len(), block_size);

        buffer.extend(chunk.payload.block.as_slice());
        offset += block_size;
    }
    assert_eq!(hash, &PieceHash::from(buffer.as_slice()));
    Ok(buffer)
}
