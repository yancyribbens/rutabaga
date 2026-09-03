use crate::output_ledger;

#[derive(Debug)]
pub struct Wallet {
    pub scan_height: u32
}
pub struct Error {}

use bitcoin::key::{Keypair};
use bitcoin::secp256k1::{SecretKey, XOnlyPublicKey};
use bitcoin::{Amount, FeeRate, ScriptBuf};
use bitcoin::transaction::Version;
use bitcoin::absolute::LockTime;
use bitcoin::Address;
use bitcoin::secp256k1::{Secp256k1};
use std::fmt;
use bitcoinkernel::core::TransactionExt;
use bitcoinkernel::core::TxOutExt;
use bitcoinkernel::core::ScriptPubkeyExt;
use std::fs;
use bitcoinkernel::TransactionRef;
use std::env;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Error")
    }
}

impl Error {
    pub fn is_user_error(&self) -> bool {
        false
    }
}

impl Default for Wallet {
    fn default() -> Self {
        Self::new()
    }
}

impl Wallet {
    pub fn new() -> Self {
        Self{ scan_height: 0 }
    }

    pub fn build_transaction(
        &self,
        _recipient: Address,
        _amount: Amount,
        _fee_rate: FeeRate,
        _long_term_fee_rate: FeeRate,
    ) -> Result<bitcoin::Transaction, Error> {
        let input = vec![];
        let output = vec![];

        let tx = bitcoin::Transaction {
            version: Version::TWO,
            lock_time: LockTime::from_height(0).unwrap_or(LockTime::ZERO),
            input,
            output,
        };
        Ok(tx)
    }

    pub fn process_disconnect(&mut self, _kernel_block: bitcoinkernel::Block) {}

    pub fn release_coins(&mut self, _outpoints: impl IntoIterator<Item = bitcoin::OutPoint>) {
        let v: Vec<bitcoin::OutPoint> = vec![];
        v.into_iter();
    }

    fn read_pubkey_from_file() -> ScriptBuf {
        let s = Secp256k1::new();
        let keys_file = env::var("RUTABAGA_KEY_FILE")
            .expect("key file RUTABAGA_KEY_FILE should be set in env before running");
        let bytes: Vec<u8> = fs::read(keys_file)
            .expect("the env var RUTABAGA_KEY_FILE should be readable");
        let sk = SecretKey::from_slice(&bytes).unwrap();
        let kp = Keypair::from_secret_key(&s, &sk);
        ScriptBuf::new_p2tr(&s, kp.x_only_public_key().0, None)
    }

    pub fn script_grep<'a>(script_pubkey: &'a ScriptBuf, block: &'a bitcoinkernel::Block) -> Vec<(usize, TransactionRef<'a>)> {
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

    pub fn scan_block(
        &mut self,
        kernel_block: bitcoinkernel::Block,
        _spent_outputs: bitcoinkernel::BlockSpentOutputs,
        _block_height: u32,
    ) -> usize {
        let script = Self::read_pubkey_from_file();
        let outs = Self::script_grep(&script, &kernel_block);

        let ledger_file = env::var("RUTABAGA_LEDGER_FILE")
            .expect("ledger file RUTABAGA_LEDGER_FILE should be set in env before running");
        let path = std::path::Path::new(&ledger_file);
        output_ledger::append(path, outs);
        0
    }
    
    pub fn import_keys(
        &mut self,
        _scan_key: SecretKey,
        _spend_xonly: XOnlyPublicKey,
    ) -> Result<(), Error> {
        Ok(())
    }

    pub fn balance(&self) -> bitcoin::Amount {
        bitcoin::Amount::ZERO
    }

    pub fn utxo_count(&self) -> usize {
        0
    }

    pub fn reserve_coins(&mut self, _outpoints: impl IntoIterator<Item = bitcoin::OutPoint>) {
        let v: Vec<bitcoin::OutPoint> = vec![];
        v.into_iter();
    }

    pub fn receive_address(&self) -> Option<String> {
        None
    }
}
