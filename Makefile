all: clippy udeps debug

pr: checks tests

checks: format clippy publish udeps todos

tests: debug release publish test

format:
	cargo fmt --all -- --check

clippy:
	cargo clippy --tests --no-deps --all-features --all-targets

publish: release
	cargo publish --dry-run --no-verify --allow-dirty

test:
	cargo test

udeps:
	cargo +nightly udeps

todos:
	find . -name "*.rs" -exec grep "TODO" {} \; -print

debug:
	cargo build

release:
	cargo build --release

run:
	cargo run --release

bundle:
	cargo bundle --release