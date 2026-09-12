//! # Transaction Builder
//!
//! Build Transaction

use bitcoin::absolute::LockTime;
use bitcoin::hashes::Hash;
use bitcoin::key::{Keypair, TapTweak, TweakedKeypair};
use bitcoin::secp256k1::{Message, Secp256k1};
use bitcoin::sighash::{Prevouts, SighashCache};
use bitcoin::transaction::{InputWeightPrediction, Version};
use bitcoin::{
    Amount, FeeRate, ScriptBuf, Sequence, TapSighashType, Transaction, TxIn, TxOut, Witness,
};

use crate::coin::Coin;

fn calc_fee(fee_rate: FeeRate, recipient: &ScriptBuf) -> Option<Amount> {
    let input_iwp = InputWeightPrediction::P2TR_KEY_DEFAULT_SIGHASH;
    let predicted_tx_weight =
        bitcoin::transaction::predict_weight(vec![input_iwp], vec![recipient.len()]);
    FeeRate::fee_wu(fee_rate, predicted_tx_weight)
}

pub fn build(
    coin: &Coin,
    recipient: &ScriptBuf,
    kp: Keypair,
    fee_rate: FeeRate,
) -> Option<Transaction> {
    let input = TxIn {
        previous_output: coin.outpoint,
        script_sig: ScriptBuf::new(),
        sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
        witness: Witness::default(),
    };

    let fee = calc_fee(fee_rate, recipient)?;

    let prevout = &coin.tx_out;
    let value = prevout.value;

    println!("in bulild_tx val {:?} fee {:?}", value, fee);
    let to_recipient = value - fee;
    let output = TxOut { value: to_recipient, script_pubkey: recipient.clone() };

    let mut tx = Transaction {
        version: Version::TWO,
        lock_time: LockTime::ZERO,
        input: vec![input],
        output: vec![output],
    };

    let mut sighasher = SighashCache::new(&mut tx);
    let sighash_type = TapSighashType::Default;
    let prevouts = Prevouts::All(&[prevout]);

    let index = 0;
    let sighash =
        sighasher.taproot_key_spend_signature_hash(index, &prevouts, sighash_type).unwrap();

    let s = Secp256k1::new();
    let tweaked: TweakedKeypair = kp.tap_tweak(&s, None);
    let msg = Message::from_digest(sighash.to_byte_array());
    let signature = s.sign_schnorr(&msg, &tweaked.to_keypair());

    let signature = bitcoin::taproot::Signature { signature, sighash_type };
    sighasher.witness_mut(index).unwrap().push(signature.to_vec());
    let tx = sighasher.into_transaction();
    Some(tx.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use arbtest::arbitrary::Arbitrary;
    use arbtest::arbtest;
    use bitcoin::consensus;
    use bitcoin::secp256k1::SecretKey;
    use bitcoin::{OutPoint, Transaction};
    use bitcoinkernel::core::TransactionExt;

    use crate::tests::get_transaction;

    fn kp() -> Keypair {
        let s = Secp256k1::new();
        let sk = SecretKey::from_slice(&[0xcd; 32]).expect("32 bytes, within curve order");
        Keypair::from_secret_key(&s, &sk)
    }

    fn bitcoin_tx(tx: bitcoinkernel::Transaction) -> bitcoin::Transaction {
        let t: Vec<u8> = tx.consensus_encode().unwrap();
        consensus::deserialize(&t).unwrap()
    }

    fn tx_coin(tx: &Transaction) -> Coin {
        let outpoint: OutPoint = tx.tx_in(0).unwrap().previous_output;
        let tx_out = tx.tx_out(0).unwrap().clone();
        // in reality, this coin is invalid since the outpoint
        // and the tx_out are in the same tx.
        Coin { outpoint, tx_out }
    }

    #[test]
    fn valid_build_tx() {
        let source_tx = bitcoin_tx(get_transaction("2462"));
        let coin = tx_coin(&source_tx);
        let script_pubkey = coin.clone().tx_out.script_pubkey;
        let fee_rate = FeeRate::from_sat_per_vb(5).unwrap();
        let result_tx = build(&coin, &script_pubkey, kp(), fee_rate).unwrap();

        let coin_val: Amount = coin.tx_out.value;
        let fee = calc_fee(fee_rate, &script_pubkey);
        let output = &result_tx.output[0];
        assert_eq!(output.value, coin_val - fee.unwrap());
    }

    #[test]
    fn invalid_fee_rate_tx() {
        let tx = bitcoin_tx(get_transaction("2462"));
        let coin = tx_coin(&tx);
        let script_pubkey = coin.clone().tx_out.script_pubkey;
        let fee_rate = FeeRate::MAX;
        let _ = build(&coin, &script_pubkey, kp(), fee_rate);
    }

    #[test]
    fn arb_build_tx() {
        arbtest(|u| {
            let outpoint = OutPoint::arbitrary(u)?;
            let tx_out = TxOut::arbitrary(u)?;
            let coin = Coin { outpoint, tx_out };
            let recipient = ScriptBuf::arbitrary(u)?;
            let fee_rate: FeeRate = u.arbitrary()?;
            let tx = build(&coin, &recipient, kp(), fee_rate);

            match tx {
                Some(t) => {
                    assert_eq!(t.input.len(), 1);
                    assert_eq!(t.output.len(), 1);
                    let input = &t.input[0];
                    let output = &t.output[0];
                    let fee = calc_fee(fee_rate, &recipient).unwrap();
                    assert_eq!(input.previous_output, outpoint);
                    assert_eq!(output.value, coin.tx_out.value - fee);
                }
                None => {
                    let fee = calc_fee(fee_rate, &recipient);
                    assert!(fee.is_none());
                }
            }

            Ok(())
        });
    }
}
