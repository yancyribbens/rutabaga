pub mod output_ledger;
pub mod spent_ledger;

use bitcoinkernel::{
    Block, BlockTreeEntry, ChainType, ChainstateManager, ChainstateManagerBuilder, Context,
    ContextBuilder,
};
use std::env;
use std::process;
use std::sync::Arc;

use std::fs;

use bitcoin::{Network, OutPoint, ScriptBuf, TxOut};
use bitcoinkernel::core::ScriptPubkeyExt;
use bitcoinkernel::core::TransactionExt;
use bitcoinkernel::core::TxOutExt;
use bitcoinkernel::TransactionRef;

use bitcoin::key::Keypair;
use bitcoin::secp256k1::{Secp256k1, SecretKey};

use bitcoin::hashes::Hash;

use bitcoinkernel::core::TxInExt;
use bitcoinkernel::core::TxOutPointExt;
use bitcoinkernel::core::TxidExt;

use bitcoin::p2p::{self, address, message, message_network};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{SystemTime, UNIX_EPOCH};

use std::net::{Shutdown, TcpStream};

use std::io::{BufReader, Write};
use bitcoin::consensus::{encode, Decodable};

use bitcoin::secp256k1::rand::Rng;

fn create_context() -> Arc<Context> {
    Arc::new(ContextBuilder::new().chain_type(ChainType::Regtest).build().unwrap())
}

pub fn script_grep<'a>(
    script_pubkey: &'a ScriptBuf,
    block: &'a bitcoinkernel::Block,
) -> Vec<(usize, TransactionRef<'a>)> {
    let mut ret = vec![];
    // ignore first transaction in block as coin-base
    for tx in block.transactions().skip(1) {
        for (i, out) in tx.outputs().enumerate() {
            if out.script_pubkey().to_bytes() == script_pubkey.to_bytes() {
                ret.push((i, tx))
            }
        }
    }
    ret
}

pub fn outpoint_grep(
    stored_outs: Vec<(OutPoint, TxOut)>,
    block: &bitcoinkernel::Block,
) -> Vec<OutPoint> {
    let stored_outpoints: Vec<_> = stored_outs.iter().map(|(outpoint, _)| *outpoint).collect();
    let mut spent_outs = vec![];
    for tx in block.transactions().skip(1) {
        for input in tx.inputs() {
            let outpoint = input.outpoint();
            let vout = outpoint.index();
            let txid = outpoint.txid();
            let txid = bitcoin::Txid::from_byte_array(txid.to_bytes());
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

fn scan_block(chainman: &ChainstateManager, block_index: &BlockTreeEntry) -> Result<(), u32> {
    let script = read_pubkey_from_file();
    let block: Block = chainman.read_block_data(block_index).unwrap();
    let outs = script_grep(&script, &block);

    let ledger_file = env::var("RUTABAGA_LEDGER_FILE")
        .expect("ledger file RUTABAGA_LEDGER_FILE should be set in env before running");

    let spent_file = env::var("RUTABAGA_SPENT_FILE")
        .expect("ledger file RUTABAGA_LEDGER_FILE should be set in env before running");

    let path = std::path::Path::new(&ledger_file);
    output_ledger::append(path, outs);

    let ledger = output_ledger::read(path);
    let spent_outs = outpoint_grep(ledger, &block);
    let spent_path = std::path::Path::new(&spent_file);
    spent_ledger::append(spent_path, spent_outs);

    Ok(())
}

fn scan_chain(chainman: &ChainstateManager) -> Result<(), u32> {
    let chain = chainman.active_chain();
    let tip_height = chain.height();

    println!("Starting scan from genesis to tip {}", tip_height);

    for block_index in chain.iter() {
        if block_index.height() % 10 == 0 {
            println!("Scanning block {} / {}", block_index.height(), tip_height);
        }
        scan_block(chainman, &block_index).unwrap();
    }

    Ok(())
}

async fn run_connection(network: Network, address: SocketAddr, chainman: ChainstateManager) -> std::io::Result<()> {
    println!("run connection");
    let version_message = build_version_message(address);
 
    let first_message =
        message::RawNetworkMessage::new(network.magic(), version_message);

    println!("send first message");
    if let Ok(mut stream) = TcpStream::connect(address) {
        // Send the message
        let _ = stream.write_all(encode::serialize(&first_message).as_slice());
        println!("Sent version message");

        // Setup StreamReader
        let read_stream = stream.try_clone().unwrap();
        let mut stream_reader = BufReader::new(read_stream);
        loop {
            // Loop an retrieve new messages
            let reply = message::RawNetworkMessage::consensus_decode(&mut stream_reader).unwrap();
            match reply.payload() {
                message::NetworkMessage::Version(_) => {
                    println!("Received version message: {:?}", reply.payload());

                    let second_message = message::RawNetworkMessage::new(
                        bitcoin::Network::Bitcoin.magic(),
                        message::NetworkMessage::Verack,
                    );

                    let _ = stream.write_all(encode::serialize(&second_message).as_slice());
                    println!("Sent verack message");
                }
                message::NetworkMessage::Verack => {
                    println!("Received verack message: {:?}", reply.payload());
                    break;
                }
                _ => {
                    println!("Received unknown message: {:?}", reply.payload());
                    break;
                }
            }
        }
        let _ = stream.shutdown(Shutdown::Both);
    } else {
        eprintln!("Failed to open connection");
    }

    Ok(())
}

fn build_version_message(address: SocketAddr) -> message::NetworkMessage {
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
    let user_agent = String::from("rudabaga");

    // "The last block received by the emitting node"
    let start_height: i32 = 0;

    // Construct the message
    message::NetworkMessage::Version(message_network::VersionMessage::new(
        services,
        timestamp as i64,
        addr_recv,
        addr_from,
        nonce,
        user_agent,
        start_height,
    ))
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

    let context = create_context();
    let data_dir = args[1].clone();
    let blocks_dir = format!("{}/blocks", data_dir);

    let chainman =
        ChainstateManagerBuilder::new(&context, &data_dir, &blocks_dir).unwrap().build().unwrap();

    chainman.import_blocks().unwrap();

    let _ = scan_chain(&chainman);

    run_connection(Network::Signet, address, chainman).await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use bitcoinkernel::{Transaction, TransactionRef};
    use std::fs;

    use crate::output_ledger;

    pub fn get_transaction(file: &str) -> Transaction {
        let file = format!("tests/transaction_{file}.bin");
        let tx_data = fs::read(file).unwrap();
        Transaction::new(&tx_data).unwrap()
    }

    pub fn outs_from_tx(tx: &Transaction) -> Vec<(usize, TransactionRef<'_>)> {
        let tx_ref = tx.as_ref();
        // the tx has only one output that's not the coinbase output
        vec![(1, tx_ref)]
    }

    pub fn write_tx_outs_to_file(tx: &Transaction, path: &std::path::Path) {
        let outs = outs_from_tx(tx);
        output_ledger::append(path, outs)
    }
}
