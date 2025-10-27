# 在 Cherry Studio 中使用 Chorus

## 修复说明

已修复 Cherry Studio 无法显示回答的问题。问题原因是缺少流式响应（SSE）支持。

### 修改内容

1. **添加了流式响应支持**：当 `stream: true` 时，使用 Server-Sent Events (SSE) 格式返回响应
2. **保持向后兼容**：当 `stream: false` 或未设置时，返回完整的 JSON 响应
3. **支持的端点**：
   - `/api/chat` - 聊天接口
   - `/api/generate` - 生成接口

## Cherry Studio 配置

### 添加自定义模型

1. 打开 Cherry Studio
2. 进入设置 -> 模型配置
3. 添加新的提供商：
   - **API 类型**: Ollama
   - **API Base URL**: `http://localhost:11435`
   - **模型名称**: `chorus`

### 使用方法

配置完成后，在对话中选择 `chorus` 模型即可使用。

## 响应格式

### 流式响应 (stream: true)

```
data: {"model":"chorus","created_at":"...","message":{"role":"assistant","content":"响应内容"},"done":false}

data: {"model":"chorus","created_at":"...","message":{"role":"assistant","content":""},"done":true}
```

### 非流式响应 (stream: false)

```json
{
  "model": "chorus",
  "created_at": "2024-01-15T10:30:00Z",
  "message": {
    "role": "assistant",
    "content": "响应内容"
  },
  "done": true
}
```

## 测试

运行测试脚本验证功能：

```bash
./test_streaming.sh
```

或手动测试：

```bash
# 测试流式响应
curl -X POST http://localhost:11435/api/chat \
  -H "Content-Type: application/json" \
  -d '{
    "model": "chorus",
    "messages": [{"role": "user", "content": "你好"}],
    "stream": true
  }'

# 测试非流式响应
curl -X POST http://localhost:11435/api/chat \
  -H "Content-Type: application/json" \
  -d '{
    "model": "chorus",
    "messages": [{"role": "user", "content": "你好"}],
    "stream": false
  }'
```

## 注意事项

- Chorus 的完整工作流需要调用多个 LLM API，响应时间较长（通常需要几十秒到几分钟）
- 确保 `~/.config/chorus/config.toml` 中配置了有效的 API Key
- Cherry Studio 可能默认发送 `stream: true`，现在已完全支持
