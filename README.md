# Rutabaga Wallet

```bash
cargo run --bin wallet wallet
```

# Wallet

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

# Node

run node with wallet env vars
```
RUTABAGA_KEY_FILE=~/rutabaga_keys RUTABAGA_LEDGER_FILE=~/rutabaga_output_ledger RUTABAGA_SPENT_FILE=~/rutabaga_spent_ledger cargo run --bin node --release -- --network signet
```

# Ledger

show ledger outputs 
```
cargo run --bin wallet print-outputs ~/rutabaga_output_ledger
```

show ledger spent outputs
```
cargo run --bin wallet print-spent-outputs ~/rutabaga_spent_ledger
```
