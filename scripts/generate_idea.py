#!/usr/bin/env python3
import json
import os
import sys
import time
import re  # 新增导入
from typing import Any, Dict, Tuple

import requests

BASE_URL = os.getenv("IFLOW_BASE_URL", "https://apis.iflow.cn/v1").rstrip("/")
MODEL = os.getenv("IFLOW_MODEL_NAME", "glm-4.6")
API_KEY = os.getenv("IFLOW_API_KEY") or os.getenv("OPENAI_API_KEY")

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
    """
    从不同供应商/协议的返回体中抽取“助手输出文本”。
    兼容：
    - iflow: output_text / output[].content[].text
    - OpenAI Chat Completions: choices[0].message.content
    - 可能的增量格式：choices[0].delta.content
    """
    if not isinstance(resp, dict):
        return json.dumps(resp, ensure_ascii=False)

    # 1) iflow: output_text
    if "output_text" in resp and isinstance(resp["output_text"], str):
        return resp["output_text"].strip()

    # 2) iflow: output chunks
    out = resp.get("output")
    if isinstance(out, list):
        chunks: list[str] = []
        for item in out:
            if not isinstance(item, dict):
                continue
            content_list = item.get("content", [])
            if not isinstance(content_list, list):
                continue
            for c in content_list:
                if isinstance(c, dict) and isinstance(c.get("text"), str):
                    chunks.append(c["text"])
        if chunks:
            return "".join(chunks).strip()

    # 3) OpenAI chat.completions: choices[0].message.content
    choices = resp.get("choices")
    if isinstance(choices, list) and choices:
        choice0 = choices[0] if isinstance(choices[0], dict) else None
        if isinstance(choice0, dict):
            msg = choice0.get("message")
            if not isinstance(msg, dict):
                # 一些实现用 delta
                msg = choice0.get("delta")

            if isinstance(msg, dict):
                content = msg.get("content")

                # content 是字符串（最常见）
                if isinstance(content, str):
                    return content.strip()

                # content 是分段结构（少数实现）
                if isinstance(content, list):
                    parts: list[str] = []
                    for p in content:
                        if not isinstance(p, dict):
                            continue
                        # 常见两种字段
                        if isinstance(p.get("text"), str):
                            parts.append(p["text"])
                        elif p.get("type") == "text" and isinstance(p.get("text"), str):
                            parts.append(p["text"])
                    if parts:
                        return "".join(parts).strip()

    # 4) fallback：返回原始 JSON 便于调试
    return json.dumps(resp, ensure_ascii=False)

def call_responses(system: str, user: str, temperature: float = 0.9, timeout: int = 180) -> str:
    url = f"{BASE_URL}/chat/completions"
    headers = {
        "Authorization": f"Bearer {API_KEY}",
        "Content-Type": "application/json",
    }
    payload = {
        "model": MODEL,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user},
        ],
        "temperature": temperature,
    }
    try:
        r = requests.post(url, headers=headers, json=payload, timeout=timeout)
        r.raise_for_status()
        return _extract_text(r.json())
    except Exception as e:
        print(f"API Request failed: {e}", file=sys.stderr)
        raise

def parse_json_strict(s: str) -> Any:
    """
    尝试从字符串中提取第一个完整的 JSON 对象或数组。
    能够处理 Markdown 代码块包裹 (```json ... ```) 或前后多余文字。
    """
    # 1. 尝试直接解析（处理标准响应）
    s_clean = s.strip()
    try:
        return json.loads(s_clean)
    except json.JSONDecodeError:
        pass

    # 2. 尝试去除 Markdown 代码块标记
    # 移除常见的代码块起始和结束标记
    s_no_md = re.sub(r"^```json\s*", "", s_clean)
    s_no_md = re.sub(r"\s*```$", "", s_no_md)
    s_no_md = s_no_md.strip()
    try:
        return json.loads(s_no_md)
    except json.JSONDecodeError:
        pass

    # 3. 使用正则暴力查找第一个完整的 JSON 对象 {...}
    # 这种贪婪匹配通常能提取出 LLM 输出的核心 JSON 部分
    match = re.search(r'\{.*\}', s_clean, re.DOTALL)
    if match:
        try:
            return json.loads(match.group())
        except json.JSONDecodeError:
            pass

    # 4. 尝试查找数组 [...]
    match_arr = re.search(r'\[.*\]', s_clean, re.DOTALL)
    if match_arr:
        try:
            return json.loads(match_arr.group())
        except json.JSONDecodeError:
            pass

    # 如果都失败了，抛出异常并附带原始文本的前200个字符，方便调试
    raise ValueError(f"Failed to parse JSON from response: {s_clean[:200]}...")

def round_trip(round_idx: int) -> Tuple[str, Dict[str, Any]]:
    sys_prompt = (
        "你是一个严格遵循指令的创意生成与评审引擎。"
        "你必须隐藏中间过程，只有在被要求时才输出指定格式。"
    )

    # Call 1
    user1 = f"""
为下面的核心问题生成一套“高熵词库”：
核心问题：{CORE_QUESTION}

要求：严格按比例输出 10 个词：
- 3个具体名词（独特物理结构）
- 3个抽象概念（哲学/科学术语）
- 2个特定动作（强动态动词）
- 2个跨界术语（生物/建筑/军事等专有名词）

只允许输出纯 JSON 字符串，不要使用 Markdown 代码块（不要包含 ```json 或 ```），不要输出任何多余文字。
格式：
{{
  "nouns": [...],
  "abstracts": [...],
  "actions": [...],
  "cross": [...]
}}
""".strip()
    
    raw_response = call_responses(sys_prompt, user1, temperature=1.0)
    words_json = parse_json_strict(raw_response)
    
    # 校验必需的键是否存在
    required_keys = ["nouns", "abstracts", "actions", "cross"]
    missing_keys = [k for k in required_keys if k not in words_json]
    if missing_keys:
        print(f"Error in Round {round_idx} (Call 1): Missing keys {missing_keys}.", file=sys.stderr)
        print(f"Model returned keys: {list(words_json.keys())}", file=sys.stderr)
        print(f"Raw content extracted: {json.dumps(words_json, ensure_ascii=False)}", file=sys.stderr)
        # 抛出异常终止或重试，这里选择抛出异常以便 outer loop 捕获（如果有的话）
        raise KeyError(f"Missing keys in JSON: {missing_keys}")

    nouns = words_json["nouns"]
    abstracts = words_json["abstracts"]
    actions = words_json["actions"]
    cross = words_json["cross"]
    words = nouns + abstracts + actions + cross

    # Call 2
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

只允许输出纯 JSON 字符串，不要使用 Markdown 代码块，不要输出任何多余文字。
格式：
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
""".strip()
    cand_json = parse_json_strict(call_responses(sys_prompt, user2, temperature=0.95))

    # Call 3
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

    # Call 4
    user4 = f"""
请你对“最终输出方案”做严格自评，判断是否同时满足四项：
新颖性、贴合度、可落地、杀伤力。

最终输出方案如下：
{final_text}

只允许输出纯 JSON 字符串，不要使用 Markdown 代码块，不要输出任何多余文字。
格式：
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
        try:
            final_text, eval_json = round_trip(i)
            scores = eval_json.get("scores", {})
            # 安全地获取分数，防止缺少某个分数项报错
            total = float(scores.get("novelty", 0)) + float(scores.get("fit", 0)) + \
                    float(scores.get("feasibility", 0)) + float(scores.get("impact", 0))

            if total > best_score:
                best = final_text
                best_score = total
                best_eval = eval_json

            if bool(eval_json.get("pass")):
                break

            time.sleep(0.5)
        except (KeyError, ValueError, requests.RequestException) as e:
            print(f"Round {i} failed: {e}", file=sys.stderr)
            # 可以选择继续尝试下一轮或者直接退出
            # continue 
            # 为了稳定性，如果解析彻底失败，建议稍作等待后重试或退出
            time.sleep(1)
            continue

    if not best:
        print("Failed to generate idea after retries.", file=sys.stderr)
        sys.exit(1)

    with open(out_path, "w", encoding="utf-8") as f:
        f.write(best.strip() + "\n")

    print(best.strip())

if __name__ == "__main__":
    main()