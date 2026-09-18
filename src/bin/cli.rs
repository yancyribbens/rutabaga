use bitcoin::absolute::LockTime;
use bitcoin::hashes::Hash;
use bitcoin::key::Keypair;
use bitcoin::key::TapTweak;
use bitcoin::key::TweakedKeypair;
use bitcoin::secp256k1::{rand, Message};
use bitcoin::secp256k1::{Secp256k1, SecretKey};
use bitcoin::sighash::Prevouts;
use bitcoin::sighash::SighashCache;
use bitcoin::transaction::Version;
use bitcoin::TapSighashType;
use bitcoin::{Address, Network};
use bitcoin::{Amount, FeeRate, OutPoint, ScriptBuf, Sequence, Transaction, TxIn, TxOut, Witness};
use clap::Parser;
use esplora_client::Builder;
use rutabaga::output_ledger;
use std::path::PathBuf;
use std::str::FromStr;
use std::{fs, io::Write};
use rutabaga::utxos_from_ledger;

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
    PrintLedger {
        outs_ledger: PathBuf,
        spent_ledger: PathBuf,
    },
    /// TODO
    SpendOutput {
        index: usize,
        outs_ledger: PathBuf,
        spent_ledger: PathBuf,
        keys_path: PathBuf,
        addr: String,
        fee_rate: u32,
    },
    /// TODO
    SpendOutputs {
        index_list: String,
        outs_ledger: PathBuf,
        spent_ledger: PathBuf,
        keys_path: PathBuf,
        addr: String,
        fee_rate: u32,
    },
    /// TODO
    Spend {
        amount: u64,
        ledger_path: PathBuf,
        keys_path: PathBuf,
        addr: String,
        fee_rate: u32,
    }
}

fn build_tx(
    outs: Vec<(OutPoint, TxOut)>,
    recipient: ScriptBuf,
    kp: Keypair,
    fee_rate: FeeRate,
) -> Transaction {
    let inputs = outs
        .iter()
        .map(|(outpoint, _)| TxIn {
            previous_output: *outpoint,
            script_sig: ScriptBuf::new(),
            sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
            witness: Witness::default(),
        })
        .collect();

    let prevouts: Vec<_> = outs.iter().map(|(_, tx_out)| tx_out).collect();
    let value = prevouts
        .iter()
        .map(|tx_out| tx_out.value)
        .try_fold(Amount::ZERO, Amount::checked_add)
        .unwrap();

    let output = TxOut {
        value,
        script_pubkey: recipient.clone(),
    };

    let mut tx = Transaction {
        version: Version::TWO,
        lock_time: LockTime::ZERO,
        input: inputs,
        output: vec![output],
    };

    // now that the weight is known, the fee can be calculated.
    let fee: Amount = FeeRate::fee_wu(fee_rate, tx.weight()).unwrap();

    // now that the fee is known, update the transaction to include a fee.
    let output = TxOut {
        value: value - fee,
        script_pubkey: recipient.clone(),
    };
    tx.output = vec![output];

    let mut sighasher = SighashCache::new(&mut tx);
    for (index, _) in outs.clone().into_iter().enumerate() {
        let sighash_type = TapSighashType::Default;
        //let prevouts = vec![tx_out];
        let prevouts = Prevouts::All(&prevouts);

        let sighash = sighasher
            .taproot_key_spend_signature_hash(index, &prevouts, sighash_type)
            .unwrap();

        let s = Secp256k1::new();
        let tweaked: TweakedKeypair = kp.tap_tweak(&s, None);
        let msg = Message::from_digest(sighash.to_byte_array());
        let signature = s.sign_schnorr(&msg, &tweaked.to_keypair());

        let signature = bitcoin::taproot::Signature {
            signature,
            sighash_type,
        };
        sighasher
            .witness_mut(index)
            .unwrap()
            .push(signature.to_vec());
    }
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
        Commands::Wallet(WalletCmd::PrintLedger {
            outs_ledger,
            spent_ledger,
        }) => {
            let utxos = utxos_from_ledger(&outs_ledger, &spent_ledger);

            for (i, u) in utxos.iter().enumerate() {
                println!("");
                println!("{}: {:#?}", i, u);
            }
        }
        Commands::Wallet(WalletCmd::SpendOutput {
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

            let utxos = utxos_from_ledger(&outs_ledger, &spent_ledger);

            let (outpoint, ref output) = utxos[index];
            let out = vec![(outpoint, output.clone())];
            let address: Address = Address::from_str(&addr)
                .unwrap()
                .require_network(Network::Signet)
                .unwrap();
            let tx = build_tx(out, address.script_pubkey(), kp, bitcoin_fee_rate);
            let builder = Builder::new("https://blockstream.info/signet/api");
            let blocking_client = builder.build_blocking();
            let response = blocking_client.broadcast(&tx).unwrap();
            println!("{:#?}", response);
        }
        Commands::Wallet(WalletCmd::SpendOutputs {
            index_list,
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

            let utxos = utxos_from_ledger(&outs_ledger, &spent_ledger);

            let outs: Vec<_> = index_list
                .split(',')
                .map(|i| i.parse::<usize>().unwrap())
                .map(|i| utxos[i].clone())
                .collect();
            let address: Address = Address::from_str(&addr)
                .unwrap()
                .require_network(Network::Signet)
                .unwrap();
            let tx = build_tx(outs, address.script_pubkey(), kp, bitcoin_fee_rate);
            let builder = Builder::new("https://blockstream.info/signet/api");
            let blocking_client = builder.build_blocking();
            let response = blocking_client.broadcast(&tx).unwrap();
            println!("{:#?}", response);
        }
        Commands::Wallet(WalletCmd::Spend {
            amount,
            ledger_path,
            keys_path,
            addr,
            fee_rate,
        }) => {
            //let s = Secp256k1::new();
            //let bytes: Vec<u8> = fs::read(&keys_path).unwrap();
            //let sk = SecretKey::from_slice(&bytes).unwrap();
            //let kp = Keypair::from_secret_key(&s, &sk);
            //let bitcoin_fee_rate = FeeRate::from_sat_per_vb_u32(fee_rate);

            let outs = output_ledger::read(&ledger_path);
            //let outs: Vec<_> = index_list
                //.split(',')
                //.map(|i| i.parse::<usize>().unwrap())
                //.map(|i| outs[i].clone())
                //.collect();
            //let address: Address = Address::from_str(&addr)
                //.unwrap()
                //.require_network(Network::Signet)
                //.unwrap();
            //let tx = build_tx(outs, address.script_pubkey(), kp, bitcoin_fee_rate);
            //let builder = Builder::new("https://blockstream.info/signet/api");
            //let blocking_client = builder.build_blocking();
            //let response = blocking_client.broadcast(&tx).unwrap();
            //println!("{:#?}", response);
        }
    }
}
