use bitcoin::absolute::LockTime;
use bitcoin::hashes::Hash;
use bitcoin::key::{Keypair, TapTweak, TweakedKeypair};
use bitcoin::secp256k1::rand;
use bitcoin::secp256k1::{Message, Secp256k1, SecretKey};
use bitcoin::sighash::{Prevouts, SighashCache};
use bitcoin::transaction::Version;
use bitcoin::{
    Address, Amount, FeeRate, Network, ScriptBuf, Sequence, TapSighashType, Transaction, TxIn,
    TxOut, Weight, Witness,
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

use bitcoin_coin_selection::select_coins;

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
    /// TODO
    SpendUtxos {
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
        outs_ledger: PathBuf,
        spent_ledger: PathBuf,
        keys_path: PathBuf,
        addr: String,
        fee_rate: u32,
    },
}

fn build_tx(coins: Vec<Coin>, recipient: ScriptBuf, kp: Keypair, fee_rate: FeeRate) -> Transaction {
    let inputs = coins
        .iter()
        .map(|coin| TxIn {
            previous_output: coin.outpoint,
            script_sig: ScriptBuf::new(),
            sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
            witness: Witness::default(),
        })
        .collect();

    let prevouts: Vec<_> = coins.iter().map(|coin| coin.tx_out.clone()).collect();
    let value = prevouts
        .iter()
        .map(|tx_out| tx_out.value)
        .try_fold(Amount::ZERO, Amount::checked_add)
        .unwrap();

    let output = TxOut { value, script_pubkey: recipient.clone() };

    let mut tx = Transaction {
        version: Version::TWO,
        lock_time: LockTime::ZERO,
        input: inputs,
        output: vec![output],
    };

    // now that the weight is known, the fee can be calculated.
    let fee: Amount = FeeRate::fee_wu(fee_rate, tx.weight()).unwrap();

    // now that the fee is known, update the transaction to include a fee.
    let output = TxOut { value: value - fee, script_pubkey: recipient.clone() };
    tx.output = vec![output];

    let mut sighasher = SighashCache::new(&mut tx);
    for (index, _) in coins.clone().into_iter().enumerate() {
        let sighash_type = TapSighashType::Default;
        //let prevouts = vec![tx_out];
        let prevouts = Prevouts::All(&prevouts);

        let sighash =
            sighasher.taproot_key_spend_signature_hash(index, &prevouts, sighash_type).unwrap();

        let s = Secp256k1::new();
        let tweaked: TweakedKeypair = kp.tap_tweak(&s, None);
        let msg = Message::from_digest(sighash.to_byte_array());
        let signature = s.sign_schnorr(&msg, &tweaked.to_keypair());

        let signature = bitcoin::taproot::Signature { signature, sighash_type };
        sighasher.witness_mut(index).unwrap().push(signature.to_vec());
    }
    let tx = sighasher.into_transaction();
    tx.to_owned()
}

const DEFAULT_DISCARD_FEE_RATE: FeeRate = FeeRate::from_sat_per_vb_u32(10);
const DEFAULT_LONG_TERM_FEE_RATE: FeeRate = FeeRate::from_sat_per_vb_u32(10);

// 32 byte txid, 4 byte output index, 1 byte scriptSig, and 4 byte sequence
const BASE_WEIGHT: Weight = Weight::from_vb_unwrap(32 + 4 + 1 + 4);

// cost of change is the cost to create a change output plus the estimated cost to spend it as
// input.
//
// therefore, cost_of_change =
//      (change output size * fee rate) +
//      (change spend size * discard fee rate)
//
// The change output size of a TR output is 57.5 vB (230 wu)
// The change spend size is its input size in a future transaction, 43 vB (172 wu)
// Therefore, the total size estimate is 100.5 vB or 402 wu
//
// params
//  * fee_rate - current effective fee rate.
//  * discard_fee_rate - target fee rate is fee rate with which output will not be a dust output.
//    ref: core PR# 10817
fn default_tr_cost_of_change(fee_rate: FeeRate, discard_fee_rate: FeeRate) -> Amount {
    // output_size is 57.5 vB
    // the base_weight is 164 wu while the P2TR key-path is 66 WU totaling 230 WU
    let change_spend_size = BASE_WEIGHT + Weight::from_wu(66);
    let change_output_size = Weight::from_vb_unchecked(43);

    let change_fee = fee_rate * change_output_size;
    let min_viable_change = discard_fee_rate * change_spend_size;
    min_viable_change + change_fee
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
            let coin_vec = vec![coin];

            let address: Address =
                Address::from_str(&addr).unwrap().require_network(Network::Signet).unwrap();
            let tx = build_tx(coin_vec, address.script_pubkey(), kp, bitcoin_fee_rate);
            let builder = Builder::new("https://blockstream.info/signet/api");
            let blocking_client = builder.build_blocking();
            let response = blocking_client.broadcast(&tx).unwrap();
            println!("{:#?}", response);
        }
        Commands::Wallet(WalletCmd::SpendUtxos {
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

            let coins = coin::from_ledger(&outs_ledger, &spent_ledger);
            let outs: Vec<_> = index_list
                .split(',')
                .map(|i| i.parse::<usize>().unwrap())
                .map(|i| coins[i].clone())
                .collect();
            let address: Address =
                Address::from_str(&addr).unwrap().require_network(Network::Signet).unwrap();
            let tx = build_tx(outs, address.script_pubkey(), kp, bitcoin_fee_rate);
            let builder = Builder::new("https://blockstream.info/signet/api");
            let blocking_client = builder.build_blocking();
            let response = blocking_client.broadcast(&tx).unwrap();
            println!("{:#?}", response);
        }
        Commands::Wallet(WalletCmd::Spend {
            amount,
            outs_ledger,
            spent_ledger,
            keys_path,
            addr,
            fee_rate,
        }) => {
            let bitcoin_amount = Amount::from_sat(amount);
            let bitcoin_fee_rate = FeeRate::from_sat_per_vb_u32(fee_rate);
            let discard_fee_rate = DEFAULT_DISCARD_FEE_RATE;
            let lt_fee_rate = DEFAULT_LONG_TERM_FEE_RATE;

            let coins = coin::from_ledger(&outs_ledger, &spent_ledger);
            let cost_of_change = default_tr_cost_of_change(bitcoin_fee_rate, discard_fee_rate);

            let (_, selection) =
                select_coins(bitcoin_amount, cost_of_change, bitcoin_fee_rate, lt_fee_rate, &coins)
                    .unwrap();
            let to_spend = selection.into_iter().cloned().collect();
            let address: Address =
                Address::from_str(&addr).unwrap().require_network(Network::Signet).unwrap();

            let s = Secp256k1::new();
            let bytes: Vec<u8> = fs::read(&keys_path).unwrap();
            let sk = SecretKey::from_slice(&bytes).unwrap();
            let kp = Keypair::from_secret_key(&s, &sk);

            let tx = build_tx(to_spend, address.script_pubkey(), kp, bitcoin_fee_rate);
            let builder = Builder::new("https://blockstream.info/signet/api");
            let blocking_client = builder.build_blocking();
            let response = blocking_client.broadcast(&tx).unwrap();
            println!("{:#?}", response);
        }
    }
}
