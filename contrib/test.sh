#!/bin/sh
#
# CI script for the `rutabaga` crate.

# -e (errexit) will immediately exit on script fail.
# -x (xtrace) enable execution tracing with expanded variables and arguments.
set -ex

# Sanity checks.
cargo --version
rustc --version

cargo build --all-features
cargo test --all-features

if [ "$DO_LINT" = true ]
then
    rustup component add clippy
    cargo clippy --all-features --all-targets -- -D warnings
fi
