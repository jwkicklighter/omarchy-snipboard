.PHONY: build test validate benchmark
build:
	cargo build --release
test:
	cargo test --release
validate: build
	build/release/snipboard-tools validate
benchmark: build
	build/release/snipboard-tools benchmark --rounds 30
