pub mod output_ledger;
pub mod spent_ledger;

const PROTOCOL_VERSION: u32 = 70015;

use std::io::{Read, Seek, SeekFrom};
use std::{env, fs, process};
use std::fs::{File, OpenOptions};

use bitcoinkernel::{Block};
use bitcoin::{BlockHash, Network, OutPoint, ScriptBuf, TxOut};
use bitcoin::key::Keypair;
use bitcoin::secp256k1::{Secp256k1, SecretKey};

use bitcoin::hashes::Hash;

use bitcoin::p2p::{self, address, message_network};
use bitcoin::p2p::message::{NetworkMessage, RawNetworkMessage};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{SystemTime, UNIX_EPOCH};

use std::net::{Shutdown, TcpStream};

use std::io::{BufReader, Write};
use bitcoin::consensus::{encode, Decodable};

use bitcoin::secp256k1::rand::Rng;

use bitcoin::p2p::message_blockdata::{Inventory, GetHeadersMessage};
use bitcoin::blockdata::block::Header;

// Create rust-bitcoin TxOut from kernel parts
fn output<'a>(index: usize, tx_ref: &'a bitcoin::Transaction) -> TxOut {
    let tx = tx_ref.to_owned();
    let mut outputs = tx.output.iter();
    let output = outputs.nth(index).unwrap();
    let script_pubkey = output.script_pubkey.clone();
    let value = output.value;
    TxOut { value, script_pubkey }
}

// Create rust-bitcoin OutPoint from kernel parts
fn outpoint<'a>(vout: u32, tx: &'a bitcoin::Transaction) -> OutPoint {
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

pub fn outpoint_grep(
    stored_outs: Vec<(OutPoint, TxOut)>,
    block: &bitcoin::Block,
) -> Vec<OutPoint> {
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

fn read_pubkey_from_file() -> ScriptBuf {
    let s = Secp256k1::new();
    let keys_file = env::var("RUTABAGA_KEY_FILE")
        .expect("key file RUTABAGA_KEY_FILE should be set in env before running");
    let bytes: Vec<u8> =
        fs::read(keys_file).expect("the env var RUTABAGA_KEY_FILE should be readable");
    let sk = SecretKey::from_slice(&bytes).unwrap();
    let kp = Keypair::from_secret_key(&s, &sk);
    ScriptBuf::new_p2tr(&s, kp.x_only_public_key().0, None)
}

async fn run_connection(network: Network, address: SocketAddr, mut data_file: File) -> std::io::Result<()> {
    // TODO
    // read data_file, and if one exists, use the last hash after the ACK to request next blocks
    let mut headers_queue = vec![];

    let script = read_pubkey_from_file();

    let ledger_file = env::var("RUTABAGA_LEDGER_FILE")
        .expect("ledger file RUTABAGA_LEDGER_FILE should be set in env before running");

    let spent_file = env::var("RUTABAGA_SPENT_FILE")
        .expect("ledger file RUTABAGA_LEDGER_FILE should be set in env before running");

    let ledger_path = std::path::Path::new(&ledger_file);
    let spent_path = std::path::Path::new(&spent_file);
    assert!(ledger_path != spent_path);

    println!("run connection");
    let version_message = build_version_message(address);
 
    let first_message = RawNetworkMessage::new(network.magic(), version_message);

    println!("send first message");
    if let Ok(mut stream) = TcpStream::connect(address) {
        // Send the message
        let _ = stream.write_all(encode::serialize(&first_message).as_slice());
        println!("Sent version message");

        // Setup StreamReader
        let read_stream = stream.try_clone().unwrap();
        let mut stream_reader = BufReader::new(read_stream);

        let mut block_count = 0;
        let mut last_blk_rcvd: Header;
        loop {
            // Loop an retrieve new messages
            let reply = RawNetworkMessage::consensus_decode(&mut stream_reader).unwrap();
            match reply.payload() {
                NetworkMessage::Block(block) => {
                    let header = block.header;
                    data_file.write_all(&header.block_hash().to_byte_array()).unwrap();
                    
                    block_count += 1;
                    println!("parse block {:?}", block_count);

                    let last_blk_rcvd = header;
                    headers_queue.retain(|h| *h != header); 

                    // check the block for any matching keys
                    let outs = script_grep(&script, block);
                    // write any of the keys and value 
                    output_ledger::append_entries(ledger_path, outs);

                    let ledger = output_ledger::read(ledger_path);
                    // check if any of the outpoints are spent
                    let spent_outs = outpoint_grep(ledger, &block);
                    if !spent_outs.is_empty() {
                        println!("{:?}", spent_outs);
                    }

                    spent_ledger::append_entries(spent_path, spent_outs);
                    
                    if headers_queue.is_empty() {
                        println!("done, ready for next set");
                        let last_block_hash: BlockHash = last_blk_rcvd.block_hash();
                        let stop_hash: BlockHash = BlockHash::from_slice(&[0; 32]).unwrap();

                        let get_headers_message = GetHeadersMessage {
                            version: PROTOCOL_VERSION,
                            locator_hashes: vec![last_block_hash],
                            stop_hash
                        };

                        let get_headers = RawNetworkMessage::new(
                            bitcoin::Network::Signet.magic(),
                            NetworkMessage::GetHeaders(get_headers_message)
                        );
                        let _ = stream.write_all(encode::serialize(&get_headers).as_slice());
                    }
                }
                NetworkMessage::Headers(headers) => {
                    let mut queue: Vec<Header> = headers.clone();
                    let inv_vec: Vec<Inventory> =
                        headers.iter().map(|h| h.block_hash()).map(|h| Inventory::Block(h)).collect();
                    
                    let inv_message = RawNetworkMessage::new(
                        bitcoin::Network::Signet.magic(),
                        NetworkMessage::GetData(inv_vec.clone())
                    );

                    headers_queue.append(&mut queue);

                    let _ = stream.write_all(encode::serialize(&inv_message).as_slice());
                }
                NetworkMessage::Inv(inv_vec) => {
                    for inv in inv_vec {
                        match inv {
                            Inventory::Block(hash) => {
                                // TODO check if I have this hash/blockheader one or not
                                
                                let inv_message = RawNetworkMessage::new(
                                    bitcoin::Network::Signet.magic(),
                                    NetworkMessage::GetData(inv_vec.clone())
                                );

                                //TODO append hash as waiting to get
                                //headers_queue.append(&mut queue);
                                println!("req block: {}", hash);
                                let _ = stream.write_all(encode::serialize(&inv_message).as_slice());
                            },
                            _ => println!("unknown {:?}", inv)
                        }
                    }
                }
                NetworkMessage::Version(_) => {
                    println!("Received version message: {:?}", reply.payload());

                    let second_message = RawNetworkMessage::new(
                        bitcoin::Network::Signet.magic(),
                        NetworkMessage::Verack,
                    );

                    let _ = stream.write_all(encode::serialize(&second_message).as_slice());
                    println!("Sent verack message");
                }
                NetworkMessage::Verack => {
                    let size = data_file.metadata()?.len();
                    if size < 32 {
                        let genesis_hash = "00000008819873e925422c1ff0f99f7cc9bbb232af63a077a480a3633bee1ef6";
                        let genesis_block_hash: BlockHash = genesis_hash.parse().unwrap();

                        let stop_hash: BlockHash = BlockHash::from_slice(&[0; 32]).unwrap();
                        let get_headers_message = GetHeadersMessage {
                            version: PROTOCOL_VERSION,
                            locator_hashes: vec![genesis_block_hash],
                            stop_hash
                        };

                        let get_headers = RawNetworkMessage::new(
                            bitcoin::Network::Signet.magic(),
                            NetworkMessage::GetHeaders(get_headers_message)
                        );
                        let _ = stream.write_all(encode::serialize(&get_headers).as_slice());
                    } else {
                        println!("headers exist");
                        data_file.seek(SeekFrom::End(-32))?;
                        let mut buf = [0u8; 32];
                        data_file.read_exact(&mut buf)?;

                        let prev_hash: BlockHash = BlockHash::from_slice(&buf).unwrap();

                        let stop_hash: BlockHash = BlockHash::from_slice(&[0; 32]).unwrap();
                        let get_headers_message = GetHeadersMessage {
                            version: PROTOCOL_VERSION,
                            locator_hashes: vec![prev_hash],
                            stop_hash
                        };

                        let get_headers = RawNetworkMessage::new(
                            bitcoin::Network::Signet.magic(),
                            NetworkMessage::GetHeaders(get_headers_message)
                        );
                        let _ = stream.write_all(encode::serialize(&get_headers).as_slice());
                    }
                }
                NetworkMessage::Ping(nonce) => {
                    let pong = NetworkMessage::Pong(*nonce);
                    let reply = RawNetworkMessage::new(
                        bitcoin::Network::Signet.magic(),
                        pong
                    );
                    let _ = stream.write_all(encode::serialize(&reply).as_slice());
                    println!("sent Pong {:?}", nonce);
                }
                _ => {
                    println!("Received unknown message: {:?}", reply.payload());
                }
            }
        }
        let _ = stream.shutdown(Shutdown::Both);
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
        relay: false
    })
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        eprintln!("Usage: {} <path_to_data_dir> <network_address>", args[0]);
        process::exit(1);
    }

    let str_address = &args[2];
    let address: SocketAddr = str_address.parse().unwrap_or_else(|error| {
        eprintln!("Error parsing address: {:?}", error);
        process::exit(1);
    });

    let data_file = args[1].clone();
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true) // Creates the file if it doesn't exist
        .open(data_file)?;

    run_connection(Network::Signet, address, file).await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use bitcoin::{OutPoint, TxOut};
    use bitcoin::consensus::{Decodable};
    use bitcoinkernel::{Transaction, TransactionRef};
    use std::fs;

    use crate::output_ledger;

    pub fn get_transaction(file: &str) -> bitcoin::Transaction {
        let file = format!("tests/transaction_{file}.bin");
        let tx_data = fs::read(file).unwrap();
        let tx: bitcoin::Transaction = Decodable::consensus_decode(&mut tx_data.as_slice()).unwrap();

        tx
    }

    pub fn outs_from_tx(tx: &bitcoin::Transaction) -> Vec<(OutPoint, TxOut)> {
        let mut ret = vec![];
        for (i, output) in tx.output.iter().enumerate() {
            println!("val {:?}", output.value);
            let vout: u32 = i.try_into().unwrap();
            let outpoint: OutPoint = OutPoint { txid: tx.compute_txid(), vout };
            ret.push((outpoint, output.clone()));
        }
        ret
    }

    pub fn write_tx_outs_to_file(tx: &bitcoin::Transaction, path: &std::path::Path) {
        let outs = outs_from_tx(tx);
        output_ledger::append_entries(path, outs)
    }
}
