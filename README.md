# Rutabaga Wallet

Rutabaga has a swollen taproot.  As such, this is a swollen Taproot (p2tr) bitcoin wallet.

**Note that this is a work in progress.  It only works on the Bitcoin Signet network currently.**

## Architecture

Rutabaga is a Taproot only Bitcoin wallet developed with the Rust Programming Language.  It connects
directly to the bitcoin p2p network to read the chainstate using no intermediate interface.
Currently for transaction broadcasting, an intermediate connection via esplora over https is used.
Esplora is used initially for rapid development and testing, however this too could be done entirely
over the p2p network without intermediaries in the future.

Rutabaga has two binary files, a wallet binary used for key generation/bitcoin spending, and a
node binary, used to track changes to the wallet balance over time.  In order to get the most up to
date information, the node must be run.  Otherwise, wallet commands might show outdated information.

The two binarys can be invoked using --bin:
* cargo run --bin wallet
* cargo run --bin node

There are three data files that the wallet uses:
* keysfile used for signing and finding Bitcoin to spend.
* outputs file, used to store outputs found when scanning the blockchain.
* spents file, which tracks which outputs where spent.

**It is very important to keep the keyfile safe.  All other files can be rebuilt, but the keys file 
can not be recovered if lost or deleted.  Furthermore, if the keys file is compromised or stolen by
another party, they can spend your bitcoin.**

Additionally, there is a fourth file file used by the node as a data_file.  This data_file is used
to keep track of which blocks in the blockchain have already been processed for the purpose of
counting spent and unspent outputs (bitcoins).

Always begin by first running the node and waiting until it is "up to date", then run any wallet
commands.

# Node Cheatsheet

run node
```
cargo run --bin node 91.192.45.220:38333 ~/rutabaga ~/rutabaga_keys ~/rutabaga_output_ledger ~/rutabaga_spent_ledger
```

## Wallet Cheatsheet

Generate Address
```
cargo run --bin wallet generate-address
```

Generate Address to File
```
cargo run --bin wallet generate-address --out /tmp/out
```

Print Keys File
```
cargo run --bin wallet print-keys-from-keys-file ~/rutabaga_keys
```

show ledger outputs 
```
cargo run --bin wallet print-outputs ~/rutabaga_output_ledger
```

show ledger spent outputs
```
cargo run --bin wallet print-spent-outputs ~/rutabaga_spent_ledger
```

show ledger UTXOs
```
cargo run --bin wallet print-utxos ~/rutabaga_output_ledger ~/rutabaga_spent_ledger
```

print balance
```
cargo run --bin wallet print-balance ~/rutabaga_output_ledger ~/rutabaga_spent_ledger
```

spend utxo 0,1,2 at 5 sats/vB
```
cargo run --bin wallet spend-utxos 0,1,2 ~/rutabaga_output_ledger ~/rutabaga_spent_ledger ~/rutabaga_keys recipient_addr 5
```

spend 3100 sats at 5 sats/vB using automatic coin-selection
```
cargo run --bin wallet spend 3100 ~/rutabaga_output_ledger ~/rutabaga_spent_ledger ~/rutabaga_keys recipient_addr 5
```
