use std::io::Write;
use std::path::Path;
use std::fs;
use bitcoin::hashes::Hash;
use bitcoin::OutPoint;
use bitcoin::TxOut;
use bitcoin::Txid;
use bitcoinkernel::TransactionRef;
use bitcoinkernel::core::TransactionExt;
use bitcoinkernel::core::TxidExt;
use bitcoin::{ScriptBuf};
use bitcoinkernel::core::TxOutExt;
use bitcoinkernel::core::ScriptPubkeyExt;

// entry size:
//  OutPoint { tx_id,: 32 bytes, vout: 4 bytes }
//  TxOut { value: 8 bytes, script_pukey: 34 bytes }
//
// memory layout:
//  [32 bytes, 4 bytes, 8 bytes, 34 bytes] -> 78 bytes
const ENTRY_SIZE: usize = 32 + 4 + 8 + 34;

// Create rust-bitcoin OutPoint from kernel parts
fn outpoint<'a>(vout: u32, tx_ref: TransactionRef<'a>) -> OutPoint {
    let id_bytes = tx_ref.to_owned().txid().to_bytes();
    let txid = Txid::from_byte_array(id_bytes);
    OutPoint { vout, txid }
}

// Create rust-bitcoin TxOut from kernel parts
fn output<'a>(index: usize, tx_ref: TransactionRef<'a>) -> TxOut {
    let tx = tx_ref.to_owned();
    let mut outputs = tx.outputs();
    let output = outputs.nth(index).unwrap();
    let script_bytes = output.script_pubkey().to_bytes();
    let script = ScriptBuf::from_bytes(script_bytes); 
    let value = output.value();
    let amount = bitcoin::SignedAmount::from_sat(value).to_unsigned().unwrap();
    TxOut {
        value: amount,
        script_pubkey: script
    }
}

// Create an entry to be written to file.
//
// store:
//  OutPoint { tx_id,: 32 bytes, vout: 4 bytes }
//  TxOut { value: 8 bytes, script_pukey: 34 bytes }
//
// memory layout:
//  [32 bytes, 4 bytes, 8 bytes, 34 bytes] -> 78 bytes
fn to_entry<'a>(index: usize, tx_ref: TransactionRef<'a>) -> [u8; ENTRY_SIZE] {
    let vout: u32 = index.try_into().unwrap();
    let outpoint = outpoint(vout, tx_ref);
    let output = output(index, tx_ref);

    let tx_id = outpoint.txid.to_byte_array();
    let vout = vout.to_le_bytes();
    let value = output.value.to_sat().to_le_bytes();
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
pub fn append<'a>(path: &Path, outs: Vec<(usize, TransactionRef<'a>)>) {
    let mut file = fs::File::options()
        .append(true)
        .create(true)
        .open(path).unwrap();

    outs.into_iter().for_each(|(i, tx_ref)| {
        file.write_all(&to_entry(i, tx_ref)).unwrap()
    })
}

// read a list of entries from file
pub fn read(path: &Path) -> Vec<(OutPoint, TxOut)>{
    let bytes = fs::read(path).unwrap();
    let output_count = bytes.len() / ENTRY_SIZE;
    (0..output_count).map(|i| {
        let curr = i * ENTRY_SIZE;
        let next = (i * ENTRY_SIZE) + ENTRY_SIZE;
        read_outs(&bytes[curr..next])
    }).collect()
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

    bitcoin::OutPoint::new(
        txid,
        index
    )
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

    bitcoin::TxOut {
        value,
        script_pubkey
    }
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
    use bitcoinkernel::Transaction;

    fn get_transaction(file: &str) -> Transaction {
        let file = format!("tests/transaction_{file}.bin");
        let tx_data = fs::read(file).unwrap();
        Transaction::new(&tx_data).unwrap()
    }

    fn outs_from_tx(tx: &Transaction) -> Vec<(usize, TransactionRef<'_>)>{
        let tx_ref = tx.as_ref();
        vec![(1, tx_ref)]
    }

    #[test]
    fn read_tx() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("outputs");

        let tx_2462 = get_transaction("2462");
        let outs = outs_from_tx(&tx_2462);
        append(&file_path, outs);

        let tx_8926 = get_transaction("8926");
        let outs = outs_from_tx(&tx_8926);
        append(&file_path, outs);

        let outs = read(&file_path);
        assert_eq!(outs.len(), 2);

        let out = &outs[0];
        let (outpoint, output) = out;
        assert_eq!(outpoint.txid.to_byte_array(), tx_2462.txid().to_bytes());
        assert_eq!(outpoint.vout, 1);
        assert_eq!(output.value.to_sat(), tx_2462.outputs().nth(1).unwrap().value().try_into().unwrap());
        assert_eq!(output.script_pubkey.to_bytes(), tx_2462.outputs().nth(1).unwrap().script_pubkey().to_bytes());

        let out = &outs[1];
        let (outpoint, output) = out;
        assert_eq!(outpoint.txid.to_byte_array(), tx_8926.txid().to_bytes());
        assert_eq!(outpoint.vout, 1);
        assert_eq!(output.value.to_sat(), tx_8926.outputs().nth(1).unwrap().value().try_into().unwrap());
        assert_eq!(output.script_pubkey.to_bytes(), tx_8926.outputs().nth(1).unwrap().script_pubkey().to_bytes());
    }
}
