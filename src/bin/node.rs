use rutabaga::output_ledger;
use rutabaga::spent_ledger;

const PROTOCOL_VERSION: u32 = 70015;

use std::fs::{File, OpenOptions};
use std::io::Read;
use std::path::Path;
use std::{env, fs, process};

use bitcoin::hashes::Hash;
use bitcoin::key::Keypair;
use bitcoin::p2p::message::{NetworkMessage, RawNetworkMessage};
use bitcoin::p2p::{self, address, message_network};
use bitcoin::secp256k1::{Secp256k1, SecretKey};
use bitcoin::{BlockHash, Network, OutPoint, ScriptBuf, TxOut};
use std::net::{IpAddr, Ipv4Addr, Shutdown, SocketAddr, TcpStream};
use std::time::{SystemTime, UNIX_EPOCH};

use bitcoin::consensus::{encode, Decodable};
use std::io::{BufReader, Write};

use bitcoin::secp256k1::rand::Rng;

use bitcoin::p2p::message_blockdata::{GetHeadersMessage, Inventory};
use std::collections::HashMap;

// Create rust-bitcoin TxOut from kernel parts
fn output(index: usize, tx_ref: &bitcoin::Transaction) -> TxOut {
    let tx = tx_ref.to_owned();
    let mut outputs = tx.output.iter();
    let output = outputs.nth(index).unwrap();
    let script_pubkey = output.script_pubkey.clone();
    let value = output.value;
    TxOut { value, script_pubkey }
}

// Create rust-bitcoin OutPoint from kernel parts
fn outpoint(vout: u32, tx: &bitcoin::Transaction) -> OutPoint {
    OutPoint { vout, txid: tx.compute_txid() }
}

pub fn script_grep<'a>(
    script_pubkey: &'a ScriptBuf,
    block: &'a bitcoin::Block,
) -> Vec<(OutPoint, TxOut)> {
    let mut ret = vec![];
    // ignore first transaction in block as coin-base
    for tx in block.txdata.iter().skip(1) {
        for (i, out) in tx.output.iter().enumerate() {
            if out.script_pubkey == *script_pubkey {
                let output = output(i, tx);
                let vout: u32 = i.try_into().unwrap();
                let outpoint = outpoint(vout, tx);
                ret.push((outpoint, output))
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
    mut data_file: File,
    spend_script: &ScriptBuf,
    outputs_path: &Path,
    spents_path: &Path,
) -> std::io::Result<()> {
    let mut blockhash_map = HashMap::new();
    let count = data_file.metadata()?.len();
    assert!(count % 32 == 0);
    let block_height = (count / 32) as i64;
    println!("stored block height: {:?}", block_height);

    if block_height == 0 {
        let genesis_hash = "00000008819873e925422c1ff0f99f7cc9bbb232af63a077a480a3633bee1ef6";
        let genesis_block_hash: BlockHash = genesis_hash.parse().unwrap();
        data_file.write_all(&genesis_block_hash.to_byte_array()).unwrap();
        blockhash_map.insert(genesis_block_hash, 0i64);
    } else {
        let mut buf = [0; 32];
        let mut read_count = 0;
        while read_count < block_height {
            data_file.read_exact(&mut buf).unwrap();
            let stored_hash: BlockHash = BlockHash::from_slice(&buf).unwrap();

            println!(
                "reading hash from file at height: {:?} with hash {:?}",
                read_count, stored_hash
            );
            blockhash_map.insert(stored_hash, read_count);
            read_count += 1;
        }
    }

    let version_message = build_version_message(address);
    let first_message = RawNetworkMessage::new(network.magic(), version_message);

    let mut blockhash_queue = vec![];
    let mut decoding_fails: u64 = 0;
    if let Ok(mut stream) = TcpStream::connect(address) {
        // Send the message
        let _ = stream.write_all(encode::serialize(&first_message).as_slice());
        println!("Sent version message");

        // Setup StreamReader
        let read_stream = stream.try_clone().unwrap();
        let mut stream_reader = BufReader::new(read_stream);

        loop {
            // Loop an retrieve new messages
            let reply = RawNetworkMessage::consensus_decode(&mut stream_reader);
            if let Ok(r) = reply {
                match r.payload() {
                    NetworkMessage::Block(block) => {
                        let header = block.header;
                        let block_hash = header.block_hash();
                        blockhash_queue.retain(|bh| *bh != block_hash);

                        let prev_blockhash = header.prev_blockhash;
                        if let Some(prev_height) = blockhash_map.get(&prev_blockhash) {
                            let height = prev_height + 1;
                            println!(
                                "recieved hash from peer at height: {:?} with hash {:?}",
                                height, block_hash
                            );

                            blockhash_map.entry(block_hash).insert_entry(height);
                            data_file.write_all(&block_hash.to_byte_array()).unwrap();

                            // check the block for any matching keys
                            let outs = script_grep(spend_script, block);

                            // write any of the keys and value
                            output_ledger::append_entries(outputs_path, outs);
                            let ledger = output_ledger::read(outputs_path);

                            // check if any of the outpoints are spent
                            let spent_outs = outpoint_grep(ledger, block);

                            spent_ledger::append_entries(spents_path, spent_outs);
                        } else {
                            println!(
                                "ignoring block {} recieved out of order preceeded by {}",
                                block_hash, prev_blockhash
                            );
                        }

                        if blockhash_queue.is_empty() {
                            println!("done, ready for next block");
                            let msg = build_headers_request(block_hash);
                            let _ = stream.write_all(encode::serialize(&msg).as_slice());
                        }
                    }
                    NetworkMessage::Headers(headers) => {
                        if headers.is_empty() {
                            println!("all caught up methinks");
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
                                    let known_hash = blockhash_map.contains_key(hash);
                                    if !known_hash {
                                        let inv_message = RawNetworkMessage::new(
                                            bitcoin::Network::Signet.magic(),
                                            NetworkMessage::GetData(inv_vec.clone()),
                                        );

                                        println!("req block: {}", hash);
                                        let _ = stream
                                            .write_all(encode::serialize(&inv_message).as_slice());
                                    }
                                }
                                _ => println!("unknown {:?}", inv),
                            }
                        }
                    }
                    NetworkMessage::Version(_) => {
                        println!("Received version message: {:?}", r.payload());

                        let second_message = RawNetworkMessage::new(
                            bitcoin::Network::Signet.magic(),
                            NetworkMessage::Verack,
                        );

                        let _ = stream.write_all(encode::serialize(&second_message).as_slice());
                        println!("Sent verack message");
                    }
                    NetworkMessage::Verack => {
                        // read last
                        let mut stored_hashes: Vec<_> = blockhash_map.iter().collect();
                        stored_hashes.sort_by(|a, b| b.1.cmp(a.1));
                        let prev_height = stored_hashes[0].1;
                        let prev_hash = stored_hashes[0].0;
                        println!("result from block height {} {:?}", prev_height, prev_hash);

                        let msg = build_headers_request(*prev_hash);
                        let _ = stream.write_all(encode::serialize(&msg).as_slice());
                    }
                    NetworkMessage::Ping(nonce) => {
                        let pong = NetworkMessage::Pong(*nonce);
                        let reply = RawNetworkMessage::new(bitcoin::Network::Signet.magic(), pong);
                        let _ = stream.write_all(encode::serialize(&reply).as_slice());
                        println!("sent Pong {:?}", nonce);
                    }
                    _ => {
                        println!("Received unknown message: {:?}", r.payload());
                    }
                }
            } else {
                decoding_fails += 1;
                println!("error decoding raw network message {:?} {} times", reply, decoding_fails);
                if decoding_fails > 100 {
                    let _ = stream.shutdown(Shutdown::Both);
                }
            }
        }
    } else {
        eprintln!("Failed to open connection");
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
    let args: Vec<String> = env::args().collect();

    if args.len() < 6 {
        eprintln!("Usage: {} <node_ip_address> <path_to_data_dir> <keys_file> <outputs_file> <spents_file>", args[0]);
        process::exit(1);
    }

    let str_address = &args[1];
    let address: SocketAddr = str_address.parse().unwrap_or_else(|error| {
        eprintln!("Error parsing address: {:?}", error);
        process::exit(1);
    });

    let str_data_file = args[2].clone();
    let data_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true) // Creates the file if it doesn't exist
        .truncate(false)
        .open(str_data_file)?;

    let str_keys_path = &args[3];
    let spend_script = read_pubkey_from_file(str_keys_path);

    let str_outputs_path = &args[4];
    let outputs_path = Path::new(&str_outputs_path);

    let str_spents_path = &args[5];
    let spents_path = Path::new(&str_spents_path);

    assert!(outputs_path != spents_path);

    let _ = connect(Network::Signet, address, data_file, &spend_script, outputs_path, spents_path)
        .await;

    Ok(())
}
