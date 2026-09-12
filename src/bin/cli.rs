use bitcoin::absolute::LockTime;
use bitcoin::hashes::Hash;
use bitcoin::key::{Keypair, TapTweak, TweakedKeypair};
use bitcoin::secp256k1::rand;
use bitcoin::secp256k1::{Message, Secp256k1, SecretKey};
use bitcoin::sighash::{Prevouts, SighashCache};
use bitcoin::transaction::Version;
use bitcoin::{
    Address, Amount, FeeRate, Network, ScriptBuf, Sequence, TapSighashType, Transaction, TxIn,
    TxOut, Witness,
};
use clap::Parser;
use esplora_client::Builder;
use rutabaga::coin;
use rutabaga::coin::Coin;
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

fn build_tx(coin: Coin, recipient: ScriptBuf, kp: Keypair, fee_rate: FeeRate) -> Transaction {
    let input = TxIn {
        previous_output: coin.outpoint,
        script_sig: ScriptBuf::new(),
        sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
        witness: Witness::default(),
    };

    let output = TxOut { value: coin.tx_out.value, script_pubkey: recipient.clone() };

    let mut tx = Transaction {
        version: Version::TWO,
        lock_time: LockTime::ZERO,
        input: vec![input],
        output: vec![output],
    };

    // now that the weight is known, the fee can be calculated.
    let fee: Amount = FeeRate::fee_wu(fee_rate, tx.weight()).unwrap();

    // now that the fee is known, update the transaction to include a fee.
    let output = TxOut { value: coin.tx_out.value - fee, script_pubkey: recipient.clone() };
    tx.output = vec![output];

    let input_index = 0;
    let sighash_type = TapSighashType::Default;
    let prevouts = vec![coin.tx_out];
    let prevouts = Prevouts::All(&prevouts);
    let mut sighasher = SighashCache::new(&mut tx);

    let sighash =
        sighasher.taproot_key_spend_signature_hash(input_index, &prevouts, sighash_type).unwrap();

    let s = Secp256k1::new();
    let tweaked: TweakedKeypair = kp.tap_tweak(&s, None);
    let msg = Message::from_digest(sighash.to_byte_array());
    let signature = s.sign_schnorr(&msg, &tweaked.to_keypair());

    let signature = bitcoin::taproot::Signature { signature, sighash_type };
    sighasher.witness_mut(input_index).unwrap().push(&signature.to_vec());
    let tx = sighasher.into_transaction();
    tx.to_owned()
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
            let coins = coin::from_ledger(&output_ledger, &spent_ledger);

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
        Commands::Wallet(WalletCmd::PrintBalance { output_ledger, spent_ledger }) => {
            let coins = coin::from_ledger(&output_ledger, &spent_ledger);
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

            let coins = coin::from_ledger(&outs_ledger, &spent_ledger);
            let coin = coins[index].clone();

            let address: Address =
                Address::from_str(&addr).unwrap().require_network(Network::Signet).unwrap();
            let tx = build_tx(coin, address.script_pubkey(), kp, bitcoin_fee_rate);
            let builder = Builder::new("https://blockstream.info/signet/api");
            let blocking_client = builder.build_blocking();
            let response = blocking_client.broadcast(&tx).unwrap();
            println!("{:#?}", response);
        }
    }
}
