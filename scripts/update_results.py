#!/usr/bin/env python3
import json
import os
import sys
from datetime import datetime, timezone

def main():
    if len(sys.argv) < 4:
        print("Usage: update_results.py <json_path> <branch_name> <idea_file> [branch_sha]", file=sys.stderr)
        sys.exit(2)

    json_path = sys.argv[1]
    branch_name = sys.argv[2]
    idea_file = sys.argv[3]
    branch_sha = sys.argv[4] if len(sys.argv) >= 5 else ""

    with open(idea_file, "r", encoding="utf-8") as f:
        idea = f.read().strip()

    ts = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")

    if os.path.exists(json_path):
        with open(json_path, "r", encoding="utf-8") as f:
            data = json.load(f)
        if not isinstance(data, list):
            raise ValueError("ideas.json must be a JSON array")
    else:
        data = []

    data.append({
        "ts_utc": ts,
        "branch": branch_name,
        "branch_sha": branch_sha,
        "idea": idea
    })

    os.makedirs(os.path.dirname(json_path), exist_ok=True)
    with open(json_path, "w", encoding="utf-8") as f:
        json.dump(data, f, ensure_ascii=False, indent=2)

if __name__ == "__main__":
    main()