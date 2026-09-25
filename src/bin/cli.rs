use bitcoin::key::Keypair;
use bitcoin::secp256k1::rand;
use bitcoin::secp256k1::{Secp256k1, SecretKey};
use bitcoin::{Address, Amount, FeeRate, Network, ScriptBuf, Weight};
use clap::Parser;
use esplora_client::Builder;
use rutabaga::coin;
use rutabaga::transaction_builder;
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
    PrintUtxos {
        output_ledger: PathBuf,
        spent_ledger: PathBuf,
    },
    /// TODO
    PrintSpentOutputs { path: PathBuf },
    /// TODO
    PrintBalance {
        output_ledger: PathBuf,
        spent_ledger: PathBuf,
    },
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
        Commands::Wallet(WalletCmd::PrintUtxos {
            output_ledger,
            spent_ledger,
        }) => {
            let coins = coin::from_ledger(&output_ledger, &spent_ledger);

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
        Commands::Wallet(WalletCmd::PrintBalance {
            output_ledger,
            spent_ledger,
        }) => {
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

            let address: Address = Address::from_str(&addr)
                .unwrap()
                .require_network(Network::Signet)
                .unwrap();
            let tx = transaction_builder::build_without_change(
                &coin_vec,
                &address.script_pubkey(),
                kp,
                bitcoin_fee_rate,
            )
            .unwrap();
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
            let address: Address = Address::from_str(&addr)
                .unwrap()
                .require_network(Network::Signet)
                .unwrap();
            let tx = transaction_builder::build_without_change(
                &outs,
                &address.script_pubkey(),
                kp,
                bitcoin_fee_rate,
            )
            .unwrap();
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
            // cost of tx with two output and no inputs
            let tx_cost = Amount::from_sat(548);
            let target = Amount::from_sat(amount);
            let total_target = target + tx_cost;

            let bitcoin_fee_rate = FeeRate::from_sat_per_vb_u32(fee_rate);
            let discard_fee_rate = DEFAULT_DISCARD_FEE_RATE;
            let lt_fee_rate = DEFAULT_LONG_TERM_FEE_RATE;

            let coins = coin::from_ledger(&outs_ledger, &spent_ledger);
            println!("{:#?}", coins);
            let cost_of_change = default_tr_cost_of_change(bitcoin_fee_rate, discard_fee_rate);

            let (_, selection) = select_coins(
                total_target,
                cost_of_change,
                bitcoin_fee_rate,
                lt_fee_rate,
                &coins,
            )
            .unwrap();
            println!("selection {:?}", selection);
            let to_spend = selection.into_iter().cloned().collect();
            let address: Address = Address::from_str(&addr)
                .unwrap()
                .require_network(Network::Signet)
                .unwrap();

            let s = Secp256k1::new();
            let bytes: Vec<u8> = fs::read(&keys_path).unwrap();
            let sk = SecretKey::from_slice(&bytes).unwrap();
            let kp = Keypair::from_secret_key(&s, &sk);
            let sender_script_pub_key = ScriptBuf::new_p2tr(&s, kp.x_only_public_key().0, None);

            let tx = transaction_builder::build_with_change(
                &to_spend,
                target,
                address.script_pubkey(),
                sender_script_pub_key,
                kp,
                bitcoin_fee_rate,
            );

            let builder = Builder::new("https://blockstream.info/signet/api");
            let blocking_client = builder.build_blocking();
            let response = blocking_client.broadcast(&tx).unwrap();
            println!("{:#?}", response);
        }
    }
}
