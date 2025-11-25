all: clippy updeps debug

pr: checks tests

checks: format clippy publish udeps

tests: debug publish test

format:
	cargo fmt --all -- --check

clippy:
	cargo clippy --tests --no-deps --all-features --all-targets

publish:
	cargo publish --dry-run

test:
	cargo test

updeps:
	cargo +nightly udeps

debug:
	cargo build

release:
	cargo build --release

run:
	cargo run --release