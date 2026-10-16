"""Explicitly approved in-shell scratch/latency probes, never external OS input.

Run only after approving activation and scratch testing. The script calls fixed
Snipboard methods only and writes synthetic diagnostics, never clipboard data.
"""
import argparse
import json
from pathlib import Path
import subprocess
import time

from .benchmark import summary

ID = "io.github.jwkicklighter.snipboard"


def call(*arguments):
    return subprocess.check_output(["omarchy-shell", "shell", *arguments], text=True).strip()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("suite", choices=("scratch", "benchmark"))
    parser.add_argument("--output", type=Path, default=Path("/tmp/opencode/snipboard-probe.json"))
    args = parser.parse_args()
    print(call("summon", ID, "{}"))
    result = call("call", ID, "selfTest" if args.suite == "scratch" else "benchmark", "")
    if not result.startswith("started"):
        raise SystemExit(result)
    print(result)
    time.sleep(7 if args.suite == "scratch" else 28)
    metrics = json.loads(call("call", ID, "metrics", ""))
    args.output.write_text(json.dumps(metrics, indent=2) + "\n")
    if args.suite == "scratch":
        print(json.dumps({"phase": metrics["testPhase"], "results": metrics["tests"]}))
        if metrics["testPhase"] != "complete" or len(metrics["tests"]) != 4 or not all(t["passed"] for t in metrics["tests"]):
            raise SystemExit(1)
    else:
        print(json.dumps({key: summary(metrics[key]) for key in ("inputToModelAssignmentMs", "syntheticInputToFrameSwappedMs", "openToFrameSwappedMs")}, indent=2))
    print(f"Synthetic diagnostics: {args.output}")


if __name__ == "__main__":
    main()
