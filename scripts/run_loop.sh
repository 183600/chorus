#!/usr/bin/env bash
set -euo pipefail

need_cmd() { command -v "$1" >/dev/null 2>&1 || { echo "Missing required command: $1" >&2; exit 2; }; }

need_cmd git
need_cmd node
need_cmd npm
need_cmd iflow
need_cmd jq
need_cmd mktemp
need_cmd date

# Defaults
MAIN_BRANCH="main"
REMOTE="origin"
PUSH="false"
ALLOW_DIRTY="false"
INSTALL_IFLOW="false"
SKIP_SMOKE="false"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --main-branch) MAIN_BRANCH="$2"; shift 2;;
    --remote) REMOTE="$2"; shift 2;;
    --push) PUSH="true"; shift 1;;
    --allow-dirty) ALLOW_DIRTY="true"; shift 1;;
    --install-iflow) INSTALL_IFLOW="true"; shift 1;;
    --skip-smoke) SKIP_SMOKE="true"; shift 1;;
    -h|--help)
      cat <<EOF
Usage: ./run_loop.sh [options]
  --main-branch <name>   default: main
  --remote <name>        default: origin
  --push                 push idea branch + main
  --allow-dirty          allow dirty git tree
  --install-iflow        npm i -g @iflow-ai/iflow-cli@latest
  --skip-smoke           skip iflow smoke test
EOF
      exit 0
      ;;
    *)
      echo "Unknown arg: $1" >&2
      exit 2
      ;;
  esac
done

# Align env defaults to NVIDIA Integrate (do not override user's exports)
export IFLOW_BASE_URL="${IFLOW_BASE_URL:-https://integrate.api.nvidia.com/v1}"
export IFLOW_MODEL_NAME="${IFLOW_MODEL_NAME:-moonshotai/kimi-k2-thinking}"
export IDEA_MAX_ROUNDS="${IDEA_MAX_ROUNDS:-8}"

# Map NVIDIA_API_KEY -> IFLOW_API_KEY if needed (for scripts + iflow-cli)
if [[ -z "${IFLOW_API_KEY:-}" ]]; then
  if [[ -n "${NVIDIA_API_KEY:-}" ]]; then
    export IFLOW_API_KEY="${NVIDIA_API_KEY}"
  elif [[ -n "${OPENAI_API_KEY:-}" ]]; then
    export IFLOW_API_KEY="${OPENAI_API_KEY}"
  fi
fi
if [[ -z "${IFLOW_API_KEY:-}" ]]; then
  echo "Missing NVIDIA_API_KEY (or IFLOW_API_KEY / OPENAI_API_KEY)." >&2
  exit 2
fi

repo_root="$(pwd)"

must_exist() {
  local p="$1"
  local hint="$2"
  [[ -f "$p" ]] || { echo "Missing required file: $p" >&2; echo "Hint: $hint" >&2; exit 1; }
}

must_exist "${repo_root}/scripts/generate_idea.sh" "Make sure scripts/generate_idea.sh exists."
must_exist "${repo_root}/scripts/update_results.sh" "Make sure scripts/update_results.sh exists."
must_exist "${repo_root}/scripts/judge_and_prune.sh" "Make sure scripts/judge_and_prune.sh exists."

current_branch() {
  git rev-parse --abbrev-ref HEAD
}

is_worktree_clean() {
  [[ -z "$(git status --porcelain)" ]]
}

ensure_results_file() {
  local results_path="$1"
  mkdir -p "$(dirname "$results_path")"
  if [[ ! -f "$results_path" ]]; then
    printf '[]' >"$results_path"
    return 0
  fi
  if ! jq -e . "$results_path" >/dev/null 2>&1; then
    echo "$results_path exists but is not valid JSON." >&2
    exit 1
  fi
}

run_cmd() {
  echo
  echo "\$ $*"
  "$@"
}

if [[ "$ALLOW_DIRTY" != "true" ]]; then
  if ! is_worktree_clean; then
    echo "Git working tree is not clean. Commit/stash your changes or pass --allow-dirty." >&2
    exit 1
  fi
fi

branch_name="idea-$(date -u +"%Y%m%d-%H%M%S")"
echo "[info] branch_name = ${branch_name}"

runner_temp="$(mktemp -d -t auto-idea-runner-temp-XXXXXX)"
echo "[info] runner_temp = ${runner_temp}"

idea_txt="${runner_temp}/idea.txt"
prompt_txt="${runner_temp}/prompt.txt"
branch_sha_txt="${runner_temp}/branch_sha.txt"

results_json="${repo_root}/results/ideas.json"
ensure_results_file "$results_json"

start_branch="$(current_branch)"
echo "[info] start_branch = ${start_branch}"

cleanup() {
  rm -rf "$runner_temp" 2>/dev/null || true
  # best-effort checkout back
  if [[ "$(current_branch 2>/dev/null || true)" != "$start_branch" ]]; then
    git checkout "$start_branch" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

# 1) Generate idea
run_cmd bash "${repo_root}/scripts/generate_idea.sh" "$idea_txt"
[[ -f "$idea_txt" ]] || { echo "generate_idea.sh did not create $idea_txt" >&2; exit 1; }

# 2) Create new branch + git identity
run_cmd git config user.name "github-actions[bot]"
run_cmd git config user.email "github-actions[bot]@users.noreply.github.com"
run_cmd git checkout -b "$branch_name"

# 3) Node / npm / iflow
run_cmd node --version
run_cmd npm --version

if [[ "$INSTALL_IFLOW" == "true" ]]; then
  run_cmd npm i -g @iflow-ai/iflow-cli@latest
fi

run_cmd iflow --version

# 4) Smoke test
if [[ "$SKIP_SMOKE" != "true" ]]; then
  run_cmd iflow "Create a file IFLOW_SMOKE_TEST.md with content 'ok'. think:high" --yolo
  [[ -f "${repo_root}/IFLOW_SMOKE_TEST.md" ]] || { echo "Smoke test failed: IFLOW_SMOKE_TEST.md not found." >&2; exit 1; }
fi

# 5) iFlow implement the idea
idea="$(cat "$idea_txt")"

cat >"$prompt_txt" <<EOF
你将把下面这个"提升 LLM 智力的全新 workflow 点子"落地为一个可运行的最小原型仓库。

要求：
1) 生成 README.md：讲清楚 workflow 的目标、步骤、输入输出、如何运行
2) 给出一个最小可运行 demo（例如 scripts/ 或 src/），能在 CI 上执行
3) 提供一个简单的自检（例如运行脚本输出关键结果）
4) 不要泄露任何密钥，不要把 ~/.codex 写入仓库

点子如下（原样粘贴）：
${idea}
EOF

prompt_content="$(cat "$prompt_txt")"
run_cmd iflow "${prompt_content} think:high" --yolo

# git add/commit (allow empty)
run_cmd git add -A
git commit -m "feat: implement idea via iflow" >/dev/null 2>&1 || true

sha="$(git rev-parse HEAD)"
echo "$sha" >"$branch_sha_txt"
echo "[info] branch sha = ${sha}"

# 6) Push idea branch
if [[ "$PUSH" == "true" ]]; then
  if [[ -n "${GH_PAT:-}" ]]; then
    remote_url="$(git remote get-url "$REMOTE")"
    if [[ "$remote_url" == https://*github.com* ]]; then
      token_url="${remote_url/https:\/\//https:\/\/x-access-token:${GH_PAT}@}"
      git remote set-url "$REMOTE" "$token_url"
    fi
  fi
  run_cmd git push -u "$REMOTE" "$branch_name"
fi

# 7) Switch back to main and pull (only when push enabled)
run_cmd git checkout "$MAIN_BRANCH"
if [[ "$PUSH" == "true" ]]; then
  run_cmd git pull "$REMOTE" "$MAIN_BRANCH"
fi

# 8) Append result to results/ideas.json
run_cmd bash "${repo_root}/scripts/update_results.sh" "$results_json" "$branch_name" "$idea_txt" "$sha"

# 9) Judge and prune
run_cmd bash "${repo_root}/scripts/judge_and_prune.sh" "$results_json"

# 10) Commit and push main (allow empty)
run_cmd git add "$results_json"
git commit -m "chore: record idea result (${branch_name})" >/dev/null 2>&1 || true

if [[ "$PUSH" == "true" ]]; then
  run_cmd git push "$REMOTE" "$MAIN_BRANCH"
fi

echo
echo "[done] one iteration completed successfully."