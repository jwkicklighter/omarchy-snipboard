"""Matching and persistent-worker round trips, explicitly NOT visible UI latency."""
import argparse
import json
import math
import platform
from pathlib import Path
import subprocess
import sys
import tempfile
import time

from snipboard.core import Index, synthetic_library, synthetic_records
from snipboard.native import NativeIndex


def summary(samples):
    ordered = sorted(samples)
    return {"n": len(samples), **{name: round(ordered[max(0, math.ceil(len(samples) * p) - 1)], 3)
                                 for name, p in (("p50_ms", .5), ("p95_ms", .95), ("p99_ms", .99))}}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--rounds", type=int, default=30)
    parser.add_argument("--reference", action="store_true", help="benchmark Python reference instead of native")
    args = parser.parse_args()
    records = synthetic_records(synthetic_library())
    index = (Index if args.reference else NativeIndex)(records)
    queries = ["", "e", "em", "ema", "email", "!eml", "prjnt", "project", "amber", "river", "999", "καλη", "zzzz"]
    for query in queries:
        index.search(query)
    samples = []
    for _ in range(args.rounds):
        for query in queries:
            start = time.perf_counter_ns()
            index.search(query)
            samples.append((time.perf_counter_ns() - start) / 1e6)
    with tempfile.TemporaryDirectory(dir="/tmp/opencode", prefix="snipboard-bench-") as data:
        start = time.perf_counter_ns()
        worker = subprocess.Popen([sys.executable, "-m", "snipboard.worker", "--data", data], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        json.loads(worker.stdout.readline())
        startup = (time.perf_counter_ns() - start) / 1e6
        trips, worker_matches = [], []
        for i in range(args.rounds * len(queries)):
            start = time.perf_counter_ns()
            worker.stdin.write(json.dumps({"op": "search", "seq": i, "query": queries[i % len(queries)]}) + "\n")
            worker.stdin.flush()
            result = json.loads(worker.stdout.readline())
            trips.append((time.perf_counter_ns() - start) / 1e6)
            worker_matches.append(result["matchMs"])
        worker.stdin.close()
        worker.wait(timeout=5)
        worker.stdout.close()
    print(json.dumps({"platform": platform.platform(), "python": platform.python_version(),
                      "matching_engine": "python-reference" if args.reference else "C++/QtCore",
                      "records": len(records), "payload_characters": sum(len(r["body"]) for r in records),
                      "index_limit_per_field": 512, "queries": queries,
                      "matching_only": summary(samples), "worker_matching_only": summary(worker_matches),
                      "sequential_worker_roundtrip_not_visible_latency": summary(trips),
                      "cold_worker_ready_ms": round(startup, 3),
                      "warm_open_visible_ms": None, "input_to_visible_ms": None}, indent=2))


if __name__ == "__main__":
    main()
