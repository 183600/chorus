#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import argparse
import datetime as dt
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


def run(cmd, *, env=None, cwd=None, check=True):
    """
    统一执行命令：
    - cmd: list[str] 推荐；也支持 str(会走 shell)
    """
    if isinstance(cmd, str):
        print(f"\n$ {cmd}")
        p = subprocess.run(cmd, shell=True, env=env, cwd=cwd)
    else:
        print("\n$ " + " ".join([shlex_quote(x) for x in cmd]))
        p = subprocess.run(cmd, env=env, cwd=cwd)
    if check and p.returncode != 0:
        raise RuntimeError(f"Command failed ({p.returncode}): {cmd}")
    return p.returncode


def shlex_quote(s: str) -> str:
    # 简单 quote，避免日志里看不清；不做完整 shell-escape（我们主要用 list 调用）
    if not s:
        return "''"
    if any(c in s for c in " \t\n\"'`$\\"):
        return "'" + s.replace("'", "'\"'\"'") + "'"
    return s


def git(*args, env=None, check=True):
    return run(["git", *args], env=env, check=check)


def ensure_results_file(results_path: Path):
    results_path.parent.mkdir(parents=True, exist_ok=True)
    if not results_path.exists():
        results_path.write_text("[]", encoding="utf-8")
        return
    # 如果存在但不是合法 JSON，也不要默默覆盖
    try:
        json.loads(results_path.read_text(encoding="utf-8"))
    except Exception as e:
        raise RuntimeError(f"{results_path} exists but is not valid JSON: {e}")


def must_exist(path: Path, hint: str):
    if not path.exists():
        raise RuntimeError(f"Missing required file: {path}\nHint: {hint}")


def current_branch():
    p = subprocess.check_output(["git", "rev-parse", "--abbrev-ref", "HEAD"]).decode().strip()
    return p


def is_worktree_clean():
    out = subprocess.check_output(["git", "status", "--porcelain"]).decode().strip()
    return out == ""


def main():
    ap = argparse.ArgumentParser(description="Run one local iteration of auto-idea-codex-loop (GitHub Actions -> local).")
    ap.add_argument("--main-branch", default="main", help="Main branch name (default: main)")
    ap.add_argument("--remote", default="origin", help="Git remote name (default: origin)")
    ap.add_argument("--push", action="store_true", help="Push idea branch + main branch to remote (default: off)")
    ap.add_argument("--allow-dirty", action="store_true", help="Allow running even if git working tree is dirty")
    ap.add_argument("--install-iflow", action="store_true", help="Run `npm i -g @iflow-ai/iflow-cli@latest`")
    ap.add_argument("--skip-smoke", action="store_true", help="Skip iFlow smoke test")
    args = ap.parse_args()

    repo_root = Path.cwd()

    # 需要的脚本存在（和 Actions 一样依赖仓库内脚本）
    must_exist(repo_root / "scripts" / "generate_idea.py", "Make sure scripts/generate_idea.py exists in this repo.")
    must_exist(repo_root / "scripts" / "update_results.py", "Make sure scripts/update_results.py exists in this repo.")
    must_exist(repo_root / "scripts" / "judge_and_prune.py", "Make sure scripts/judge_and_prune.py exists in this repo.")

    if not args.allow_dirty and not is_worktree_clean():
        raise RuntimeError("Git working tree is not clean. Commit/stash your changes or pass --allow-dirty.")

    # 对齐 Actions 里的 env 默认值（不覆盖用户自己 export 的）
    env = os.environ.copy()
    env.setdefault("IFLOW_BASE_URL", "https://apis.iflow.cn/v1")
    env.setdefault("IFLOW_MODEL_NAME", "glm-4.6")
    env.setdefault("IDEA_MAX_ROUNDS", "8")

    if not env.get("IFLOW_API_KEY"):
        raise RuntimeError("Missing IFLOW_API_KEY. Please `export IFLOW_API_KEY=...` before running.")

    # 生成分支名（UTC）
    branch_name = "idea-" + dt.datetime.utcnow().strftime("%Y%m%d-%H%M%S")
    print(f"\n[info] branch_name = {branch_name}")

    # 模拟 RUNNER_TEMP
    runner_temp = Path(tempfile.mkdtemp(prefix="auto-idea-runner-temp-"))
    idea_txt = runner_temp / "idea.txt"
    prompt_txt = runner_temp / "prompt.txt"
    branch_sha_txt = runner_temp / "branch_sha.txt"

    print(f"[info] runner_temp = {runner_temp}")

    # results/ideas.json
    results_json = repo_root / "results" / "ideas.json"
    ensure_results_file(results_json)

    # 保存当前分支，出错时尽量切回
    start_branch = current_branch()
    print(f"[info] start_branch = {start_branch}")

    try:
        # 1) Generate idea (multi-call) —— 按 Actions：python scripts/generate_idea.py "${RUNNER_TEMP}/idea.txt"
        run([sys.executable, "scripts/generate_idea.py", str(idea_txt)], env=env)

        if not idea_txt.exists():
            raise RuntimeError(f"generate_idea.py did not create {idea_txt}")

        # 2) Create new branch + git identity（和 Actions 一样）
        git("config", "user.name", "github-actions[bot]")
        git("config", "user.email", "github-actions[bot]@users.noreply.github.com")
        git("checkout", "-b", branch_name)

        # 3) Node / npm / iflow
        run(["node", "--version"], check=True)
        run(["npm", "--version"], check=True)

        if args.install_iflow:
            run(["npm", "i", "-g", "@iflow-ai/iflow-cli@latest"], check=True)

        # 确保 iflow 可用
        run(["iflow", "--version"], check=True)

        # 4) Sanity test iFlow can run（对应 smoke test step）
        if not args.skip_smoke:
            run(["iflow", "Create a file IFLOW_SMOKE_TEST.md with content 'ok'. think:high", "--yolo"], env=env)
            if not (repo_root / "IFLOW_SMOKE_TEST.md").exists():
                raise RuntimeError("Smoke test failed: IFLOW_SMOKE_TEST.md not found.")

        # 5) iFlow implement the idea（和 workflow 的 prompt 拼接逻辑一致）
        idea = idea_txt.read_text(encoding="utf-8")

        prompt_lines = [
            '你将把下面这个"提升 LLM 智力的全新 workflow 点子"落地为一个可运行的最小原型仓库。',
            "",
            "要求：",
            "1) 生成 README.md：讲清楚 workflow 的目标、步骤、输入输出、如何运行",
            "2) 给出一个最小可运行 demo（例如 scripts/ 或 src/），能在 CI 上执行",
            "3) 提供一个简单的自检（例如运行脚本输出关键结果）",
            "4) 不要泄露任何密钥，不要把 ~/.codex 写入仓库",
            "",
            "点子如下（原样粘贴）：",
            idea,
        ]
        prompt_txt.write_text("\n".join(prompt_lines), encoding="utf-8")

        prompt_content = prompt_txt.read_text(encoding="utf-8")
        run(["iflow", f"{prompt_content} think:high", "--yolo"], env=env)

        # git add/commit（commit 允许空：和 Actions 的 `|| true` 一样）
        git("add", "-A")
        git("commit", "-m", "feat: implement idea via iflow", check=False)

        # 记录 branch sha
        sha = subprocess.check_output(["git", "rev-parse", "HEAD"]).decode().strip()
        branch_sha_txt.write_text(sha, encoding="utf-8")
        print(f"[info] branch sha = {sha}")

        # 6) Push idea branch（本地默认不 push；需要就 --push）
        if args.push:
            # 如果你想用 GH_PAT 强制 HTTPS，可启用下面逻辑（可选）
            # 注意：这会临时改 remote URL；你也可以直接用 SSH remote，不需要 GH_PAT。
            gh_pat = env.get("GH_PAT")
            if gh_pat:
                # 尝试从当前 remote 推断 repo（尽力而为）
                remote_url = subprocess.check_output(["git", "remote", "get-url", args.remote]).decode().strip()
                # 如果 remote 不是 github.com 或格式不标准，这里就不改
                if "github.com" in remote_url and remote_url.startswith("https://"):
                    # https://github.com/OWNER/REPO.git -> https://x-access-token:TOKEN@github.com/OWNER/REPO.git
                    token_url = remote_url.replace("https://", f"https://x-access-token:{gh_pat}@")
                    git("remote", "set-url", args.remote, token_url)

            git("push", "-u", args.remote, branch_name)

        # 7) Switch back to main and restore original files（checkout main + pull）
        git("checkout", args.main_branch)
        if args.push:
            # 只在 push 模式下 pull，避免离线或无 remote 的情况下卡住
            git("pull", args.remote, args.main_branch)

        # 8) Append result to results/ideas.json on main
        run([sys.executable, "scripts/update_results.py",
             str(results_json), branch_name, str(idea_txt), sha], env=env)

        # 9) Compare with previous and prune if better
        run([sys.executable, "scripts/judge_and_prune.py", str(results_json)], env=env)

        # 10) Commit and push main（同 Actions：commit 允许空）
        git("add", str(results_json))
        git("commit", "-m", f"chore: record idea result ({branch_name})", check=False)

        if args.push:
            git("push", args.remote, args.main_branch)

        print("\n[done] one iteration completed successfully.")

    finally:
        # 清理 temp
        try:
            shutil.rmtree(runner_temp, ignore_errors=True)
        except Exception:
            pass
        # 尽量切回开始分支（如果你希望始终停在 main，可删除这段）
        try:
            if current_branch() != start_branch:
                git("checkout", start_branch, check=False)
        except Exception:
            pass


if __name__ == "__main__":
    main()