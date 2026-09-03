pub mod hash_tracker;
pub mod outs_file;

#[cfg(test)]
mod tests {
    use bitcoin::consensus::Decodable;
    use bitcoin::{OutPoint, TxOut};
    use std::fs;

    pub fn get_transaction(file: &str) -> bitcoin::Transaction {
        let file = format!("tests/transaction_{file}.bin");
        let tx_data = fs::read(file).unwrap();
        Decodable::consensus_decode(&mut tx_data.as_slice()).unwrap()
    }

    pub fn outs_from_tx(tx: &bitcoin::Transaction) -> Vec<(OutPoint, TxOut)> {
        let mut ret = vec![];
        for (i, output) in tx.output.iter().enumerate() {
            let vout: u32 = i.try_into().unwrap();
            let outpoint: OutPoint = OutPoint { txid: tx.compute_txid(), vout };
            ret.push((outpoint, output.clone()));
        }
        ret
    }
}
