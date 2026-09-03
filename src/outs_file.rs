//! # OutsFile
//!
//! The OutsFile reads and writes a list Outputs (TxOut) and OutPoints to file.
//! Each Output and OutPoint tracks a received amount of Bitcoin and the conditions
//! to spend it.

use bitcoin::hashes::Hash;
use bitcoin::{OutPoint, TxOut};
use std::io::Write;
use std::path::Path;

use std::fs::File;
use std::io::prelude::*;

use log::{debug, info};

pub struct OutsFile {
    pub file: File,
}

impl OutsFile {
    // entry size:
    //  OutPoint { tx_id,: 32 bytes, vout: 4 bytes }
    //  TxOut { value: 8 bytes, script_pukey: 34 bytes }
    //
    // memory layout:
    //  [32 bytes, 4 bytes, 8 bytes, 34 bytes] -> 78 bytes
    const ENTRY_SIZE: usize = 32 + 4 + 8 + 34;

    pub fn new(path: &Path) -> Self {
        debug!("Loading OutsFile {:?}", path);
        let file =
            std::fs::File::options().read(true).create(true).append(true).open(path).unwrap();
        Self { file }
    }

    pub fn len(&mut self) -> usize {
        self.read().len()
    }

    pub fn is_empty(&mut self) -> bool {
        self.read().len() == 0
    }

    // read a list of entries from file
    pub fn read(&mut self) -> Vec<(OutPoint, TxOut)> {
        let mut buf = [0; Self::ENTRY_SIZE];
        // reset file pointer so we know its reading from start
        self.file.rewind().unwrap();

        let mut outs = vec![];
        loop {
            match self.file.read_exact(&mut buf) {
                Ok(_) => {
                    let out = Self::read_outs(&buf);
                    outs.push(out);
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    break;
                }
                Err(_e) => {
                    // I/O error
                    break;
                }
            }
        }
        outs
    }

    // Write an entry to file.
    //
    // store:
    //  OutPoint { tx_id,: 32 bytes, vout: 4 bytes }
    //  TxOut { value: 8 bytes, script_pukey: 34 bytes }
    //
    // memory layout:
    //  [32 bytes, 4 bytes, 8 bytes, 34 bytes] -> 78 bytes
    pub fn write_entry(&mut self, outpoint: OutPoint, output: TxOut) {
        let tx_id = outpoint.txid.to_byte_array();
        let vout = outpoint.vout.to_le_bytes();
        let value = output.value.to_sat().to_le_bytes();
        let script_bytes = output.script_pubkey.to_bytes();

        assert_eq!(script_bytes.len(), 34);

        let mut buf = [0; Self::ENTRY_SIZE];
        buf[0..32].copy_from_slice(&tx_id);
        buf[32..32 + 4].copy_from_slice(&vout);
        buf[36..36 + 8].copy_from_slice(&value);
        buf[44..].copy_from_slice(&script_bytes);

        self.file.write_all(&buf).unwrap()
    }

    // append a list of entries to file
    pub fn append_entries(&mut self, outs: Vec<(OutPoint, TxOut)>) {
        outs.into_iter().for_each(|(outpoint, output)| self.append_entry(outpoint, output))
    }

    // append a list of entries to file
    pub fn append_entry(&mut self, outpoint: OutPoint, output: TxOut) {
        info!("write entry: {:?} {:?}", outpoint, output);
        let stored_outs = self.read();
        let duplicate = stored_outs.contains(&(outpoint, output.clone()));

        if !duplicate {
            self.write_entry(outpoint, output);
        } else {
            debug!("found duplicate, not recording");
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
        let outpoint = Self::read_outpoint(bytes);
        let tx_out = Self::read_tx_out(&bytes[36..]);
        (outpoint, tx_out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::outs_from_tx;
    use tempfile;

    use crate::tests::get_transaction;

    #[test]
    fn read_tx() {
        let tx_2462 = tests::get_transaction("2462");
        let tx_8926 = tests::get_transaction("8926");

        let dir = tempfile::tempdir().unwrap();
        let outs_path = dir.path().join("outputs");
        let mut outs_file = OutsFile::new(&outs_path);

        let outs = outs_from_tx(&tx_2462);
        outs_file.append_entries(outs);

        let outs = outs_from_tx(&tx_8926);
        outs_file.append_entries(outs);

        let outs = outs_file.read();
        assert_eq!(outs.len(), 4);

        let out = &outs[0];
        let (outpoint, output) = out;
        assert_eq!(outpoint.txid.to_byte_array(), tx_2462.compute_txid().to_byte_array());
        assert_eq!(outpoint.vout, 0);
        assert_eq!(output.value, tx_2462.output.first().unwrap().value);
        assert_eq!(
            output.script_pubkey.to_bytes(),
            tx_2462.output.first().unwrap().script_pubkey.to_bytes()
        );

        let out = &outs[1];
        let (outpoint, output) = out;
        assert_eq!(outpoint.txid.to_byte_array(), tx_2462.compute_txid().to_byte_array());
        assert_eq!(outpoint.vout, 1);
        assert_eq!(output.value, tx_2462.output.get(1).unwrap().value);
        assert_eq!(
            output.script_pubkey.to_bytes(),
            tx_2462.output.get(1).unwrap().script_pubkey.to_bytes()
        );

        let out = &outs[2];
        let (outpoint, output) = out;
        assert_eq!(outpoint.txid.to_byte_array(), tx_8926.compute_txid().to_byte_array());
        assert_eq!(outpoint.vout, 0);
        assert_eq!(output.value, tx_8926.output.first().unwrap().value);
        assert_eq!(
            output.script_pubkey.to_bytes(),
            tx_8926.output.first().unwrap().script_pubkey.to_bytes()
        );

        let out = &outs[3];
        let (outpoint, output) = out;
        assert_eq!(outpoint.txid.to_byte_array(), tx_8926.compute_txid().to_byte_array());
        assert_eq!(outpoint.vout, 1);
        assert_eq!(output.value, tx_8926.output.get(1).unwrap().value);
        assert_eq!(
            output.script_pubkey.to_bytes(),
            tx_8926.output.get(1).unwrap().script_pubkey.to_bytes()
        );
    }

    #[test]
    fn duplicate_tx() {
        let dir = tempfile::tempdir().unwrap();
        let outs_path = dir.path().join("outputs");
        let mut outs_file = OutsFile::new(&outs_path);

        let tx_2462 = get_transaction("2462");
        let outs = outs_from_tx(&tx_2462);
        outs_file.append_entries(outs);

        let outs = outs_from_tx(&tx_2462);
        outs_file.append_entries(outs);

        let outs = outs_file.read();
        assert_eq!(outs.len(), 2);
    }
}
