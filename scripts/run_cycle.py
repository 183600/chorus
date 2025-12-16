import os, json, subprocess, shutil, time
from datetime import datetime
from pathlib import Path
from typing import List, Dict, Any, Optional

# OpenAI SDK (兼容 NVIDIA Integrate OpenAI-style API)
from openai import OpenAI

REPO_ROOT = Path(__file__).resolve().parents[1]
HISTORY_FILE = REPO_ROOT / "history.json"
IMPLEMENTATION_PROMPT_FILE = REPO_ROOT / "implementation_prompt.md"

NVAPI_KEY = os.getenv("NVAPI_KEY")
OPENAI_BASE_URL = os.getenv("OPENAI_BASE_URL", "https://integrate.api.nvidia.com/v1")
LLM_MODEL = os.getenv("LLM_MODEL", "moonshotai/kimi-k2-thinking")
MAX_IDEA_ROUNDS = int(os.getenv("MAX_IDEA_ROUNDS", "4"))
MAX_CHAIN_DEPTH = int(os.getenv("MAX_CHAIN_DEPTH", "2"))

CORE_QUESTION = "发明一个目前没有被发明的workflow，大幅提升llm智力，可以落地"

FINAL_OUTPUT_FORMAT_INSTRUCTION = """当且仅当你内部确认已达到“极好标准”时，按以下格式输出一个方案（必须只输出一次）：

- **最关键刺激词**：[...]
- **词汇特性提取**：[...]
- **创意映射（逻辑同构）**：[...]
- **最终狂野点子（一个具体方案）**：[...]
- **落地第一步（48小时内可做的最小实验）**：[...]
"""

SYSTEM_SAFETY = "严禁输出中间过程、清单、评分或推理过程；只允许在最终一步按格式输出一个方案。"

client = OpenAI(api_key=NVAPI_KEY, base_url=OPENAI_BASE_URL)

def chat(messages: List[Dict[str, str]], temperature=0.9, max_tokens=1600) -> str:
    resp = client.chat.completions.create(
        model=LLM_MODEL,
        messages=messages,
        temperature=temperature,
        max_tokens=max_tokens
    )
    return resp.choices[0].message.content.strip()

def generate_high_entropy_words() -> Dict[str, List[str]]:
    prompt = f"""请生成高熵词库（严格满足配比）：
1) 3个具体名词（独特物理结构）
2) 3个抽象概念（哲学/科学术语）
3) 2个特定动作（强动态动词）
4) 2个跨界术语（生物/建筑/军事等）
仅以 JSON 返回：
{{"nouns": [...], "abstracts": [...], "actions": [...], "jargon": [...]}}
不得包含任何解释。"""
    out = chat([
        {"role": "system", "content": SYSTEM_SAFETY},
        {"role": "user", "content": prompt},
    ], temperature=0.8, max_tokens=600)
    # 容错解析
    try:
        # 找出第一个 JSON 块
        start = out.find("{")
        end = out.rfind("}")
        data = json.loads(out[start:end+1])
        return data
    except Exception as e:
        raise RuntimeError(f"无法解析高熵词库 JSON: {out}") from e

def generate_candidates(seeds: Dict[str, List[str]]) -> List[Dict[str, Any]]:
    words = seeds.get("nouns", []) + seeds.get("abstracts", []) + seeds.get("actions", []) + seeds.get("jargon", [])
    prompt = f"""核心问题：{CORE_QUESTION}

你将使用下面的10个刺激词逐一生成候选方案（每个词1个候选）：
{json.dumps(words, ensure_ascii=False)}

要求（内部执行，不得透露过程）：
- 对每个词：词汇特性提取 → 逻辑同构映射 → 具体方案（1-2句概要）
只以 JSON 数组返回，每个元素：
{{"word": "...", "candidate": "..."}}
不得输出任何多余文字。
"""
    out = chat([
        {"role": "system", "content": SYSTEM_SAFETY},
        {"role": "user", "content": prompt},
    ], temperature=0.9, max_tokens=1400)
    try:
        start = out.find("[")
        end = out.rfind("]")
        data = json.loads(out[start:end+1])
        if not isinstance(data, list) or len(data) != 10:
            raise ValueError("必须返回10个候选")
        return data
    except Exception as e:
        raise RuntimeError(f"无法解析候选 JSON: {out}") from e

def select_and_finalize(candidates: List[Dict[str, Any]]) -> str:
    prompt = f"""下面是10个候选概要（内部已评估）：
{json.dumps(candidates, ensure_ascii=False)}

现在只输出“最终最优方案”，且必须严格按如下格式输出（且只输出一次；不得泄露任何中间过程）：
{FINAL_OUTPUT_FORMAT_INSTRUCTION}
"""
    out = chat([
        {"role": "system", "content": SYSTEM_SAFETY},
        {"role": "user", "content": prompt},
    ], temperature=0.8, max_tokens=1000)
    return out

def is_extremely_good(final_text: str) -> bool:
    judge = f"""请仅以 true/false 返回：
该方案是否同时满足以下四项：新颖性、贴合度、可落地、杀伤力。
方案如下：
{final_text}"""
    out = chat([
        {"role": "system", "content": "严格只返回 true 或 false"},
        {"role": "user", "content": judge},
    ], temperature=0.0, max_tokens=4)
    return out.strip().lower().startswith("t")

def ideation_loop() -> str:
    # 多轮：直到“极好”或达到最大轮数
    for _ in range(MAX_IDEA_ROUNDS):
        seeds = generate_high_entropy_words()
        candidates = generate_candidates(seeds)
        final_text = select_and_finalize(candidates)
        if is_extremely_good(final_text):
            return final_text
    # 兜底：返回最后一轮结果
    return final_text

def ensure_history_file():
    if not HISTORY_FILE.exists():
        HISTORY_FILE.write_text("[]", encoding="utf-8")

def load_history() -> list:
    ensure_history_file()
    try:
        return json.loads(HISTORY_FILE.read_text(encoding="utf-8"))
    except Exception:
        return []

def save_history(data: list):
    HISTORY_FILE.write_text(json.dumps(data, ensure_ascii=False, indent=2), encoding="utf-8")

def compare_better(prev_text: str, new_text: str) -> bool:
    prompt = f"""你是评审，判断两个方案谁更优（只许返回 A 或 B）：
评判：新颖性、与核心问题贴合度、可落地性、潜在杀伤力。
核心问题：{CORE_QUESTION}

A:
{prev_text}

B:
{new_text}

只返回 A 或 B。"""
    out = chat([
        {"role": "system", "content": "只返回 A 或 B"},
        {"role": "user", "content": prompt},
    ], temperature=0.0, max_tokens=2)
    return out.strip() == "B"

def branch_name_now() -> str:
    ts = datetime.utcnow().strftime("%Y%m%d-%H%M%S")
    return f"idea-{ts}-utc"

def run_cmd(cmd: str, check=True, capture=False, env=None):
    print(f"$ {cmd}")
    if capture:
        return subprocess.check_output(cmd, shell=True, text=True, env=env)
    else:
        subprocess.run(cmd, shell=True, check=check, env=env)

def clean_repo_for_branch(keep_paths: List[str]):
    # 删除除 keep_paths 以外的所有文件
    for p in REPO_ROOT.iterdir():
        rel = str(p.relative_to(REPO_ROOT))
        if rel in keep_paths or rel == ".git":
            continue
        if rel.startswith(".git"):
            continue
        if p.is_dir():
            shutil.rmtree(p)
        else:
            try:
                p.unlink()
            except FileNotFoundError:
                pass

def create_implementation_prompt(final_text: str) -> str:
    # 把“最终狂野点子”转为 codex 的实现指令
    prompt = f"""你是资深工程代理。请在当前仓库实现以下方案（如需可重构项目结构）：
【方案】（按原格式）：
{final_text}

实现要求：
- 以可运行为目标，补足所有必要文件（README、脚本、入口等）
- 保持仓库内不需要的旧代码已清空（当前分支已清理）
- 提供最小可运行 Demo 或 workflow
- 若该方案本身是一个 workflow，请直接在 .github/workflows/ 内生成相应 yml 和脚本
- 提供快速验证命令（在 README 顶部）
- 所有生成文件直接写入仓库根（或合理的子目录）
开始自动实现，现在你可以修改/创建文件。
"""
    IMPLEMENTATION_PROMPT_FILE.write_text(prompt, encoding="utf-8")
    return prompt

def run_codex_full_auto(prompt: str) -> str:
    # 将 prompt 直接作为参数传给 codex
    # 这里假设 codex 会在当前目录落地文件
    try:
        cmd = f'codex --full-auto "{prompt.replace(\'"\', r\'\\\"\')}" --dangerously-bypass-approvals-and-sandbox'
        out = run_cmd(cmd, capture=True)
        return out or ""
    except subprocess.CalledProcessError as e:
        return f"[codex failed] {e}"

def main():
    # 避免无限自触发：根据历史最新条目的 depth 进行裁剪（可移除此护栏）
    history = load_history()
    latest_depth = history[-1].get("chain_depth", 0) if history else 0
    if latest_depth >= MAX_CHAIN_DEPTH:
        print(f"Guard: chain depth {latest_depth} >= {MAX_CHAIN_DEPTH}, skipping to avoid infinite loop.")
        return

    # 1) 多轮调用拿“最优点子”
    final_text = ideation_loop()
    print("Final idea:\n", final_text)

    # 2) 建分支 + 清空代码 + codex 实做
    idea_branch = branch_name_now()
    run_cmd(f"git checkout -b {idea_branch}")
    clean_repo_for_branch(keep_paths=[".github"])
    impl_prompt = create_implementation_prompt(final_text)
    codex_output = run_codex_full_auto(impl_prompt)

    # 提交 idea 分支
    run_cmd("git add -A")
    run_cmd(f'git commit -m "feat({idea_branch}): implement idea via codex [bot]"')
    run_cmd(f"git push origin {idea_branch}")

    # 3) 记录到 main 的 history.json 底部
    run_cmd("git checkout main")
    run_cmd("git pull --rebase origin main")
    history = load_history()
    record = {
        "timestamp_utc": datetime.utcnow().isoformat(timespec="seconds"),
        "branch": idea_branch,
        "chain_depth": latest_depth + 1,
        "final_output": final_text,
        "codex_log_excerpt": (codex_output[:4000] if codex_output else "")
    }
    history.append(record)

    # 4) 与倒数第二条比较，如更好则删掉上一条
    if len(history) >= 2:
        prev = history[-2]
        prev_text = prev.get("final_output", "")
        if prev_text:
            better = compare_better(prev_text, final_text)
            if better:
                # 删除“上一条”
                removed = history.pop(-2)
                print(f"New idea judged better; removed previous entry for branch {removed.get('branch')}.")

    save_history(history)
    run_cmd("git add history.json")
    run_cmd(f'git commit -m "chore(history): append result for {idea_branch} [bot]"')
    run_cmd("git push origin main")

if __name__ == "__main__":
    main()