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

    pub fn write_outs_file(tx_2462: &Transaction, tx_8926: &Transaction) -> Vec<(OutPoint, TxOut)>{
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("outputs");

        let outs = outs_from_tx(&tx_2462);
        append(&file_path, outs);

        let outs = outs_from_tx(&tx_8926);
        append(&file_path, outs);

        let outs = read(&file_path);
        assert_eq!(outs.len(), 2);
        outs
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

        let tx_2250 = get_transaction("2250");

        let outs = write_outs_file(&tx_2462, &tx_8926);
        assert_eq!(2, outs.len());

        // TODO create a fixture file with a transaction that spends an out
        // by create a rust-bitcoin tx: 
        // https://docs.rs/bitcoin/latest/bitcoin/struct.Transaction.html
        // then consensus decode and encode it into a kernel tx:
        // https://docs.rs/bitcoin/latest/bitcoin/struct.Transaction.html
        // then write that file as a fixture creating a third tx file

        //let tx_in = bitcoin::TxIn {
            //previous_output: outs[0].0,
            //script_sig: ScriptBuf::new(),
            //sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
            //witness: Witness::default(),
        //};

        //let tx = bitcoin::Transaction {
            //version: Version::TWO,
            //lock_time: LockTime::from_height(0).unwrap_or(LockTime::ZERO),
            //input: vec![tx_in],
            //output: vec![],
        //};

        //let mut serialized_tx = Vec::new();
        //let byte_count = tx.consensus_encode(&mut serialized_tx);

        //let enc = bitcoinkernel::Transaction::new(&serialized_tx).unwrap(); 
        //println!("id: {:?}", tx.txid());
        //fs::write("/tmp/2250_transaction.bin", enc.consensus_encode().unwrap());
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
