#[derive(Debug)]

pub struct Wallet {
    pub scan_height: u32
}
pub struct Error {}

use bitcoin::secp256k1::{SecretKey, XOnlyPublicKey};
use bitcoin::{Amount, FeeRate};
use bitcoin::transaction::Version;
use bitcoin::absolute::LockTime;
use bitcoin::Address;
use std::fmt;

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

    pub fn scan_block(
        &mut self,
        _kernel_block: bitcoinkernel::Block,
        _spent_outputs: bitcoinkernel::BlockSpentOutputs,
        _block_height: u32,
    ) -> usize {
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
