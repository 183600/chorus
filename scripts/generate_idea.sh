#!/usr/bin/env bash
set -euo pipefail

# Defaults (can be overridden by env)
BASE_URL="${IFLOW_BASE_URL:-https://integrate.api.nvidia.com/v1}"
BASE_URL="${BASE_URL%/}"
MODEL="${IFLOW_MODEL_NAME:-moonshotai/kimi-k2-thinking}"

API_KEY="${NVIDIA_API_KEY:-${IFLOW_API_KEY:-${OPENAI_API_KEY:-}}}"
if [[ -z "${API_KEY}" ]]; then
  echo "Missing NVIDIA_API_KEY (or IFLOW_API_KEY / OPENAI_API_KEY) in env." >&2
  exit 2
fi

CORE_QUESTION="发明一个目前没有被发明的workflow，大幅提升llm智力，可以落地"

FINAL_FORMAT=$'- **最关键刺激词**：[…]\n- **词汇特性提取**：[…]\n- **创意映射（逻辑同构）**：[…]\n- **最终狂野点子（一个具体方案）**：[…]\n- **落地第一步（48小时内可做的最小实验）**：[…]\n'

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || { echo "Missing required command: $1" >&2; exit 2; }
}
need_cmd curl
need_cmd jq
need_cmd perl

call_chat_completions() {
  local system="$1"
  local user="$2"
  local temperature="$3"
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

  # capture http code reliably
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
  # Prefer OpenAI chat.completions shape; keep iflow compatibility.
  text="$(printf '%s' "$resp_json" | jq -r '
    if (.choices?[0]?.message?.content? != null) then .choices[0].message.content
    elif (.choices?[0]?.delta?.content? != null) then .choices[0].delta.content
    elif (.output_text? != null) then .output_text
    elif (.output? != null) then ([.output[]?.content[]? | select(.text!=null) | .text] | join(""))
    else ""
    end
  ' 2>/dev/null || true)"

  if [[ -z "$text" ]]; then
    # fallback to raw JSON for debugging
    printf '%s' "$resp_json"
  else
    # trim
    printf '%s' "$text" | sed -e 's/^[[:space:]]\+//' -e 's/[[:space:]]\+$//'
  fi
}

# Try to sanitize and extract first JSON object/array from a text blob.
extract_first_json() {
  local s="$1"

  # remove common markdown fences
  s="$(printf '%s' "$s" | sed -e '1s/^```json[[:space:]]*//' -e '1s/^```[[:space:]]*//' -e '$s/[[:space:]]*```$//')"

  # If already valid JSON, print it.
  if printf '%s' "$s" | jq -e . >/dev/null 2>&1; then
    printf '%s' "$s"
    return 0
  fi

  # perl: slurp and capture first {...} or [...]
  local extracted
  extracted="$(printf '%s' "$s" | perl -0777 -ne '
    if (/\G.*?(\{(?:[^{}]++|(?1))*\}|\[(?:[^\[\]]++|(?1))*\])/s) { print $1; }
    elsif(/(\{.*\}|\[.*\])/s){ print $1; }
  ' || true)"

  if [[ -z "$extracted" ]]; then
    echo "Failed to extract JSON from model output (head): $(printf '%s' "$s" | head -c 200)" >&2
    return 1
  fi

  if ! printf '%s' "$extracted" | jq -e . >/dev/null 2>&1; then
    echo "Extracted JSON is not valid (head): $(printf '%s' "$extracted" | head -c 200)" >&2
    return 1
  fi

  printf '%s' "$extracted"
}

round_trip() {
  local round_idx="$1"
  local tmpdir="$2"

  local sys_prompt
  sys_prompt="你是一个严格遵循指令的创意生成与评审引擎。你必须隐藏中间过程，只有在被要求时才输出指定格式。"

  # -------- Call 1 --------
  local user1
  user1="$(cat <<EOF
为下面的核心问题生成一套“高熵词库”：
核心问题：${CORE_QUESTION}

要求：严格按比例输出 10 个词：
- 3个具体名词（独特物理结构）
- 3个抽象概念（哲学/科学术语）
- 2个特定动作（强动态动词）
- 2个跨界术语（生物/建筑/军事等专有名词）

只允许输出纯 JSON 字符串，不要使用 Markdown 代码块（不要包含 \`\`\`json 或 \`\`\`），不要输出任何多余文字。
格式：
{
  "nouns": [...],
  "abstracts": [...],
  "actions": [...],
  "cross": [...]
}
EOF
)"

  local resp1 text1 words_json
  resp1="$(call_chat_completions "$sys_prompt" "$user1" "1.0" "180")"
  text1="$(extract_text_from_response_json "$resp1")"
  words_json="$(extract_first_json "$text1")"

  # validate keys
  if ! printf '%s' "$words_json" | jq -e '.nouns and .abstracts and .actions and .cross' >/dev/null; then
    echo "Round ${round_idx} Call1: missing keys in JSON: $words_json" >&2
    return 1
  fi

  # build words list
  local words_list_json
  words_list_json="$(printf '%s' "$words_json" | jq -c '[.nouns[], .abstracts[], .actions[], .cross[]]')"

  # -------- Call 2 --------
  local user2
  user2="$(cat <<EOF
你将使用“随机词刺激法”为核心问题生成候选方案。
核心问题：${CORE_QUESTION}

刺激词列表（共10个）：
${words_list_json}

对每个刺激词生成1个候选方案（共10个），每个候选包含：
- stimulus_word
- word_traits
- mapping
- proposal
- first_48h_experiment

只允许输出纯 JSON 字符串，不要使用 Markdown 代码块，不要输出任何多余文字。
格式：
{
  "candidates": [
    {
      "stimulus_word": "...",
      "word_traits": "...",
      "mapping": "...",
      "proposal": "...",
      "first_48h_experiment": "..."
    }
  ]
}
EOF
)"

  local resp2 text2 cand_json
  resp2="$(call_chat_completions "$sys_prompt" "$user2" "0.95" "180")"
  text2="$(extract_text_from_response_json "$resp2")"
  cand_json="$(extract_first_json "$text2")"

  if ! printf '%s' "$cand_json" | jq -e '.candidates and (.candidates|type=="array")' >/dev/null; then
    echo "Round ${round_idx} Call2: invalid candidates JSON: $cand_json" >&2
    return 1
  fi

  # -------- Call 3 --------
  local user3
  user3="$(cat <<EOF
你将从候选中选出“一个最优方案”，并且必须满足：
- 新颖性、贴合度、可落地、杀伤力 四项同时达标
- 最终只输出1个方案
- 不得输出任何中间过程

候选如下（JSON）：
${cand_json}

现在请严格按以下格式输出（只输出一次，且只输出一个方案）：
${FINAL_FORMAT}
EOF
)"

  local resp3 final_text
  resp3="$(call_chat_completions "$sys_prompt" "$user3" "0.75" "180")"
  final_text="$(extract_text_from_response_json "$resp3")"
  final_text="$(printf '%s' "$final_text" | sed -e 's/^[[:space:]]\+//' -e 's/[[:space:]]\+$//')"

  # -------- Call 4 --------
  local user4
  user4="$(cat <<EOF
请你对“最终输出方案”做严格自评，判断是否同时满足四项：
新颖性、贴合度、可落地、杀伤力。

最终输出方案如下：
${final_text}

只允许输出纯 JSON 字符串，不要使用 Markdown 代码块，不要输出任何多余文字。
格式：
{
  "pass": true/false,
  "scores": {
    "novelty": 0-10,
    "fit": 0-10,
    "feasibility": 0-10,
    "impact": 0-10
  },
  "why": "一句话理由"
}
EOF
)"

  local resp4 text4 eval_json
  resp4="$(call_chat_completions "$sys_prompt" "$user4" "0.2" "180")"
  text4="$(extract_text_from_response_json "$resp4")"
  eval_json="$(extract_first_json "$text4")"

  if ! printf '%s' "$eval_json" | jq -e '.pass != null and .scores != null' >/dev/null; then
    echo "Round ${round_idx} Call4: invalid eval JSON: $eval_json" >&2
    return 1
  fi

  # outputs to temp files
  printf '%s' "$final_text" >"${tmpdir}/final_text.txt"
  printf '%s' "$eval_json"  >"${tmpdir}/eval.json"
}

main() {
  local out_path="${1:-idea.txt}"
  local max_rounds="${IDEA_MAX_ROUNDS:-8}"

  local best_text=""
  local best_score="-1"
  local best_eval=""

  local i tmpdir total pass
  for i in $(seq 1 "$max_rounds"); do
    tmpdir="$(mktemp -d)"
    if round_trip "$i" "$tmpdir"; then
      local final_text eval_json
      final_text="$(cat "${tmpdir}/final_text.txt")"
      eval_json="$(cat "${tmpdir}/eval.json")"

      total="$(printf '%s' "$eval_json" | jq -r '
        ( .scores.novelty // 0 ) +
        ( .scores.fit // 0 ) +
        ( .scores.feasibility // 0 ) +
        ( .scores.impact // 0 )
      ' 2>/dev/null || echo 0)"

      # numeric compare using awk
      if awk "BEGIN{exit !(${total} > ${best_score})}"; then
        best_text="$final_text"
        best_score="$total"
        best_eval="$eval_json"
      fi

      pass="$(printf '%s' "$eval_json" | jq -r '.pass' 2>/dev/null || echo false)"
      if [[ "$pass" == "true" ]]; then
        rm -rf "$tmpdir"
        break
      fi

      rm -rf "$tmpdir"
      sleep 0.5
    else
      rm -rf "$tmpdir"
      echo "Round ${i} failed." >&2
      sleep 1
      continue
    fi
  done

  if [[ -z "$best_text" ]]; then
    echo "Failed to generate idea after retries." >&2
    exit 1
  fi

  mkdir -p "$(dirname "$out_path")" 2>/dev/null || true
  printf '%s\n' "$best_text" >"$out_path"
  printf '%s\n' "$best_text"
}

main "${1:-}"