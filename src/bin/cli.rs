use bitcoin::key::Keypair;
use bitcoin::secp256k1::rand;
use bitcoin::secp256k1::{Secp256k1, SecretKey};
use bitcoin::{Address, Amount, FeeRate, Network};
use clap::Parser;
use esplora_client::Builder;
use rutabaga::coin::from_ledger;
use rutabaga::transaction_builder::build;
use rutabaga::{output_ledger, spent_ledger};
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::str::FromStr;

#[derive(clap::Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    commands: Commands,
}

#[derive(Debug, Clone, clap::Subcommand)]
enum Commands {
    /// Wallet commands.
    #[command(subcommand)]
    Wallet(WalletCmd),
}

#[derive(Debug, Clone, clap::Subcommand)]
enum WalletCmd {
    /// TODO
    GenerateAddress {
        /// TODO
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// TODO
    PrintKeysFromKeysFile { path: PathBuf },
    /// TODO
    PrintOutputs { path: PathBuf },
    /// TODO
    PrintUtxos { output_ledger: PathBuf, spent_ledger: PathBuf },
    /// TODO
    PrintSpentOutputs { path: PathBuf },
    /// TODO
    PrintBalance { output_ledger: PathBuf, spent_ledger: PathBuf },
    /// TODO
    SpendUtxo {
        index: usize,
        outs_ledger: PathBuf,
        spent_ledger: PathBuf,
        keys_path: PathBuf,
        addr: String,
        fee_rate: u32,
    },
}

fn main() {
    let cli = Args::parse();

    match cli.commands {
        Commands::Wallet(WalletCmd::GenerateAddress { out }) => {
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
        Commands::Wallet(WalletCmd::PrintKeysFromKeysFile { path }) => {
            let s = Secp256k1::new();
            let bytes: Vec<u8> = fs::read(path).unwrap();
            let sk = SecretKey::from_slice(&bytes).unwrap();
            let kp = Keypair::from_secret_key(&s, &sk);
            let address = Address::p2tr(&s, kp.x_only_public_key().0, None, Network::Signet);
            println!("{:?}", address);
            println!("{}", kp.secret_key().display_secret());
        }
        Commands::Wallet(WalletCmd::PrintOutputs { path }) => {
            let outs = output_ledger::read(&path);
            println!("output count: {:?}", outs.len());

            for out in outs {
                println!();
                println!("{:#?}", out);
            }
        }
        Commands::Wallet(WalletCmd::PrintUtxos { output_ledger, spent_ledger }) => {
            let coins = from_ledger(&output_ledger, &spent_ledger);

            for (i, coin) in coins.iter().enumerate() {
                let txout = coin.tx_out.clone();
                println!("{}: {:#?}", i, txout);
            }
        }
        Commands::Wallet(WalletCmd::PrintSpentOutputs { path }) => {
            let spents = spent_ledger::read(&path);
            println!("count: {:?}", spents.len());

            for s in spents {
                println!();
                println!("{:#?}", s);
            }
        }
        Commands::Wallet(WalletCmd::PrintBalance { output_ledger, spent_ledger }) => {
            let coins = from_ledger(&output_ledger, &spent_ledger);
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
        Commands::Wallet(WalletCmd::SpendUtxo {
            index,
            outs_ledger,
            spent_ledger,
            keys_path,
            addr,
            fee_rate,
        }) => {
            let s = Secp256k1::new();
            let bytes: Vec<u8> = fs::read(&keys_path).unwrap();
            let sk = SecretKey::from_slice(&bytes).unwrap();
            let kp = Keypair::from_secret_key(&s, &sk);
            let bitcoin_fee_rate = FeeRate::from_sat_per_vb_u32(fee_rate);

            let coins = from_ledger(&outs_ledger, &spent_ledger);
            let coin = coins[index].clone();

            let address: Address =
                Address::from_str(&addr).unwrap().require_network(Network::Signet).unwrap();
            let tx = build(&coin, &address.script_pubkey(), kp, bitcoin_fee_rate).unwrap();
            let builder = Builder::new("https://blockstream.info/signet/api");
            let blocking_client = builder.build_blocking();
            let response = blocking_client.broadcast(&tx).unwrap();
            println!("{:#?}", response);
        }
    }
}
