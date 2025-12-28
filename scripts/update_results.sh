#!/usr/bin/env bash
set -euo pipefail

need_cmd() { command -v "$1" >/dev/null 2>&1 || { echo "Missing required command: $1" >&2; exit 2; }; }
need_cmd jq
need_cmd date

if [[ $# -lt 3 ]]; then
  echo "Usage: update_results.sh <json_path> <branch_name> <idea_file> [branch_sha]" >&2
  exit 2
fi

json_path="$1"
branch_name="$2"
idea_file="$3"
branch_sha="${4:-}"

if [[ ! -f "$idea_file" ]]; then
  echo "Missing idea_file: $idea_file" >&2
  exit 2
fi

ts="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

mkdir -p "$(dirname "$json_path")" 2>/dev/null || true

if [[ -f "$json_path" ]]; then
  # validate it's JSON
  if ! jq -e . "$json_path" >/dev/null 2>&1; then
    echo "$json_path exists but is not valid JSON." >&2
    exit 1
  fi
  # validate array
  if ! jq -e 'type=="array"' "$json_path" >/dev/null 2>&1; then
    echo "ideas.json must be a JSON array" >&2
    exit 1
  fi
else
  printf '[]' >"$json_path"
fi

tmp="$(mktemp)"

jq \
  --arg ts "$ts" \
  --arg branch "$branch_name" \
  --arg sha "$branch_sha" \
  --rawfile idea "$idea_file" \
  '. + [{
    ts_utc: $ts,
    branch: $branch,
    branch_sha: $sha,
    idea: ($idea | gsub("\\r$";"") | rtrimstr("\n"))
  }]' \
  "$json_path" >"$tmp"

mv "$tmp" "$json_path"