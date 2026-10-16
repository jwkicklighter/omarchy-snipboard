export PYTHONDONTWRITEBYTECODE=1
.PHONY: build test validate benchmark
build:
	cmake -S . -B build -DCMAKE_BUILD_TYPE=Release
	cmake --build build --parallel 2
test: build
	python -m unittest discover -s tests -v
validate:
	python -m tools.validate
benchmark: build
	python -m tools.benchmark --rounds 30
