#!/usr/bin/env bash

# no longer necessary; see rust-toolchain.toml
#rustup toolchain install nightly
#rustup override set nightly

rustup component add rustfmt
rustup component add clippy
cargo build --all-targets

# possible to make dependencies part of the image, without building it?
# https://stackoverflow.com/questions/58473606/cache-rust-dependencies-with-docker-build