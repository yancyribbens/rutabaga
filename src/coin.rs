//! # Coin
//!
//! A Coin is that which has a UTXO (a TxOut which is unspent) and an OutPoint
//! with which to spend in the future

use bitcoin::{Amount, OutPoint, TxOut, Weight};
use bitcoin_coin_selection::WeightedUtxo;
use std::path::Path;

use crate::{output_ledger, spent_ledger};

#[derive(Clone, Debug)]
pub struct Coin {
    pub outpoint: OutPoint,
    pub tx_out: TxOut,
}

impl WeightedUtxo for Coin {
    fn satisfaction_weight(&self) -> Weight {
        // see rust-bitcoin InputWeightPrediction P2TR_KEY_DEFAULT_SIGHASH
        // for full calculation, see InputWeightPrediction::from_slice()
        // 1 witness_len + 1 item len +  64 signature
        Weight::from_wu(66)
    }

    fn value(&self) -> Amount {
        self.tx_out.value
    }
}

pub fn from_ledger(outs_ledger: &Path, spent_ledger: &Path) -> Vec<Coin> {
    let outs = output_ledger::read(outs_ledger);
    let spent_outpoints = spent_ledger::read(spent_ledger);
    to_coin(outs, spent_outpoints)
}

pub fn to_coin(outs: Vec<(OutPoint, TxOut)>, spent_outputs: Vec<OutPoint>) -> Vec<Coin> {
    outs.into_iter()
        .filter(|(outpoint, _)| !spent_outputs.contains(outpoint))
        .map(|(outpoint, tx_out)| Coin { outpoint, tx_out })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::coin;
    use crate::tests::{get_transaction, write_tx_outs_to_file};
    use crate::{output_ledger, spent_ledger};

    #[test]
    fn read_coins_from_ledger() {
        let tx_2462 = get_transaction("2462");
        let tx_8926 = get_transaction("8926");

        let dir = tempfile::tempdir().unwrap();
        let outs_path = dir.path().join("outputs");

        write_tx_outs_to_file(&tx_2462, &outs_path);
        write_tx_outs_to_file(&tx_8926, &outs_path);
        let outs = output_ledger::read(&outs_path);

        let (outpoint, _) = outs[0];
        let dir = tempfile::tempdir().unwrap();
        let spent_path = dir.path().join("spent");
        spent_ledger::append(&spent_path, vec![outpoint]);

        let coin = coin::from_ledger(&outs_path, &spent_path);
        assert_eq!(coin.len(), 1);
        assert_eq!(outs[1].0, coin[0].outpoint);
        assert_eq!(outs[1].1, coin[0].tx_out);
    }
}
