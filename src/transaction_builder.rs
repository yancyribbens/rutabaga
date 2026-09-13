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
    Amount, FeeRate, ScriptBuf, Sequence, TapSighashType, Transaction, TxIn, TxOut, Weight, Witness,
};

use bitcoin_coin_selection::{branch_and_bound, single_random_draw};

use crate::coin::Coin;

const DEFAULT_DISCARD_FEE_RATE: FeeRate = FeeRate::from_sat_per_vb_u32(10);
const DEFAULT_LONG_TERM_FEE_RATE: FeeRate = FeeRate::from_sat_per_vb_u32(10);

// 32 byte txid, 4 byte output index, 1 byte scriptSig, and 4 byte sequence
const BASE_WEIGHT: Weight = Weight::from_vb_unwrap(32 + 4 + 1 + 4);

// cost of change is the cost to create a change output plus the estimated cost to spend it as
// input.
//
// therefore, cost_of_change =
//      (change output size * fee rate) +
//      (change spend size * discard fee rate)
//
// The change output size of a TR output is 57.5 vB (230 wu)
// The change spend size is its input size in a future transaction, 43 vB (172 wu)
// Therefore, the total size estimate is 100.5 vB or 402 wu
//
// params
//  * fee_rate - current effective fee rate.
//  * discard_fee_rate - target fee rate is fee rate with which output will not be a dust output.
//    ref: core PR# 10817
fn default_tr_cost_of_change(fee_rate: FeeRate, discard_fee_rate: FeeRate) -> Amount {
    // output_size is 57.5 vB
    // the base_weight is 164 wu while the P2TR key-path is 66 WU totaling 230 WU
    let change_spend_size = BASE_WEIGHT + Weight::from_wu(66);
    let change_output_size = Weight::from_vb_unchecked(43);

    let change_fee = fee_rate.checked_mul_by_weight(change_output_size).unwrap_or(Amount::MAX);
    let min_viable_change =
        discard_fee_rate.checked_mul_by_weight(change_spend_size).unwrap_or(Amount::MAX);
    min_viable_change.checked_add(change_fee).unwrap_or(Amount::MAX)
}

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

pub fn build_with_change_possible(
    available_coins: &[Coin],
    target: Amount,
    recipient: &ScriptBuf,
    change_addr: &ScriptBuf,
    kp: Keypair,
    fee_rate: FeeRate,
) -> Option<Transaction> {
    let discard_fee_rate = DEFAULT_DISCARD_FEE_RATE;
    let lt_fee_rate = DEFAULT_LONG_TERM_FEE_RATE;
    let cost_of_change = default_tr_cost_of_change(fee_rate, discard_fee_rate);

    let base_tx_fee = calc_fee(fee_rate, &[], &[recipient])?;
    let total_target = target + base_tx_fee;
    let bnb_result =
        branch_and_bound(total_target, cost_of_change, fee_rate, lt_fee_rate, available_coins);

    let mut coins = vec![];
    if let Some((_i, selection)) = bnb_result {
        coins = selection.into_iter().cloned().collect();
    } else {
        let base_tx_fee = calc_fee(fee_rate, &[], &[recipient, change_addr])?;
        let total_target = target + base_tx_fee;
        let (_, selection) = single_random_draw(total_target, fee_rate, available_coins)?;
        coins = selection.into_iter().cloned().collect();
    }

    let input: Vec<_> = coins
        .iter()
        .map(|coin| TxIn {
            previous_output: coin.outpoint,
            script_sig: ScriptBuf::new(),
            sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
            witness: Witness::default(),
        })
        .collect();
    let mut output = vec![];

    let prevouts: Vec<_> = coins.iter().map(|coin| coin.tx_out.clone()).collect();
    let sum_of_inputs = prevouts
        .iter()
        .map(|tx_out| tx_out.value)
        .try_fold(Amount::ZERO, Amount::checked_add)
        .unwrap();

    let recipient_output = TxOut { value: target, script_pubkey: recipient.clone() };
    output.push(recipient_output);

    if sum_of_inputs > total_target + cost_of_change {
        let tx_fee = calc_fee(fee_rate, &input, &[recipient, change_addr])?;
        let change = sum_of_inputs - target - tx_fee;
        let change_output = TxOut { value: change, script_pubkey: change_addr.clone() };
        output.push(change_output);
    }

    let mut tx = Transaction { version: Version::TWO, lock_time: LockTime::ZERO, input, output };

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
    use bitcoin::consensus;
    use bitcoin::secp256k1::SecretKey;
    use bitcoin::{OutPoint, Transaction};
    use bitcoinkernel::core::TransactionExt;

    use crate::tests::get_transaction;

    use bitcoin::amount::CheckedSum;

    use bitcoin_coin_selection::WeightedUtxo;

    fn kp() -> Keypair {
        let s = Secp256k1::new();
        let sk = SecretKey::from_slice(&[0xcd; 32]).expect("32 bytes, within curve order");
        Keypair::from_secret_key(&s, &sk)
    }

    fn bitcoin_tx(tx: bitcoinkernel::Transaction) -> bitcoin::Transaction {
        let t: Vec<u8> = tx.consensus_encode().unwrap();
        consensus::deserialize(&t).unwrap()
    }

    fn calc_input_sum(inputs: &[TxIn], coins: Vec<Coin>) -> Amount {
        let prev_outs: Vec<OutPoint> = inputs.iter().map(|i| i.previous_output).collect();
        let mut input_sum = Amount::ZERO;
        for p in prev_outs {
            let input_coin = coins.iter().find(|c| c.outpoint == p).unwrap();
            input_sum += input_coin.value();
        }
        input_sum
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
        let tx_2462 = bitcoin_tx(get_transaction("2462"));
        let tx_8926 = bitcoin_tx(get_transaction("8926"));

        let coin_2462 = tx_coin(&tx_2462);
        let coin_8926 = tx_coin(&tx_8926);
        let coins = vec![coin_2462, coin_8926];

        let fee_rate = FeeRate::from_sat_per_vb(5).unwrap();
        let script_pubkey = &coins[0].tx_out.script_pubkey;
        let tx = build_without_change(&coins, script_pubkey, kp(), fee_rate).unwrap();

        let coin_val: Amount = coins.iter().map(|c| c.tx_out.value).sum();
        let fee = calc_fee(fee_rate, &[], &[&script_pubkey]);
        let output = &tx.output[0];
        assert_eq!(output.value, coin_val - fee.unwrap());
    }

    #[test]
    fn valid_build_tx_with_maybe_change() {
        let s = Secp256k1::new();

        let tx_2462 = bitcoin_tx(get_transaction("2462"));
        let coin_a = tx_coin(&tx_2462);
        let coin_b = coin_a.clone();
        let coins = vec![coin_a, coin_b];

        let target = Amount::from_sat(1100);
        let fee_rate = FeeRate::from_sat_per_vb(5).unwrap();
        let recipient = ScriptBuf::new_p2tr(&s, kp().x_only_public_key().0, None);
        let change_addr = recipient.clone();

        let tx =
            build_with_change_possible(&coins, target, &recipient, &change_addr, kp(), fee_rate)
                .unwrap();

        let outputs = &tx.output;
        let inputs = &tx.input;

        assert_eq!(outputs[0].value, target);
        // with change possible was called, but no change was created.
        assert_eq!(1, tx.output.len());

        let input_sum = calc_input_sum(inputs, coins);
        let tx_fee = calc_fee(fee_rate, &tx.input, &[&recipient]).unwrap();
        assert!(input_sum > target + tx_fee);
    }

    #[test]
    fn valid_build_tx_with_change() {
        let tx_2462 = bitcoin_tx(get_transaction("2462"));
        let tx_8926 = bitcoin_tx(get_transaction("8926"));

        let coin_2462 = tx_coin(&tx_2462);
        let coin_8926 = tx_coin(&tx_8926);
        let coins = vec![coin_2462, coin_8926];

        let target = Amount::from_sat(42);
        let fee_rate = FeeRate::from_sat_per_vb(5).unwrap();
        let change_addr = &coins[0].tx_out.script_pubkey;
        let recipient = ScriptBuf::new();
        let tx =
            build_with_change_possible(&coins, target, &recipient, change_addr, kp(), fee_rate)
                .unwrap();
        let inputs = &tx.input;
        let outputs = &tx.output;
        assert!(!inputs.is_empty());
        assert!(outputs.len() == 2);

        let tx_fee = calc_fee(fee_rate, &tx.input, &[&recipient, &change_addr]).unwrap();

        let input_sum = calc_input_sum(inputs, coins);
        assert!(input_sum + tx_fee >= target);

        let output_sum: Amount = outputs.iter().map(|o| o.value).sum();
        assert_eq!(input_sum, output_sum + tx_fee);

        let recipient_output = outputs[0].clone();
        let change_output = outputs[1].clone();

        assert_eq!(recipient_output.value, target);
        assert_eq!(change_output.value, input_sum - tx_fee - recipient_output.value);
    }

    #[test]
    fn bnb_target() {
        let tx_2462 = bitcoin_tx(get_transaction("2462"));
        let coin = tx_coin(&tx_2462);
        let coins = vec![coin];
        let target = coins[0].value();

        let fee_rate = FeeRate::ZERO;
        let recipient = ScriptBuf::new();
        let change_addr = ScriptBuf::new();

        let tx =
            build_with_change_possible(&coins, target, &recipient, &change_addr, kp(), fee_rate)
                .unwrap();

        let inputs = tx.input;
        let outputs = tx.output;
        assert!(outputs.len() == 1);

        let input_sum = calc_input_sum(&inputs, coins);
        assert!(input_sum == target);
    }

    #[test]
    fn invalid_fee_rate_tx_without_change() {
        let tx = bitcoin_tx(get_transaction("2462"));
        let coin = tx_coin(&tx);

        let fee_rate = FeeRate::MAX;
        let tx = build_without_change(&[coin], &ScriptBuf::new(), kp(), fee_rate);
        assert!(tx.is_none());
    }

    #[test]
    fn invalid_fee_rate_tx_with_change() {
        let target = Amount::ZERO;
        let recipient = ScriptBuf::new();
        let change_addr = ScriptBuf::new();
        let fee_rate = FeeRate::MAX;
        let tx = build_with_change_possible(&[], target, &recipient, &change_addr, kp(), fee_rate);
        assert!(tx.is_none());
    }

    #[test]
    fn val_less_than_fee() {
        let tx = bitcoin_tx(get_transaction("2462"));
        let outpoint = OutPoint { txid: tx.compute_txid(), vout: 0 };

        let tx_out = TxOut { value: Amount::ZERO, script_pubkey: ScriptBuf::new() };

        let fee_rate = FeeRate::from_sat_per_vb(10).unwrap();
        let coin = Coin { outpoint, tx_out };
        let tx = build_without_change(&[coin], &ScriptBuf::new(), kp(), fee_rate);
        assert!(tx.is_none());
    }

    #[test]
    fn no_solution() {
        let coin = vec![];
        let target = Amount::from_sat(42);
        let recipient = ScriptBuf::new();
        let change_addr = ScriptBuf::new();
        let fee_rate = FeeRate::ZERO;

        let tx =
            build_with_change_possible(&coin, target, &recipient, &change_addr, kp(), fee_rate);
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

    #[test]
    fn arb_build_with_change_possible() {
        arbtest(|u| {
            let coins: Vec<Coin> = Vec::arbitrary(u)?;
            let target = Amount::arbitrary(u)?;
            let recipient = ScriptBuf::arbitrary(u)?;
            let change_addr = ScriptBuf::arbitrary(u)?;
            let fee_rate: FeeRate = FeeRate::arbitrary(u)?;
            let tx = build_with_change_possible(
                &coins,
                target,
                &recipient,
                &change_addr,
                kp(),
                fee_rate,
            );

            let discard_fee_rate = DEFAULT_DISCARD_FEE_RATE;
            let cost_of_change = default_tr_cost_of_change(fee_rate, discard_fee_rate);

            match tx {
                Some(t) => {
                    let inputs = &t.input;
                    let outputs = &t.output;

                    assert!(!inputs.is_empty());
                    assert!(outputs.len() == 1 || outputs.len() == 2);

                    let tx_fee = FeeRate::fee_wu(fee_rate, t.weight()).unwrap();
                    let input_sum = calc_input_sum(inputs, coins);

                    let output_sum: Amount = outputs.iter().map(|o| o.value).sum();

                    let recipient_output = outputs[0].clone();
                    assert_eq!(recipient_output.value, target);

                    // check if solution is not a bnb solution (had change)
                    if input_sum > target + cost_of_change {
                        assert_eq!(input_sum, output_sum + tx_fee);
                        let change_output = outputs[1].clone();
                        assert_eq!(
                            change_output.value,
                            input_sum - tx_fee - recipient_output.value
                        );
                    } else {
                        assert!(input_sum + tx_fee >= target);
                        assert!(outputs.len() == 1);
                    }
                }
                None => {
                    let base_tx_fee = calc_fee(fee_rate, &[], &[&recipient, &change_addr]);
                    if let Some(fee) = base_tx_fee {
                        let total_target = target + fee;
                        let srd_result = single_random_draw(total_target, fee_rate, &coins);
                        assert!(srd_result.is_none());
                    }
                }
            }

            Ok(())
        });
    }
}
