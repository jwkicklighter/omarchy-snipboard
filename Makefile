.PHONY: build test validate benchmark package bundle
build:
	cargo build --release --locked
test:
	cargo test --release --locked
	python3 -m unittest discover -s tests -p 'test_*.py'
validate: build
	build/release/snipboard-tools validate
benchmark: build
	build/release/snipboard-tools benchmark --rounds 30
package:
	python3 tools/package.py --smoke-test
bundle: build
	python3 tools/bundle.py --write --worker build/release/snipboard-worker
