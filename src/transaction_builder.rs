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

fn calc_fee(fee_rate: FeeRate, inputs: &[TxIn], outputs: &[&ScriptBuf]) -> Option<Amount> {
    let input_iwp = InputWeightPrediction::P2TR_KEY_DEFAULT_SIGHASH;
    let input_iwps: Vec<_> = inputs.iter().map(|_| input_iwp).collect();
    let lens: Vec<_> = outputs.iter().map(|o| o.len()).collect();
    let predicted_tx_weight = bitcoin::transaction::predict_weight(input_iwps, lens);

    FeeRate::fee_wu(fee_rate, predicted_tx_weight)
}

pub fn build_without_change(
    coins: &[Coin],
    recipient: &ScriptBuf,
    kp: Keypair,
    fee_rate: FeeRate,
) -> Option<Transaction> {
    let inputs = coins
        .iter()
        .map(|coin| TxIn {
            previous_output: coin.outpoint,
            script_sig: ScriptBuf::new(),
            sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
            witness: Witness::default(),
        })
        .collect();

    let fee = calc_fee(fee_rate, &[], &[recipient])?;

    let prevouts: Vec<_> = coins.iter().map(|coin| coin.tx_out.clone()).collect();
    let value =
        prevouts.iter().map(|tx_out| tx_out.value).try_fold(Amount::ZERO, Amount::checked_add)?;

    let to_recipient = value.checked_sub(fee)?;
    let output = TxOut { value: to_recipient, script_pubkey: recipient.clone() };

    let mut tx = Transaction {
        version: Version::TWO,
        lock_time: LockTime::ZERO,
        input: inputs,
        output: vec![output],
    };

    let mut sighasher = SighashCache::new(&mut tx);
    for (index, _) in coins.iter().enumerate() {
        let sighash_type = TapSighashType::Default;
        let prevouts = Prevouts::All(&prevouts);

        let sighash =
            sighasher.taproot_key_spend_signature_hash(index, &prevouts, sighash_type).unwrap();

        let s = Secp256k1::new();
        let tweaked: TweakedKeypair = kp.tap_tweak(&s, None);
        let msg = Message::from_digest(sighash.to_byte_array());
        let signature = s.sign_schnorr(&msg, &tweaked.to_keypair());

        let signature = bitcoin::taproot::Signature { signature, sighash_type };
        sighasher.witness_mut(index).unwrap().push(signature.to_vec());
    }
    let tx = sighasher.into_transaction();
    Some(tx.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use arbtest::arbitrary::{Arbitrary, Result, Unstructured};
    use arbtest::arbtest;
    use bitcoin::amount::CheckedSum;
    use bitcoin::secp256k1::SecretKey;
    use bitcoin::{OutPoint, Transaction};

    use crate::tests::get_transaction;

    fn kp() -> Keypair {
        let s = Secp256k1::new();
        let sk = SecretKey::from_slice(&[0xcd; 32]).expect("32 bytes, within curve order");
        Keypair::from_secret_key(&s, &sk)
    }

    fn tx_coin(tx: &Transaction) -> Coin {
        let outpoint: OutPoint = tx.tx_in(0).unwrap().previous_output;
        let tx_out = tx.tx_out(0).unwrap().clone();
        //tx_out.value = Amount::from_sat(1000);
        // in reality, this coin is invalid since the outpoint
        // and the tx_out are in the same tx.
        Coin { outpoint, tx_out }
    }

    impl<'a> Arbitrary<'a> for Coin {
        fn arbitrary(u: &mut Unstructured<'a>) -> Result<Self> {
            let outpoint: OutPoint = u.arbitrary()?;
            let tx_out: TxOut = u.arbitrary()?;
            let coin = Coin { outpoint, tx_out };
            Ok(coin)
        }
    }

    #[test]
    fn valid_build_tx_without_change() {
        let tx_2462 = get_transaction("2462");
        let tx_8926 = get_transaction("8926");

        let coin_2462 = tx_coin(&tx_2462);
        let coin_8926 = tx_coin(&tx_8926);
        let coins = vec![coin_2462, coin_8926];

        let fee_rate = FeeRate::from_sat_per_vb(5).unwrap();
        let script_pubkey = &coins[0].tx_out.script_pubkey;
        let tx = build_without_change(&coins, script_pubkey, kp(), fee_rate).unwrap();

        let coin_val: Amount = coins.iter().map(|c| c.tx_out.value).sum();
        let fee = calc_fee(fee_rate, &[], &[script_pubkey]);
        let output = &tx.output[0];
        assert_eq!(output.value, coin_val - fee.unwrap());
    }

    #[test]
    fn invalid_fee_rate_tx_without_change() {
        let tx = get_transaction("2462");
        let coin = tx_coin(&tx);

        let fee_rate = FeeRate::MAX;
        let tx = build_without_change(&[coin], &ScriptBuf::new(), kp(), fee_rate);
        assert!(tx.is_none());
    }

    #[test]
    fn val_less_than_fee() {
        let tx = get_transaction("2462");
        let outpoint = OutPoint { txid: tx.compute_txid(), vout: 0 };

        let tx_out = TxOut { value: Amount::ZERO, script_pubkey: ScriptBuf::new() };

        let fee_rate = FeeRate::from_sat_per_vb(10).unwrap();
        let coin = Coin { outpoint, tx_out };
        let tx = build_without_change(&[coin], &ScriptBuf::new(), kp(), fee_rate);
        assert!(tx.is_none());
    }

    #[test]
    fn arb_build_tx_without_change() {
        arbtest(|u| {
            let recipient = ScriptBuf::arbitrary(u)?;
            let coins: Vec<Coin> = Vec::arbitrary(u)?;
            let fee_rate: FeeRate = u.arbitrary()?;
            let tx = build_without_change(&coins, &recipient, kp(), fee_rate);

            match tx {
                Some(t) => {
                    assert_eq!(t.input.len(), coins.len());
                    assert_eq!(t.output.len(), 1);

                    let inputs = &t.input;
                    let output = &t.output[0];

                    let input_outs: Vec<OutPoint> =
                        inputs.iter().map(|i| i.previous_output).collect();
                    let coin_outs: Vec<OutPoint> = coins.iter().map(|c| c.outpoint).collect();
                    assert_eq!(input_outs, coin_outs);

                    let coin_val: Amount = coins.iter().map(|c| c.tx_out.value).sum();
                    let fee = calc_fee(fee_rate, &[], &[&recipient]).unwrap();
                    assert_eq!(output.value, coin_val - fee);
                }
                None => {
                    let fee = calc_fee(fee_rate, &[], &[&recipient]);
                    let val = coins.iter().map(|c| c.tx_out.value).checked_sum();
                    if let Some(f) = fee {
                        if let Some(v) = val {
                            assert!(v < f)
                        }
                    }
                }
            }

            Ok(())
        });
    }
}
