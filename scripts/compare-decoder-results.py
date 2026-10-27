#!/usr/bin/env python3
"""Compare captured measurement-test directories or measure-example JSONL files."""

import argparse
import json
from pathlib import Path


def load(path):
    if path.is_dir():
        return {item.stem: json.loads(item.read_text()) for item in path.glob("*.json")}
    result = {}
    for line in path.read_text().splitlines():
        row = json.loads(line)
        if row["file"] in result:
            raise ValueError(f"duplicate input: {row['file']}")
        result[row["file"]] = row["result"]
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("before", type=Path)
    parser.add_argument("after", type=Path)
    args = parser.parse_args()
    before, after = load(args.before), load(args.after)
    if not before or before.keys() != after.keys():
        raise SystemExit("input sets differ or are empty")
    wording = []
    for name, old in sorted(before.items()):
        new = after[name]
        if old["accepted"] != new["accepted"]:
            raise SystemExit(f"accept/reject changed: {name}")
        if old["accepted"]:
            if old["payload"] != new["payload"]:
                raise SystemExit(f"AudioVerificationPayload changed: {name}")
        elif old["error"] != new["error"]:
            wording.append({"input": name, "before": old["error"], "after": new["error"]})
    print(json.dumps({
        "inputs": len(before),
        "accepted": sum(row["accepted"] for row in before.values()),
        "rejected": sum(not row["accepted"] for row in before.values()),
        "payloads_and_verdicts_equal": True,
        "error_wording_changes": wording,
    }, indent=2))


if __name__ == "__main__":
    main()
