pub mod output_ledger;
pub mod spent_ledger;
pub use bitcoin::{OutPoint, TxOut};
use std::path::Path;

pub fn utxos_from_ledger(outs_ledger: &Path, spent_ledger: &Path) -> Vec<(OutPoint, TxOut)> {
    let outs = output_ledger::read(&outs_ledger);
    let spent_outpoints = spent_ledger::read(&spent_ledger);
    utxos(outs, spent_outpoints)
}

pub fn utxos(outs: Vec<(OutPoint, TxOut)>, spent_outputs: Vec<OutPoint>) -> Vec<(OutPoint, TxOut)> {
    outs.into_iter()
        .filter(|(outpoint, _)| !spent_outputs.contains(outpoint))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use bitcoinkernel::Transaction;
    use bitcoinkernel::TransactionRef;
    use bitcoin::OutPoint;
    use bitcoin::TxOut;
    use crate::output_ledger::{append, read};

    use arbtest::arbtest;
    use arbtest::arbitrary::Arbitrary;

    pub fn get_transaction(file: &str) -> Transaction {
        let file = format!("tests/transaction_{file}.bin");
        let tx_data = fs::read(file).unwrap();
        Transaction::new(&tx_data).unwrap()
    }

    #[test]
    fn utxos() {
        arbtest(|u| {
            let unspent_outpoint = OutPoint::arbitrary(u)?;
            let unspent_tx_out = TxOut::arbitrary(u)?;

            let spent_outpoint = OutPoint::arbitrary(u)?;
            let spent_tx_out = TxOut::arbitrary(u)?;

            let outs = vec![
                (unspent_outpoint, unspent_tx_out.clone()),
                (spent_outpoint, spent_tx_out)
            ];

            let res = crate::utxos(outs, vec![spent_outpoint]);
            let (outpoint, ref tx_out) = res[0];

            assert_eq!(outpoint, unspent_outpoint);
            assert_eq!(tx_out, &unspent_tx_out.clone());

            Ok(())
        });
    }
}
