set shell := ["zsh", "-cu"]

default:
    @just --list

run:
    cargo run

fmt:
    cargo fmt --all

clippy:
    cargo clippy --all-targets --all-features -- -D warnings

test:
    cargo test

check: fmt clippy test
