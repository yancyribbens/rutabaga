# rutabaga

```bash
cargo run --bin cli wallet
```

# cli

print keys file
```
cargo run --bin cli wallet print-keys-from-keys-file ~/rutabaga_key
```

# node

run node with wallet env vars
```
RUTABAGA_KEY_FILE=~/rutabaga_keys RUTABAGA_LEDGER_FILE=~/rutabaga_output_ledger cargo run --bin node --release -- --network signet
```
