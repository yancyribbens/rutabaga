use bitcoin::key::Keypair;
use bitcoin::secp256k1::rand;
use bitcoin::secp256k1::{Secp256k1, SecretKey};
use bitcoin::{Address, Network};
use clap::Parser;
use rutabaga::coin::from_ledger;
use rutabaga::{output_ledger, spent_ledger};
use std::path::PathBuf;
use std::{fs, io::Write};

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

                let script_pubkey = &txout.script_pubkey;
                let address = Address::from_script(script_pubkey, Network::Signet).unwrap();
                let output = (coin.outpoint, txout, address);

                println!();
                println!("{}: {:#?}", i, output);
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
    }
}
