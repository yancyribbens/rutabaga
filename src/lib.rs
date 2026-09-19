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
    use bitcoinkernel::core::TransactionExt;
    use bitcoinkernel::core::TxInExt;
    use bitcoinkernel::core::TxOutPointExt;
    use bitcoin::hashes::Hash;
    use bitcoinkernel::core::TxidExt;
    use crate::output_ledger::{append, read};

    use arbtest::arbtest;
    use arbtest::arbitrary::Arbitrary;

    use crate::{output_ledger, spent_ledger, utxos_from_ledger};

    use bitcoin::{OutPoint, ScriptBuf, Sequence, TxIn, TxOut, Witness};
    use bitcoin::absolute::LockTime;
    use bitcoin::transaction::Version;

    use bitcoin::consensus::Encodable;

    pub fn get_transaction(file: &str) -> Transaction {
        let file = format!("tests/transaction_{file}.bin");
        let tx_data = fs::read(file).unwrap();
        Transaction::new(&tx_data).unwrap()
    }

    pub fn outs_from_tx(tx: &Transaction) -> Vec<(usize, TransactionRef<'_>)> {
        let tx_ref = tx.as_ref();
        vec![(1, tx_ref)]
    }

    fn outpoints_from_tx(tx: &Transaction) -> Vec<OutPoint> {
        tx.inputs()
            .map(|i| {
                let outpoint = i.outpoint();
                let vout = outpoint.index();
                let txid = outpoint.txid();
                let txid = bitcoin::Txid::from_byte_array(txid.to_bytes());
                bitcoin::OutPoint { txid, vout }
            })
            .collect()
    }

    #[test]
    fn read_utxos_from_ledger() {
        let tx_2462 = get_transaction("2462");
        let tx_8926 = get_transaction("8926");

        let dir = tempfile::tempdir().unwrap();
        let outs_path = dir.path().join("outputs");

        let outs = outs_from_tx(&tx_2462);
        append(&outs_path, outs);

        let outs = outs_from_tx(&tx_8926);
        append(&outs_path, outs);

        let outs = read(&outs_path);
        assert_eq!(2, outs.len());

        let (outpoint, ref tx_out) = outs[0];
        let dir = tempfile::tempdir().unwrap();
        let spent_path = dir.path().join("spent");
        spent_ledger::append(&spent_path, vec![outpoint]);

        let utxos = utxos_from_ledger(&outs_path, &spent_path);
        assert_eq!(utxos.len(), 1);
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
