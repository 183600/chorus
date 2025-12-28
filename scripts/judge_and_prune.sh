#!/usr/bin/env bash
set -euo pipefail

# Defaults (can be overridden by env)
BASE_URL="${IFLOW_BASE_URL:-https://integrate.api.nvidia.com/v1}"
BASE_URL="${BASE_URL%/}"
MODEL="${IFLOW_MODEL_NAME:-moonshotai/kimi-k2-thinking}"

API_KEY="${NVIDIA_API_KEY:-${IFLOW_API_KEY:-${OPENAI_API_KEY:-}}}"
if [[ -z "${API_KEY}" ]]; then
  echo "Missing NVIDIA_API_KEY (or IFLOW_API_KEY / OPENAI_API_KEY)." >&2
  exit 2
fi

need_cmd() { command -v "$1" >/dev/null 2>&1 || { echo "Missing required command: $1" >&2; exit 2; }; }
need_cmd jq
need_cmd curl
need_cmd perl

call_chat_completions() {
  local system="$1"
  local user="$2"
  local temperature="${3:-0.2}"
  local timeout="${4:-180}"

  local url="${BASE_URL}/chat/completions"
  local payload
  payload="$(jq -n \
    --arg model "$MODEL" \
    --arg sys "$system" \
    --arg usr "$user" \
    --argjson temp "$temperature" \
    '{
      model: $model,
      messages: [
        {role:"system", content:$sys},
        {role:"user", content:$usr}
      ],
      temperature: $temp
    }'
  )"

  local resp http_code body
  resp="$(curl -sS -m "$timeout" \
    -H "Authorization: Bearer ${API_KEY}" \
    -H "Content-Type: application/json" \
    -d "$payload" \
    -w $'\n__HTTP_CODE__:%{http_code}' \
    "$url"
  )"
  http_code="${resp##*__HTTP_CODE__:}"
  body="${resp%$'\n__HTTP_CODE__:'*}"

  if [[ "$http_code" -lt 200 || "$http_code" -ge 300 ]]; then
    echo "API error HTTP ${http_code}: ${body}" >&2
    return 1
  fi

  printf '%s' "$body"
}

extract_text_from_response_json() {
  local resp_json="$1"
  local text
  text="$(printf '%s' "$resp_json" | jq -r '
    if (.choices?[0]?.message?.content? != null) then .choices[0].message.content
    elif (.choices?[0]?.delta?.content? != null) then .choices[0].delta.content
    elif (.output_text? != null) then .output_text
    elif (.output? != null) then ([.output[]?.content[]? | select(.text!=null) | .text] | join(""))
    else ""
    end
  ' 2>/dev/null || true)"
  if [[ -z "$text" ]]; then
    printf '%s' "$resp_json"
  else
    printf '%s' "$text" | sed -e 's/^[[:space:]]\+//' -e 's/[[:space:]]\+$//'
  fi
}

extract_first_json() {
  local s="$1"
  s="$(printf '%s' "$s" | sed -e '1s/^```json[[:space:]]*//' -e '1s/^```[[:space:]]*//' -e '$s/[[:space:]]*```$//')"

  if printf '%s' "$s" | jq -e . >/dev/null 2>&1; then
    printf '%s' "$s"
    return 0
  fi

  local extracted
  extracted="$(printf '%s' "$s" | perl -0777 -ne '
    if (/\G.*?(\{(?:[^{}]++|(?1))*\}|\[(?:[^\[\]]++|(?1))*\])/s) { print $1; }
    elsif(/(\{.*\}|\[.*\])/s){ print $1; }
  ' || true)"

  if [[ -z "$extracted" ]]; then
    echo "Failed to extract JSON from judge output (head): $(printf '%s' "$s" | head -c 200)" >&2
    return 1
  fi

  if ! printf '%s' "$extracted" | jq -e . >/dev/null 2>&1; then
    echo "Extracted judge JSON invalid (head): $(printf '%s' "$extracted" | head -c 200)" >&2
    return 1
  fi

  printf '%s' "$extracted"
}

if [[ $# -lt 1 ]]; then
  echo "Usage: judge_and_prune.sh <json_path>" >&2
  exit 2
fi

json_path="$1"
if [[ ! -f "$json_path" ]]; then
  echo "Missing file: $json_path" >&2
  exit 2
fi

# Need at least 2 entries
len="$(jq -r 'length' "$json_path" 2>/dev/null || echo 0)"
if [[ "$len" -lt 2 ]]; then
  exit 0
fi

prev_idea="$(jq -r '.[-2].idea // ""' "$json_path")"
last_idea="$(jq -r '.[-1].idea // ""' "$json_path")"

sys_prompt="你是严格的方案评审裁判，只能按要求输出 JSON。"
user_prompt="$(cat <<EOF
请比较两个方案，判断哪一个“整体更好”（四项同权重）：
- 新颖性
- 贴合度
- 可落地
- 杀伤力

方案A（prev）：
${prev_idea}

方案B（last）：
${last_idea}

只允许输出 JSON：
{
  "winner": "prev" 或 "last",
  "scores": {
    "prev": {"novelty":0-10,"fit":0-10,"feasibility":0-10,"impact":0-10},
    "last": {"novelty":0-10,"fit":0-10,"feasibility":0-10,"impact":0-10}
  },
  "why": "一句话原因"
}
不要输出任何多余文字。
EOF
)"

resp="$(call_chat_completions "$sys_prompt" "$user_prompt" "0.2" "180")"
text="$(extract_text_from_response_json "$resp")"
judge_json="$(extract_first_json "$text")"

# Update file: attach judge to last; if winner==last then delete prev
tmp="$(mktemp)"
jq --argjson judge "$judge_json" '
  (.[-1].judge = $judge) |
  if ($judge.winner == "last") then del(.[-2]) else . end
' "$json_path" >"$tmp"

mv "$tmp" "$json_path"