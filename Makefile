all: clippy updeps debug

clippy:
	cargo clippy

updeps:
	cargo +nightly udeps

debug:
	cargo build

release:
	cargo build --release

run:
	cargo run