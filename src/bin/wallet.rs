use bitcoin::secp256k1::rand;
use bitcoin::secp256k1::Secp256k1;
use bitcoin::{Address, Network};
use clap::Parser;
use std::path::PathBuf;
use std::{fs, io::Write};

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
    }
}
