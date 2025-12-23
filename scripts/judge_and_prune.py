#!/usr/bin/env python3
import json
import os
import sys
from typing import Any, Dict
import requests

BASE_URL = os.getenv("IFLOW_BASE_URL", "https://apis.iflow.cn/v1").rstrip("/")
MODEL = os.getenv("IFLOW_MODEL", "glm-4.6")
API_KEY = os.getenv("IFLOW_API_KEY") or os.getenv("OPENAI_API_KEY")

if not API_KEY:
    print("Missing IFLOW_API_KEY (or OPENAI_API_KEY).", file=sys.stderr)
    sys.exit(2)

def call_responses(system: str, user: str, temperature: float = 0.2, timeout: int = 180) -> Dict[str, Any]:
    url = f"{BASE_URL}/responses"
    headers = {"Authorization": f"Bearer {API_KEY}", "Content-Type": "application/json"}
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
    return r.json()

def extract_text(resp: Dict[str, Any]) -> str:
    if "output_text" in resp and isinstance(resp["output_text"], str):
        return resp["output_text"].strip()
    out = resp.get("output")
    if isinstance(out, list):
        chunks = []
        for item in out:
            if isinstance(item, dict):
                for c in item.get("content", []):
                    if isinstance(c, dict) and "text" in c:
                        chunks.append(c["text"])
        if chunks:
            return "".join(chunks).strip()
    return json.dumps(resp, ensure_ascii=False)

def parse_json_strict(s: str) -> Any:
    s2 = s.strip()
    start = s2.find("{")
    if start == -1:
        raise ValueError("No JSON object found")
    return json.loads(s2[start:])

def main():
    if len(sys.argv) < 2:
        print("Usage: judge_and_prune.py <json_path>", file=sys.stderr)
        sys.exit(2)

    json_path = sys.argv[1]
    with open(json_path, "r", encoding="utf-8") as f:
        data = json.load(f)

    if not isinstance(data, list) or len(data) < 2:
        sys.exit(0)

    prev = data[-2]
    last = data[-1]

    sys_prompt = "你是严格的方案评审裁判，只能按要求输出 JSON。"
    user = f"""
请比较两个方案，判断哪一个“整体更好”（四项同权重）：
- 新颖性
- 贴合度
- 可落地
- 杀伤力

方案A（prev）：
{prev.get("idea","")}

方案B（last）：
{last.get("idea","")}

只允许输出 JSON：
{{
  "winner": "prev" 或 "last",
  "scores": {{
    "prev": {{"novelty":0-10,"fit":0-10,"feasibility":0-10,"impact":0-10}},
    "last": {{"novelty":0-10,"fit":0-10,"feasibility":0-10,"impact":0-10}}
  }},
  "why": "一句话原因"
}}
不要输出任何多余文字。
""".strip()

    resp = call_responses(sys_prompt, user)
    text = extract_text(resp)
    judge = parse_json_strict(text)

    # 记录裁判结果到 last（可选，但很实用）
    last["judge"] = judge

    if judge.get("winner") == "last":
        # 删除上一个（倒数第二个）
        del data[-2]

    with open(json_path, "w", encoding="utf-8") as f:
        json.dump(data, f, ensure_ascii=False, indent=2)

if __name__ == "__main__":
    main()