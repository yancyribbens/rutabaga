use bitcoin::secp256k1::rand;
use bitcoin::key::{Keypair};
use bitcoin::secp256k1::{Secp256k1, SecretKey};
use bitcoin::{Address, Network};
use clap::Parser;
use std::path::PathBuf;
use std::{fs, io::Write};
use rutabaga::output_ledger;
use bitcoin::{OutPoint, Sequence, ScriptBuf, Transaction, TxIn, TxOut, Witness};
use bitcoin::transaction::Version;
use bitcoin::absolute::LockTime;
use std::str::FromStr;
//use bitcoin::{Address, Network};
//use bitcoin::address::{NetworkUnchecked, NetworkChecked};

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
    PrintLedger { path: PathBuf },
    /// TODO
    SpendOutput { index: usize, path: PathBuf, addr: String}
}

fn build_tx(outpoint: OutPoint, tx_out: &TxOut, recipient: ScriptBuf) -> Transaction {
    let input = TxIn {
        previous_output: outpoint,
        script_sig: ScriptBuf::new(),
        sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
        witness: Witness::new()
    };

    let fee = bitcoin::Amount::from_sat(100);
    let output = TxOut {
        value: tx_out.value - fee,
        script_pubkey: recipient 
    };

    Transaction {
        version: Version::TWO,
        lock_time: LockTime::ZERO,
        input: vec![input],
        output: vec![output]
    }
}

use esplora_client::Builder;

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
        Commands::Wallet(WalletCmd::PrintLedger { path }) => {
            let outs = output_ledger::read(&path); 
            println!("count: {:?}", outs.len());

            for (i, out) in outs.into_iter().enumerate() {
                let (outpoint, ref txout) = out;
                let script_pubkey = &txout.script_pubkey;
                let address = Address::from_script(script_pubkey, Network::Signet).unwrap();
                let output = (outpoint, txout, address);
                println!();
                println!("output: {}", i);
                println!("{:#?}", output);
            }
        }
        Commands::Wallet(WalletCmd::SpendOutput { index, path, addr }) => {
            let outs = output_ledger::read(&path); 
            let (outpoint, ref output) = outs[index];
            let address: Address = Address::from_str(&addr).unwrap()
               .require_network(Network::Signet).unwrap();
            let tx = build_tx(outpoint, output, address.script_pubkey());
            let builder = Builder::new(&Network::Signet.to_string());
            let blocking_client = builder.build_blocking();
            let response = blocking_client.broadcast(&tx).unwrap();
            println!("{:#?}", response);
        }
    }
}
