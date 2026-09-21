const PROTOCOL_VERSION: u32 = 70015;

use rutabaga::hash_tracker::HashTracker;
use rutabaga::outs_file::OutsFile;
use rutabaga::spent_file::SpentFile;

use std::io::{BufReader, Write};
use std::path::Path;
use std::{env, fs, process};

use bitcoin::consensus::{encode, Decodable};
use bitcoin::hashes::Hash;
use bitcoin::key::Keypair;
use bitcoin::p2p::message::{NetworkMessage, RawNetworkMessage};
use bitcoin::p2p::message_blockdata::{GetHeadersMessage, Inventory};
use bitcoin::p2p::{self, address, message_network};
use bitcoin::secp256k1::rand::Rng;
use bitcoin::secp256k1::{Secp256k1, SecretKey};
use bitcoin::{BlockHash, Network, OutPoint, ScriptBuf, TxOut};
use log::{debug, error, info, trace};
use std::net::{IpAddr, Ipv4Addr, Shutdown, SocketAddr, TcpStream};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn script_grep<'a>(
    script_pubkey: &'a ScriptBuf,
    block: &'a bitcoin::Block,
) -> Vec<(OutPoint, TxOut)> {
    let mut ret = vec![];
    // ignore first transaction in block as coin-base
    for tx in block.txdata.iter().skip(1) {
        for (i, output) in tx.output.iter().enumerate() {
            if output.script_pubkey == *script_pubkey {
                let vout: u32 = i.try_into().unwrap();
                let outpoint = OutPoint { vout, txid: tx.compute_txid() };
                ret.push((outpoint, output.clone()))
            }
        }
    }
    ret
}

pub fn outpoint_grep(stored_outs: Vec<(OutPoint, TxOut)>, block: &bitcoin::Block) -> Vec<OutPoint> {
    let stored_outpoints: Vec<_> = stored_outs.iter().map(|(outpoint, _)| *outpoint).collect();
    let mut spent_outs = vec![];
    for tx in block.txdata.iter().skip(1) {
        for input in tx.input.iter() {
            let outpoint = input.previous_output;
            let vout = outpoint.vout;
            let txid = outpoint.txid;
            let bitcoin_outpoint = bitcoin::OutPoint { txid, vout };
            if stored_outpoints.contains(&bitcoin_outpoint) {
                spent_outs.push(bitcoin_outpoint);
            }
        }
    }
    spent_outs
}

fn read_pubkey_from_file(keys_file_path: &str) -> ScriptBuf {
    let s = Secp256k1::new();
    let bytes: Vec<u8> =
        fs::read(keys_file_path).expect("the env var RUTABAGA_KEY_FILE should be readable");
    let sk = SecretKey::from_slice(&bytes).unwrap();
    let kp = Keypair::from_secret_key(&s, &sk);
    ScriptBuf::new_p2tr(&s, kp.x_only_public_key().0, None)
}

fn build_headers_request(locator_hash: BlockHash) -> RawNetworkMessage {
    let stop_hash: BlockHash = BlockHash::from_slice(&[0; 32]).unwrap();

    let get_headers_msg = GetHeadersMessage {
        version: PROTOCOL_VERSION,
        locator_hashes: vec![locator_hash],
        stop_hash,
    };

    RawNetworkMessage::new(
        bitcoin::Network::Signet.magic(),
        NetworkMessage::GetHeaders(get_headers_msg),
    )
}

async fn connect(
    network: Network,
    address: SocketAddr,
    spend_script: &ScriptBuf,
    outs_path: &Path,
    spent_path: &Path,
    tracker_path: &Path,
) -> std::io::Result<()> {
    let mut hash_tracker = HashTracker::new(tracker_path);
    let mut outs_file = OutsFile::new(outs_path);
    let mut spent_file = SpentFile::new(spent_path);

    let version_message = build_version_message(address);
    let first_message = RawNetworkMessage::new(network.magic(), version_message);

    let mut blockhash_queue = vec![];
    let mut decoding_fails: u64 = 0;
    if let Ok(mut stream) = TcpStream::connect(address) {
        let _ = stream.write_all(encode::serialize(&first_message).as_slice());
        info!("Sent version message");

        // Setup StreamReader
        let read_stream = stream.try_clone().unwrap();
        let mut stream_reader = BufReader::new(read_stream);

        loop {
            let reply = RawNetworkMessage::consensus_decode(&mut stream_reader);
            if let Ok(r) = reply {
                match r.payload() {
                    NetworkMessage::Block(block) => {
                        let header = block.header;
                        let block_hash = header.block_hash();
                        blockhash_queue.retain(|bh| *bh != block_hash);

                        let prev_blockhash = header.prev_blockhash;
                        if let Some(prev_height) = hash_tracker.get_hash_height(&prev_blockhash) {
                            let height = prev_height + 1;
                            info!(
                                "recieved hash from peer at height: {:?} with hash {:?}",
                                height, block_hash
                            );

                            hash_tracker.insert_and_store(block_hash, height);

                            // check the block for any matching keys
                            let outs = script_grep(spend_script, block);

                            // write any of the keys and value
                            outs_file.append_entries(outs);

                            // check if any of the outpoints are spent
                            let spent_outs = outpoint_grep(outs_file.read(), block);
                            spent_file.append_entries(spent_outs);
                        } else {
                            debug!(
                                "ignoring block {} received out of order preceded by {}",
                                block_hash, prev_blockhash
                            );
                        }

                        if blockhash_queue.is_empty() {
                            debug!("done, ready for next block");
                            let msg = build_headers_request(block_hash);
                            let _ = stream.write_all(encode::serialize(&msg).as_slice());
                        }
                    }
                    NetworkMessage::Headers(headers) => {
                        if headers.is_empty() {
                            info!("No new headers from peer (all caught up).");
                        } else {
                            let hashes: Vec<BlockHash> =
                                headers.iter().map(|h| h.block_hash()).collect();
                            let inv_vec: Vec<Inventory> =
                                hashes.iter().map(|h| Inventory::Block(*h)).collect();

                            let inv_message = RawNetworkMessage::new(
                                bitcoin::Network::Signet.magic(),
                                NetworkMessage::GetData(inv_vec.clone()),
                            );

                            blockhash_queue = hashes;

                            let _ = stream.write_all(encode::serialize(&inv_message).as_slice());
                        }
                    }
                    NetworkMessage::Inv(inv_vec) => {
                        for inv in inv_vec {
                            match inv {
                                Inventory::Block(hash) => {
                                    let known_hash = hash_tracker.contains_hash(hash);
                                    if !known_hash {
                                        let inv_message = RawNetworkMessage::new(
                                            bitcoin::Network::Signet.magic(),
                                            NetworkMessage::GetData(inv_vec.clone()),
                                        );

                                        info!("requestng block from peer: {}", hash);
                                        let _ = stream
                                            .write_all(encode::serialize(&inv_message).as_slice());
                                    }
                                }
                                _ => debug!("unimplemented {:?}", inv),
                            }
                        }
                    }
                    NetworkMessage::Version(_) => {
                        info!("Received version message from peer: {:?}", r.payload());

                        let second_message = RawNetworkMessage::new(
                            bitcoin::Network::Signet.magic(),
                            NetworkMessage::Verack,
                        );

                        let _ = stream.write_all(encode::serialize(&second_message).as_slice());
                        info!("reply to version message with Verack");
                    }
                    NetworkMessage::Verack => {
                        info!("peer sent Verack");

                        let (height, hash) = hash_tracker.last_hash();
                        info!("send block height {} {:?} to peer", height, hash);

                        let msg = build_headers_request(hash);
                        let _ = stream.write_all(encode::serialize(&msg).as_slice());
                    }
                    NetworkMessage::Ping(nonce) => {
                        trace!("Received Ping {:?}", nonce);
                        let pong = NetworkMessage::Pong(*nonce);
                        let reply = RawNetworkMessage::new(bitcoin::Network::Signet.magic(), pong);
                        let _ = stream.write_all(encode::serialize(&reply).as_slice());
                        trace!("Sent Pong {:?}", nonce);
                    }
                    _ => {
                        debug!("unimplemented: {:?}", r.payload());
                    }
                }
            } else {
                decoding_fails += 1;
                error!("error decoding raw network message {:?} {} times", reply, decoding_fails);
                if decoding_fails > 100 {
                    error!("reached connection error limit, shutting down");
                    let _ = stream.shutdown(Shutdown::Both);
                    process::exit(1);
                }
            }
        }
    } else {
        error!("Failed to open connection");
    }

    Ok(())
}

fn build_version_message(address: SocketAddr) -> NetworkMessage {
    // Building version message, see https://en.bitcoin.it/wiki/Protocol_documentation#version
    let my_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0);

    // "bitfield of features to be enabled for this connection"
    let services = p2p::ServiceFlags::NONE;

    // "standard UNIX timestamp in seconds"
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).expect("Time error").as_secs();

    // "The network address of the node receiving this message"
    let addr_recv = address::Address::new(&address, p2p::ServiceFlags::NONE);

    // "The network address of the node emitting this message"
    let addr_from = address::Address::new(&my_address, p2p::ServiceFlags::NONE);

    // "Node random nonce, randomly generated every time a version packet is sent. This nonce is used to detect connections to self."
    let nonce: u64 = bitcoin::secp256k1::rand::thread_rng().gen();

    // "User Agent (0x00 if string is 0 bytes long)"
    let user_agent = String::from("rutabaga");

    // "The last block received by the emitting node"
    let start_height: i32 = 0;

    bitcoin::p2p::message::NetworkMessage::Version(message_network::VersionMessage {
        version: PROTOCOL_VERSION,
        services,
        timestamp: timestamp as i64,
        receiver: addr_recv,
        sender: addr_from,
        nonce,
        user_agent,
        start_height,
        relay: false,
    })
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();
    info!("Loading Rutabaga");

    let args: Vec<String> = env::args().collect();

    if args.len() < 5 {
        error!(
            "Usage: {} <node_ip_address> <path_to_data_dir> <keys_file> <outputs_file>",
            args[0]
        );
        process::exit(1);
    }

    let str_address = &args[1];
    let address: SocketAddr = str_address.parse().unwrap_or_else(|error| {
        error!("Error parsing address: {:?}", error);
        process::exit(1);
    });

    let mut paths = std::collections::HashSet::new();

    let str_tracker_path = &args[2];
    paths.insert(str_tracker_path);
    let tracker_path = Path::new(&str_tracker_path);

    let str_keys_path = &args[3];
    paths.insert(str_keys_path);
    let spend_script = read_pubkey_from_file(str_keys_path);

    let str_outputs_path = &args[4];
    paths.insert(str_outputs_path);
    let outputs_path = Path::new(&str_outputs_path);

    let str_spent_path = &args[5];
    paths.insert(str_spent_path);
    let spent_path = Path::new(&str_spent_path);

    if paths.len() < 4 {
        error!(
            "Error paths are not unique: {:?} {:?} {:?} {:?}",
            str_tracker_path, str_keys_path, str_outputs_path, str_spent_path
        );
        process::exit(1);
    }

    let _ =
        connect(Network::Signet, address, &spend_script, outputs_path, spent_path, tracker_path)
            .await;

    Ok(())
}
