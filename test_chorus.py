import os, subprocess, time, socket, json, sys, re
from pathlib import Path
from getpass import getpass
import requests  # 提前导入，后面要用

PORT = 11435
REPO_DIR = Path("/home/engine/project")
BIN_PATH = REPO_DIR / "target" / "release" / "chorus"
LOG_PATH = REPO_DIR / "server.log"
CONFIG_PATH = Path.home() / ".config" / "chorus" / "config.toml"

def run(cmd, cwd=None, env=None):
    print(f">> {cmd}")
    subprocess.run(cmd, shell=True, check=True, cwd=cwd, env=env)

def is_listening(host="127.0.0.1", port=PORT, timeout=1.5):
    try:
        with socket.create_connection((host, port), timeout=timeout):
            return True
    except Exception:
        return False

def wait_port(host="127.0.0.1", port=PORT, timeout=120):
    t0 = time.time()
    while time.time() - t0 < timeout:
        if is_listening(host, port):
            return True
        time.sleep(1)
    return False

# 0) 如果已经在跑，直接提示成功
if is_listening():
    print(f"Chorus 已在端口 {PORT} 运行 -> http://127.0.0.1:{PORT}")
else:
    # 1) 准备二进制：若不存在，则构建
    if not BIN_PATH.exists():
        print("未找到编译好的 Chorus，开始构建...")
        run("cargo build --release -j 2", cwd=str(REPO_DIR))
    else:
        print("检测到已构建的 Chorus 二进制，跳过编译。")

    # 2) 准备最小可用配置（如不存在则创建）
    if not CONFIG_PATH.exists():
        print("未检测到配置文件，创建最小可用配置（仅用 glm-4.6，温度=0，便于稳定输出 JSON）")
        CONFIG_PATH.parent.mkdir(parents=True, exist_ok=True)
        iflow_key = (os.getenv("IFLOW_API_KEY") or getpass("请输入 iFlow API Key (apis.iflow.cn)：")).strip()
        cfg = f"""
[server]
host = "127.0.0.1"
port = {PORT}

[[model]]
name = "glm-4.6"
api_base = "https://apis.iflow.cn/v1"
api_key = "{iflow_key}"

[workflow-integration]
nested_worker_depth = 1
json = \"\"\"{{
  "analyzer": {{"ref": "glm-4.6"}},
  "workers": [{{"name": "glm-4.6", "temperature": 0}}],
  "synthesizer": {{"ref": "glm-4.6"}}
}}\"\"\"

[workflow.timeouts]
analyzer_timeout_secs = 120
worker_timeout_secs = 180
synthesizer_timeout_secs = 120
"""
        CONFIG_PATH.write_text(cfg, encoding="utf-8")
        print(f"配置写入：{CONFIG_PATH}")
    else:
        print(f"使用已有配置：{CONFIG_PATH}")

    # 3) 启动服务（后台），写日志
    print("启动 Chorus 服务...")
    LOG_PATH.parent.mkdir(parents=True, exist_ok=True)
    server_out = open(LOG_PATH, "wb")
    ENV2 = os.environ.copy()
    ENV2["RUST_LOG"] = "info"
    proc = subprocess.Popen([str(BIN_PATH)], cwd=str(REPO_DIR), env=ENV2,
                            stdout=server_out, stderr=subprocess.STDOUT)

    # 4) 等待端口就绪
    if not wait_port(timeout=180):
        server_out.close()
        print("\n启动失败：端口未就绪。最近 200 行日志如下：\n")
        try:
            print(subprocess.check_output(f"tail -n 200 {LOG_PATH}", shell=True, text=True))
        except Exception as e:
            print(f"(无法读取日志: {e})")
        raise SystemExit(1)

    print(f"Chorus 已就绪 -> http://127.0.0.1:{PORT}")

# 4.5) 使用 requests 调用一次生成接口（替代原先的 curl，避免引号冲突）
try:
    prompt = """我是一名中国的16岁学习成绩差（语文、数学、英语以及物理都很差）的男开发者，在15岁时开发过一个调用多次llm，选一个最好的结果的工具，在15岁时尝试开发过一个能像大模型那样生成自然语言的内容的符号ai（名字是knowprolog），失败了，在15岁时开发了一个把typus（我和大模型一起创造的一个编程语言）编译到go代码的编译器，目前我不写代码，这两个的代码基本都是ai生成的。学过Rust和haskell（15岁时），没学完就放弃了。曾经开发过Chrome扩展（但是并不是特别会前端）（14岁左右时），也用AndroLua（最开始用的FusionApp）（同时搭配其他一些工具，如MT管理器）开发过安卓应用（9岁时用FusionApp开发过工具箱，写过lua代码）（10岁开始时用AndroLua开发过工具箱、相机和浏览器，写过lua代码），还用c语言开发过窗口管理器（13岁时）（好像过了3个月左右就放弃开发了）（但是并不是特别会c语言），走过一段时间oi路（12岁到13岁），后来放弃那条路了。这个人了解一些历史、地理、荣格八维（12岁开始接触）、mbti（12岁开始接触）、分子人类学（12岁开始大量接触）、linux（12岁开始大量接触）的知识，不过就一些，不是很多，同时知道一些手机的参数"""

    payload = {
        "model": "chorus",
        "prompt": prompt,
        "stream": False,
    }

    resp = requests.post(f"http://127.0.0.1:{PORT}/api/generate", json=payload, timeout=60)
    print("生成接口状态码:", resp.status_code)
    print("生成接口响应体前 1000 字符:\n", resp.text[:1000])
except Exception as e:
    print("调用 /api/generate 失败：", e)

# 5) 快速健康检查
try:
    r = requests.get(f"http://127.0.0.1:{PORT}/v1/models", timeout=8)
    if r.ok:
        print("健康检查 OK：/v1/models 可访问。")
    else:
        print("健康检查失败：/v1/models 返回非 200。状态码:", r.status_code, r.text[:300])
except Exception as e:
    print("健康检查异常：", e)
    print("\n可执行以下命令查看日志：")
    print(f"!tail -n 200 {LOG_PATH}")