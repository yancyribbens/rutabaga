use bitcoin::hashes::Hash;
use bitcoin::{OutPoint, TxOut};
use std::fs;
use std::io::Write;
use std::path::Path;

// entry size:
//  OutPoint { tx_id,: 32 bytes, vout: 4 bytes }
//  TxOut { value: 8 bytes, script_pukey: 34 bytes }
//
// memory layout:
//  [32 bytes, 4 bytes, 8 bytes, 34 bytes] -> 78 bytes
const ENTRY_SIZE: usize = 32 + 4 + 8 + 34;

// Create an entry to be written to file.
//
// store:
//  OutPoint { tx_id,: 32 bytes, vout: 4 bytes }
//  TxOut { value: 8 bytes, script_pukey: 34 bytes }
//
// memory layout:
//  [32 bytes, 4 bytes, 8 bytes, 34 bytes] -> 78 bytes
fn to_entry(outpoint: OutPoint, output: TxOut) -> [u8; ENTRY_SIZE] {
    let tx_id = outpoint.txid.to_byte_array();
    let vout = outpoint.vout.to_le_bytes();
    let value = output.value.to_sat().to_le_bytes();
    println!("append entry: {:?}", value);
    let script_bytes = output.script_pubkey.to_bytes();

    assert_eq!(script_bytes.len(), 34);

    let mut buf = [0; ENTRY_SIZE];
    buf[0..32].copy_from_slice(&tx_id);
    buf[32..32 + 4].copy_from_slice(&vout);
    buf[36..36 + 8].copy_from_slice(&value);
    buf[44..].copy_from_slice(&script_bytes);
    buf
}

// append a list of entries to file
pub fn append_entries(path: &Path, outs: Vec<(OutPoint, TxOut)>) {
    outs.into_iter().for_each(|(outpoint, output)| append_entry(path, outpoint, output))
}

// append a list of entries to file
pub fn append_entry(path: &Path, outpoint: OutPoint, output: TxOut) {
    let mut file = fs::File::options().append(true).create(true).open(path).unwrap();

    let stored_outs = read(path);
    let duplicate = stored_outs.contains(&(outpoint, output.clone()));

    if !duplicate {
        let _ = file.write_all(&to_entry(outpoint, output));
    }
}

// read a list of entries from file
pub fn read(path: &Path) -> Vec<(OutPoint, TxOut)> {
    if !path.exists() {
        vec![]
    } else {
        let bytes = fs::read(path).unwrap();
        let output_count = bytes.len() / ENTRY_SIZE;
        (0..output_count)
            .map(|i| {
                let curr = i * ENTRY_SIZE;
                let next = (i * ENTRY_SIZE) + ENTRY_SIZE;
                read_outs(&bytes[curr..next])
            })
            .collect()
    }
}

// read:
//  OutPoint { tx_id,: 32 bytes, vout: 4 bytes }
fn read_outpoint(bytes: &[u8]) -> OutPoint {
    let mut txid = [0; 32];
    txid.copy_from_slice(&bytes[0..32]);
    let txid = bitcoin::Txid::from_byte_array(txid);

    let mut index_bytes = [0; 4];
    index_bytes.copy_from_slice(&bytes[32..32 + 4]);
    let index = u32::from_le_bytes(index_bytes);

    bitcoin::OutPoint::new(txid, index)
}

// read:
//  TxOut { value: 8 bytes, script_pukey: 34 bytes }
fn read_tx_out(bytes: &[u8]) -> TxOut {
    let mut value_bytes = [0; 8];
    value_bytes.copy_from_slice(&bytes[0..8]);
    let value = i64::from_le_bytes(value_bytes);
    let signed_amount = bitcoin::SignedAmount::from_sat(value);
    let value = signed_amount.to_unsigned().unwrap();

    let mut script_bytes = [0; 34];
    script_bytes.copy_from_slice(&bytes[8..8 + 34]);
    let script_pubkey = bitcoin::ScriptBuf::from_bytes(script_bytes.to_vec());

    bitcoin::TxOut { value, script_pubkey }
}

// Read an entry from storage
//
// read:
//  OutPoint { tx_id,: 32 bytes, vout: 4 bytes }
//  TxOut { value: 8 bytes, script_pukey: 34 bytes }
//
// memory layout:
//  [32 bytes, 4 bytes, 8 bytes, 34 bytes] -> 78 bytes
fn read_outs(bytes: &[u8]) -> (OutPoint, TxOut) {
    let outpoint = read_outpoint(bytes);
    let tx_out = read_tx_out(&bytes[36..]);
    (outpoint, tx_out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile;

    use crate::tests::get_transaction;
    use crate::tests::write_tx_outs_to_file;

    #[test]
    fn read_tx() {
        let tx_2462 = tests::get_transaction("2462");
        let tx_8926 = tests::get_transaction("8926");

        let dir = tempfile::tempdir().unwrap();
        let outs_path = dir.path().join("outputs");

        write_tx_outs_to_file(&tx_2462, &outs_path);
        write_tx_outs_to_file(&tx_8926, &outs_path);
        let outs = read(&outs_path);
        assert_eq!(outs.len(), 4);

        let out = &outs[0];
        let (outpoint, output) = out;
        assert_eq!(outpoint.txid.to_byte_array(), tx_2462.compute_txid().to_byte_array());
        assert_eq!(outpoint.vout, 0);
        assert_eq!(
            output.value,
            tx_2462.output.first().unwrap().value
        );
        assert_eq!(
            output.script_pubkey.to_bytes(),
            tx_2462.output.first().unwrap().script_pubkey.to_bytes()
        );

        let out = &outs[1];
        let (outpoint, output) = out;
        assert_eq!(outpoint.txid.to_byte_array(), tx_2462.compute_txid().to_byte_array());
        assert_eq!(outpoint.vout, 1);
        assert_eq!(
            output.value,
            tx_2462.output.get(1).unwrap().value
        );
        assert_eq!(
            output.script_pubkey.to_bytes(),
            tx_2462.output.get(1).unwrap().script_pubkey.to_bytes()
        );

        let out = &outs[2];
        let (outpoint, output) = out;
        assert_eq!(outpoint.txid.to_byte_array(), tx_8926.compute_txid().to_byte_array());
        assert_eq!(outpoint.vout, 0);
        assert_eq!(
            output.value,
            tx_8926.output.first().unwrap().value
        );
        assert_eq!(
            output.script_pubkey.to_bytes(),
            tx_8926.output.first().unwrap().script_pubkey.to_bytes()
        );

        let out = &outs[3];
        let (outpoint, output) = out;
        assert_eq!(outpoint.txid.to_byte_array(), tx_8926.compute_txid().to_byte_array());
        assert_eq!(outpoint.vout, 1);
        assert_eq!(
            output.value,
            tx_8926.output.get(1).unwrap().value
        );
        assert_eq!(
            output.script_pubkey.to_bytes(),
            tx_8926.output.get(1).unwrap().script_pubkey.to_bytes()
        );
    }

    #[test]
    fn duplicate_tx() {
        let dir = tempfile::tempdir().unwrap();
        let outs_path = dir.path().join("outputs");

        let tx_2462 = tests::get_transaction("2462");
        write_tx_outs_to_file(&tx_2462, &outs_path);
        write_tx_outs_to_file(&tx_2462, &outs_path);

        let outs = read(&outs_path);
        assert_eq!(outs.len(), 2);
    }
}
