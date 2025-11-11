#!/usr/bin/env python3
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import re
from typing import Optional


def redact_auth(header: Optional[str]) -> str:
    if not header:
        return "<missing>"
    if header.startswith("Bearer "):
        token = header[7:]
        if len(token) <= 8:
            return "Bearer ****"
        return f"Bearer {token[:4]}...{token[-4:]}"
    return "<redacted>"


def build_reply(model: str, prompt: str) -> str:
    if "请分析以下用户提示" in prompt:
        payload = {
            "temperature": 0.8,
            "reasoning": "mock auto temperature",
        }
        return json.dumps(payload, ensure_ascii=False)

    if "选出" in prompt or "selected_index" in prompt:
        match = re.search(r"【回答(\d+)：([^】]+)】", prompt)
        if match:
            index = int(match.group(1))
            worker = match.group(2)
        else:
            index = 1
            worker = "模型1"

        payload = {
            "selected_index": index,
            "selected_worker": worker,
            "selected_response": f"mock reply from {worker}",
            "reasoning": "mock selector reasoning",
        }
        return json.dumps(payload, ensure_ascii=False)

    return f"{model} mock reply"


class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        if self.path != "/chat/completions":
            self.send_response(404)
            self.end_headers()
            return

        auth = self.headers.get("Authorization")
        print(f"AUTH_HEADER={redact_auth(auth)}")

        length = int(self.headers.get("Content-Length", "0") or 0)
        body = self.rfile.read(length)

        try:
            payload = json.loads(body)
        except json.JSONDecodeError:
            payload = {}

        messages = payload.get("messages") or []
        if messages and isinstance(messages[-1], dict):
            last_prompt = str(messages[-1].get("content", ""))
        else:
            last_prompt = ""
        model = payload.get("model", "mock-model")

        reply_content = build_reply(model, last_prompt)

        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()

        resp = {
            "choices": [
                {
                    "index": 0,
                    "message": {"role": "assistant", "content": reply_content},
                    "finish_reason": "stop",
                }
            ]
        }
        self.wfile.write(json.dumps(resp, ensure_ascii=False).encode("utf-8"))

    def log_message(self, format, *args):
        # Suppress default logging to keep output clean
        return


if __name__ == "__main__":
    server = HTTPServer(("127.0.0.1", 18080), Handler)
    print("Mock iFlow server on 127.0.0.1:18080")
    server.serve_forever()
