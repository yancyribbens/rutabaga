use bitcoin::hashes::Hash;
use bitcoin::OutPoint;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::Path;

// entry size:
//  OutPoint { tx_id,: 32 bytes, vout: 4 bytes }
//
// memory layout:
//  [32 bytes, 4 bytes] -> 36 bytes
const ENTRY_SIZE: usize = 32 + 4;

// append a list of entries to file
pub fn append(path: &Path, outs: Vec<OutPoint>) {
    let mut file = File::options().append(true).create(true).open(path).unwrap();

    outs.into_iter().for_each(|outpoint| {
        let mut buf = [0; ENTRY_SIZE];
        let tx_id = outpoint.txid.to_byte_array();
        let vout = outpoint.vout.to_le_bytes();

        buf[0..32].copy_from_slice(&tx_id);
        buf[32..ENTRY_SIZE].copy_from_slice(&vout);
        file.write_all(&buf).unwrap()
    });
}

// read a list of entries from file
pub fn read(path: &Path) -> Vec<OutPoint> {
    let bytes = fs::read(path).unwrap();
    let spent_count = bytes.len() / ENTRY_SIZE;
    (0..spent_count)
        .map(|i| {
            let curr = i * ENTRY_SIZE;
            let next = (i * ENTRY_SIZE) + ENTRY_SIZE;
            read_outpoint(&bytes[curr..next])
        })
        .collect()
}

// read:
//  OutPoint { tx_id,: 32 bytes, vout: 4 bytes }
fn read_outpoint(bytes: &[u8]) -> OutPoint {
    let mut txid = [0; 32];
    txid.copy_from_slice(&bytes[0..32]);
    let txid = bitcoin::Txid::from_byte_array(txid);

    let mut index_bytes = [0; 4];
    index_bytes.copy_from_slice(&bytes[32..ENTRY_SIZE]);
    let index = u32::from_le_bytes(index_bytes);

    bitcoin::OutPoint::new(txid, index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoinkernel::core::{TransactionExt, TxInExt, TxOutPointExt, TxidExt};
    use bitcoinkernel::Transaction;
    use tempfile;

    use crate::tests::get_transaction;

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
    fn read_tx() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("spent_outpoints");

        let tx_2462 = tests::get_transaction("2462");
        let outpoints = outpoints_from_tx(&tx_2462);
        append(&file_path, outpoints);

        let tx_8926 = tests::get_transaction("8926");
        let outpoints = outpoints_from_tx(&tx_8926);
        append(&file_path, outpoints);

        let spent_outpoints = read(&file_path);

        let outpoint = &spent_outpoints[0];
        assert_eq!(
            outpoint.txid.to_byte_array(),
            tx_2462.inputs().next().unwrap().outpoint().txid().to_bytes()
        );
        assert_eq!(outpoint.vout, 0);

        let outpoint = &spent_outpoints[1];
        assert_eq!(
            outpoint.txid.to_byte_array(),
            tx_8926.inputs().next().unwrap().outpoint().txid().to_bytes()
        );
        assert_eq!(outpoint.vout, 2);
    }
}
