#!/usr/bin/env python3
import json
import sys


def load(path):
    with open(path, "r", encoding="utf-8") as fh:
        data = json.load(fh)
    if isinstance(data, dict):
        data.pop("_metadata", None)
    return data


def main():
    if len(sys.argv) != 3:
        print("usage: compare_json.py ACTUAL GOLDEN", file=sys.stderr)
        return 2
    actual = load(sys.argv[1])
    golden = load(sys.argv[2])
    if actual != golden:
        print(f"actual != golden for {sys.argv[2]}", file=sys.stderr)
        print("actual:", json.dumps(actual, sort_keys=True), file=sys.stderr)
        print("golden:", json.dumps(golden, sort_keys=True), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
