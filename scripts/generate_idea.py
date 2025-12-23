#!/usr/bin/env python3
import json
import os
import sys
import time
from typing import Any, Dict, Tuple

import requests

BASE_URL = os.getenv("IFLOW_BASE_URL", "https://apis.iflow.cn/v1").rstrip("/")
MODEL = os.getenv("IFLOW_MODEL", "glm-4.6")
API_KEY = os.getenv("IFLOW_API_KEY") or os.getenv("OPENAI_API_KEY")  # 兼容你 auth.json 的字段名

if not API_KEY:
    print("Missing IFLOW_API_KEY (or OPENAI_API_KEY) in env.", file=sys.stderr)
    sys.exit(2)

CORE_QUESTION = "发明一个目前没有被发明的workflow，大幅提升llm智力，可以落地"

FINAL_FORMAT = """- **最关键刺激词**：[…]
- **词汇特性提取**：[…]
- **创意映射（逻辑同构）**：[…]
- **最终狂野点子（一个具体方案）**：[…]
- **落地第一步（48小时内可做的最小实验）**：[…]
"""

def _extract_text(resp: Dict[str, Any]) -> str:
    # 尽量兼容 OpenAI Responses 风格的多种返回
    if isinstance(resp, dict):
        if "output_text" in resp and isinstance(resp["output_text"], str):
            return resp["output_text"].strip()
        out = resp.get("output")
        if isinstance(out, list):
            chunks = []
            for item in out:
                for c in item.get("content", []) if isinstance(item, dict) else []:
                    if isinstance(c, dict) and "text" in c:
                        chunks.append(c["text"])
            if chunks:
                return "".join(chunks).strip()
    return json.dumps(resp, ensure_ascii=False)

def call_responses(system: str, user: str, temperature: float = 0.9, timeout: int = 180) -> str:
    url = f"{BASE_URL}/responses"
    headers = {
        "Authorization": f"Bearer {API_KEY}",
        "Content-Type": "application/json",
    }
    payload = {
        "model": MODEL,
        "input": [
            {"role": "system", "content": [{"type": "text", "text": system}]},
            {"role": "user", "content": [{"type": "text", "text": user}]},
        ],
        "temperature": temperature,
    }
    r = requests.post(url, headers=headers, json=payload, timeout=timeout)
    r.raise_for_status()
    return _extract_text(r.json())

def parse_json_strict(s: str) -> Any:
    # 只取第一个 JSON 对象/数组，避免模型前后夹杂文本
    s2 = s.strip()
    first_obj = s2.find("{")
    first_arr = s2.find("[")
    if first_obj == -1 and first_arr == -1:
        raise ValueError("No JSON start found")
    start = first_obj if (first_obj != -1 and (first_arr == -1 or first_obj < first_arr)) else first_arr
    return json.loads(s2[start:])

def round_trip(round_idx: int) -> Tuple[str, Dict[str, Any]]:
    sys_prompt = (
        "你是一个严格遵循指令的创意生成与评审引擎。"
        "你必须隐藏中间过程，只有在被要求时才输出指定格式。"
    )

    # Call 1: 生成 10 个高熵词库（只返回 JSON）
    user1 = f"""
为下面的核心问题生成一套“高熵词库”：
核心问题：{CORE_QUESTION}

要求：严格按比例输出 10 个词：
- 3个具体名词（独特物理结构）
- 3个抽象概念（哲学/科学术语）
- 2个特定动作（强动态动词）
- 2个跨界术语（生物/建筑/军事等专有名词）

只允许输出 JSON，格式：
{{
  "nouns": [...],
  "abstracts": [...],
  "actions": [...],
  "cross": [...]
}}
不要输出任何多余文字。
""".strip()
    words_json = parse_json_strict(call_responses(sys_prompt, user1, temperature=1.0))
    nouns = words_json["nouns"]
    abstracts = words_json["abstracts"]
    actions = words_json["actions"]
    cross = words_json["cross"]
    words = nouns + abstracts + actions + cross

    # Call 2: 用 10 个词分别生成 10 个候选（只返回 JSON，不要展示思考过程）
    user2 = f"""
你将使用“随机词刺激法”为核心问题生成候选方案。
核心问题：{CORE_QUESTION}

刺激词列表（共10个）：
{json.dumps(words, ensure_ascii=False)}

对每个刺激词生成1个候选方案（共10个），每个候选包含：
- stimulus_word
- word_traits
- mapping
- proposal
- first_48h_experiment

只允许输出 JSON，格式：
{{
  "candidates": [
    {{
      "stimulus_word": "...",
      "word_traits": "...",
      "mapping": "...",
      "proposal": "...",
      "first_48h_experiment": "..."
    }}
  ]
}}
不要输出任何多余文字。
""".strip()
    cand_json = parse_json_strict(call_responses(sys_prompt, user2, temperature=0.95))

    # Call 3: 评审 + 只输出 1 个最终方案（严格按你给的格式）
    user3 = f"""
你将从候选中选出“一个最优方案”，并且必须满足：
- 新颖性、贴合度、可落地、杀伤力 四项同时达标
- 最终只输出1个方案
- 不得输出任何中间过程

候选如下（JSON）：
{json.dumps(cand_json, ensure_ascii=False)}

现在请严格按以下格式输出（只输出一次，且只输出一个方案）：
{FINAL_FORMAT}
""".strip()
    final_text = call_responses(sys_prompt, user3, temperature=0.75).strip()

    # Call 4: 让模型自评是否达标（只返回 JSON），用于决定是否需要再来一轮
    user4 = f"""
请你对“最终输出方案”做严格自评，判断是否同时满足四项：
新颖性、贴合度、可落地、杀伤力。

最终输出方案如下：
{final_text}

只允许输出 JSON：
{{
  "pass": true/false,
  "scores": {{
    "novelty": 0-10,
    "fit": 0-10,
    "feasibility": 0-10,
    "impact": 0-10
  }},
  "why": "一句话理由"
}}
不要输出任何多余文字。
""".strip()
    eval_json = parse_json_strict(call_responses(sys_prompt, user4, temperature=0.2))
    return final_text, eval_json

def main():
    out_path = sys.argv[1] if len(sys.argv) >= 2 else "idea.txt"
    max_rounds = int(os.getenv("IDEA_MAX_ROUNDS", "8"))

    best = None
    best_score = -1.0
    best_eval = None

    for i in range(1, max_rounds + 1):
        final_text, eval_json = round_trip(i)
        scores = eval_json.get("scores", {})
        total = float(scores.get("novelty", 0)) + float(scores.get("fit", 0)) + float(scores.get("feasibility", 0)) + float(scores.get("impact", 0))

        if total > best_score:
            best = final_text
            best_score = total
            best_eval = eval_json

        if bool(eval_json.get("pass")):
            break

        time.sleep(0.5)

    if not best:
        print("Failed to generate idea.", file=sys.stderr)
        sys.exit(1)

    with open(out_path, "w", encoding="utf-8") as f:
        f.write(best.strip() + "\n")

    # 只输出最终方案到 stdout（方便 Actions 日志看到），不输出中间过程
    print(best.strip())

if __name__ == "__main__":
    main()