//! # Coin
//!
//! A Coin is that which has a UTXO (a TxOut which is unspent) and an OutPoint
//! with which to spend in the future

use bitcoin::{OutPoint, TxOut};

#[derive(Clone, Debug)]
pub struct Coin {
    pub outpoint: OutPoint,
    pub tx_out: TxOut,
}

pub fn from_outs(outs: Vec<(OutPoint, TxOut)>, spent_outpoints: Vec<OutPoint>) -> Vec<Coin> {
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
    use crate::outs_file::OutsFile;
    use crate::spent_file::SpentFile;
    use crate::tests::get_transaction;
    use crate::tests::outs_from_tx;

    #[test]
    fn read_coins_from_outs() {
        let tx_2462 = get_transaction("2462");
        let tx_8926 = get_transaction("8926");

        let dir = tempfile::tempdir().unwrap();
        let outs_path = dir.path().join("outputs");
        let mut outs_file = OutsFile::new(&outs_path);

        let outs = outs_from_tx(&tx_2462);
        outs_file.append_entries(outs);

        let outs = outs_from_tx(&tx_8926);
        outs_file.append_entries(outs);

        let outs = outs_file.read();

        let (outpoint, _) = outs[0];
        let dir = tempfile::tempdir().unwrap();
        let spent_path = dir.path().join("spent");
        let mut spent_file = SpentFile::new(&spent_path);
        spent_file.append_entries(vec![outpoint]);

        let coin = coin::from_outs(outs.clone(), spent_file.read());
        assert_eq!(coin.len(), 3);
        assert_eq!(outs[1].0, coin[0].outpoint);
        assert_eq!(outs[1].1, coin[0].tx_out);

        assert_eq!(outs[2].0, coin[1].outpoint);
        assert_eq!(outs[2].1, coin[1].tx_out);

        assert_eq!(outs[3].0, coin[2].outpoint);
        assert_eq!(outs[3].1, coin[2].tx_out);
    }
}
