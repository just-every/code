#!/bin/bash

set -xe

cargo build
cargo build --no-default-features
cargo build --no-default-features --features managed
cargo build --no-default-features --features unmanaged
cargo build --no-default-features --features serde
cargo build --no-default-features --features managed,serde
cargo build --no-default-features --features unmanaged,serde
cargo build --no-default-features --features managed,unmanaged

(
	cd postgres
	cargo build
	cargo build --no-default-features
	cargo build --no-default-features --features rt_tokio_1
	cargo build --no-default-features --features rt_async-std_1
)

(
	cd redis
	cargo build
	cargo build --no-default-features --features rt_tokio_1
	cargo build --no-default-features --features rt_async-std_1
	cargo build --no-default-features --features rt_tokio_1 cluster
	cargo build --no-default-features --features rt_async-std_1 cluster
)

(
	cd lapin
	cargo build
	cargo build --no-default-features
	cargo build --no-default-features --features rt_tokio_1
	cargo build --no-default-features --features rt_async-std_1
)

(
	cd sqlite
	cargo build
	cargo build --no-default-features
)
