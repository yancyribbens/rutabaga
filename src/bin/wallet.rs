use rutabaga::outs_file::OutsFile;
use rutabaga::spent_file::SpentFile;

use bitcoin::key::Keypair;
use bitcoin::secp256k1::rand;
use bitcoin::secp256k1::{Secp256k1, SecretKey};
use bitcoin::{Address, Amount, FeeRate, Network};
use clap::Parser;
use esplora_client::Builder;
use rutabaga::coin::from_outs;
use rutabaga::transaction_builder::build_without_change;
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::str::FromStr;

#[derive(clap::Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    commands: WalletCmd,
}

#[derive(Debug, Clone, clap::Subcommand)]
enum WalletCmd {
    /// Generate a new p2tr wallet address with secret key.
    GenerateAddress {
        /// Path to save newly created address and secret key.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Print address and secret key from file.
    PrintKeysFromKeysFile { path: PathBuf },
    /// Print all outputs stored in file.
    PrintOutputs { outs_path: PathBuf },
    /// Print unspent outputs by reading outputs file and spent file.
    PrintUtxos { outs_path: PathBuf, spent_path: PathBuf },
    /// Print spent outputs stored in file.
    PrintSpentOutputs { spent_path: PathBuf },
    /// Print the value stored in all unspent outputs.
    PrintBalance { outs_path: PathBuf, spent_path: PathBuf },
    /// Spend UTXOs given a comma separated list of indexes from the PrintUtxos command.
    SpendUtxos {
        index_list: String,
        outs_path: PathBuf,
        spent_path: PathBuf,
        keys_path: PathBuf,
        addr: String,
        fee_rate: u32,
    },
}

fn main() {
    let cli = Args::parse();

    match cli.commands {
        WalletCmd::GenerateAddress { out } => {
            let s = Secp256k1::new();
            let (priv_key, pub_key) = s.generate_keypair(&mut rand::thread_rng());
            let (internal_key, _parity) = pub_key.x_only_public_key();
            let address = Address::p2tr(&s, internal_key, None, Network::Signet);
            println!("{:?}", address);

            if let Some(o) = out {
                let mut file = fs::File::create_new(o).unwrap();
                file.write_all(&priv_key.secret_bytes()).unwrap();
            } else {
                let display = priv_key.display_secret();
                println!("secret_key={}", display);
            }
        }
        WalletCmd::PrintKeysFromKeysFile { path } => {
            let s = Secp256k1::new();
            let bytes: Vec<u8> = fs::read(path).unwrap();
            let sk = SecretKey::from_slice(&bytes).unwrap();
            let kp = Keypair::from_secret_key(&s, &sk);
            let address = Address::p2tr(&s, kp.x_only_public_key().0, None, Network::Signet);
            println!("{:?}", address);
            println!("{}", kp.secret_key().display_secret());
        }
        WalletCmd::PrintOutputs { outs_path } => {
            let outs = OutsFile::new(&outs_path).read();
            println!("output count: {:?}", outs.len());

            for out in outs {
                println!();
                println!("{:#?}", out);
            }
        }
        WalletCmd::PrintUtxos { outs_path, spent_path } => {
            let outs = OutsFile::new(&outs_path).read();
            let spent_outpoints = SpentFile::new(&spent_path).read();
            let coins = from_outs(outs, spent_outpoints);

            for (i, coin) in coins.iter().enumerate() {
                let txout = coin.tx_out.clone();
                println!("{}: {:#?}", i, txout);
            }
        }
        WalletCmd::PrintSpentOutputs { spent_path } => {
            let spent = SpentFile::new(&spent_path).read();
            println!("count: {:?}", spent.len());

            for s in spent {
                println!();
                println!("{:#?}", s);
            }
        }
        WalletCmd::PrintBalance { outs_path, spent_path } => {
            let outs_file = OutsFile::new(&outs_path).read();
            let spent_outpoints = SpentFile::new(&spent_path).read();
            let coins = from_outs(outs_file, spent_outpoints);
            let unique_addresses: Vec<_> = coins
                .iter()
                .map(|coin| {
                    let script_pubkey = &coin.tx_out.script_pubkey;
                    Address::from_script(script_pubkey, Network::Signet).unwrap()
                })
                .collect::<HashSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();

            let balance: Amount = coins.iter().map(|coin| coin.tx_out.value).sum();

            println!("UTXO count: {:?}", coins.len());
            println!("address count: {:?}", unique_addresses.len());
            for addr in unique_addresses {
                println!("{:?}", addr);
            }
            println!("ledger balance: {:?}", balance);
        }
        WalletCmd::SpendUtxos { index_list, outs_path, spent_path, keys_path, addr, fee_rate } => {
            let s = Secp256k1::new();
            let bytes: Vec<u8> = fs::read(&keys_path).unwrap();
            let sk = SecretKey::from_slice(&bytes).unwrap();
            let kp = Keypair::from_secret_key(&s, &sk);
            let bitcoin_fee_rate = FeeRate::from_sat_per_vb_u32(fee_rate);

            let outs = OutsFile::new(&outs_path).read();
            let spent_outpoints = SpentFile::new(&spent_path).read();
            let coins = from_outs(outs, spent_outpoints);
            let select_coins: Vec<_> = index_list
                .split(',')
                .map(|i| i.parse::<usize>().unwrap())
                .map(|i| coins[i].clone())
                .collect();
            let address: Address =
                Address::from_str(&addr).unwrap().require_network(Network::Signet).unwrap();
            let tx =
                build_without_change(&select_coins, &address.script_pubkey(), kp, bitcoin_fee_rate)
                    .unwrap();
            let builder = Builder::new("https://blockstream.info/signet/api");
            let blocking_client = builder.build_blocking();
            let response = blocking_client.broadcast(&tx).unwrap();
            println!("{:#?}", response);
        }
    }
}
