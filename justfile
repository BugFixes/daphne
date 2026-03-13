set shell := ["bash", "-cu"]

toolchain := "1.93.0"
cargo := env("HOME") + "/.cargo/bin/rustup run " + toolchain + " cargo"

default:
    @just --list

run:
    {{cargo}} run

fmt:
    {{cargo}} fmt --all

clippy:
    {{cargo}} clippy --all-targets --all-features -- -D warnings

test:
    {{cargo}} test --all-features

migrate:
    {{cargo}} run

check: fmt clippy test
