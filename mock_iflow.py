#!/usr/bin/env python3
from http.server import BaseHTTPRequestHandler, HTTPServer
import json


def _redact_secret(value: str) -> str:
    if not value:
        return "[redacted]"
    if len(value) <= 8:
        return "*" * len(value)
    return f"{value[:4]}***{value[-4:]}"

def _redact_authorization(header: str) -> str:
    if not header:
        return "[missing]"
    if header.lower().startswith("bearer "):
        token = header[7:]
        return f"Bearer {_redact_secret(token)}"
    return _redact_secret(header)

class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        if self.path != "/chat/completions":
            self.send_response(404)
            self.end_headers()
            return
        auth = self.headers.get('Authorization')
        print(f"AUTH_HEADER={_redact_authorization(auth)}")
        length = int(self.headers.get('Content-Length','0'))
        _ = self.rfile.read(length)
        self.send_response(200)
        self.send_header('Content-Type','application/json')
        self.end_headers()
        resp = {
            "choices": [
                {
                    "index": 0,
                    "message": {"role": "assistant", "content": "mock reply"},
                    "finish_reason": "stop"
                }
            ]
        }
        self.wfile.write(json.dumps(resp).encode('utf-8'))

if __name__ == '__main__':
    server = HTTPServer(("127.0.0.1", 18080), Handler)
    print("Mock iFlow server on 127.0.0.1:18080")
    server.serve_forever()
