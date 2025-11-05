# Chorus

<div align="center">

**一个智能的 LLM API 聚合服务，通过多模型协同提供更高质量的 AI 响应**

[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![API](https://img.shields.io/badge/API-Ollama--compatible-green.svg)](https://github.com/ollama/ollama)

</div>

## 📖 简介

Chorus 是一个用 Rust 编写的高性能 LLM API 服务，提供与 Ollama 兼容的 API 接口。它的独特之处在于采用了**三步智能工作流**：

1. **智能分析** - 由 GLM-4.6 分析用户问题并确定最佳 temperature 参数
2. **多模型协同** - 按序调用 7 个不同的 LLM 模型生成响应
3. **智能综合** - 将所有响应综合成一个高质量的最终答案

这种方法结合了多个模型的优势，提供更准确、更全面的回答。

## ✨ 特性

- 🚀 **高性能** - 基于 Rust 和 Tokio 异步运行时
- 🔄 **智能工作流** - 三步式处理流程，确保输出质量
- 🎯 **自适应参数** - 自动分析并设置最优 temperature
- 🤝 **多模型协同** - 支持 7 个不同的 LLM 模型
- 🔌 **兼容 Ollama** - API 接口与 Ollama 兼容
- ⚙️ **灵活配置** - TOML 格式配置文件
- 📝 **详细日志** - 完整的请求追踪和错误日志
- 🛡️ **错误处理** - 健壮的错误处理机制

## 🏗️ 架构

```
┌─────────────┐
│   Client    │
└──────┬──────┘
       │ HTTP Request
       ▼
┌─────────────────────────────────────┐
│         Chorus Server               │
│  ┌───────────────────────────────┐  │
│  │   Step 1: Prompt Analysis     │  │
│  │   Model: GLM-4.6              │  │
│  │   Output: Temperature         │  │
│  └───────────┬───────────────────┘  │
│              ▼                       │
│  ┌───────────────────────────────┐  │
│  │   Step 2: Multi-Model Work    │  │
│  │   Models:                     │  │
│  │   • qwen3-max                 │  │
│  │   • qwen3-vl-plus            │  │
│  │   • kimi-k2-0905             │  │
│  │   • glm-4.6                  │  │
│  │   • deepseek-v3.2            │  │
│  │   • deepseek-v3.1            │  │
│  │   • deepseek-r1              │  │
│  └───────────┬───────────────────┘  │
│              ▼                       │
│  ┌───────────────────────────────┐  │
│  │   Step 3: Response Synthesis  │  │
│  │   Model: GLM-4.6              │  │
│  │   Output: Final Answer        │  │
│  └───────────────────────────────┘  │
└─────────────────────────────────────┘
```

## 🚀 快速开始

### 前置要求

- Rust 1.75 或更高版本
- 有效的 iFlow API Key

### 安装

1. **克隆仓库**

```bash
git clone https://github.com/yourusername/chorus.git
cd chorus
```

2. **配置 API Key**

- **生产 / 真实调用**：编辑 `~/.config/chorus/config.toml` 文件，将所有 `your-api-key-here` 替换为真实的 iFlow API Key。
- **本地快速体验**：仓库提供了 `mock_iflow.py`（模拟 iFlow 服务）和 `test-config.toml` 示例配置，无需真实 API Key。使用方式如下：

  ```bash
  # 终端 1：启动本地模拟 LLM 服务（监听 127.0.0.1:18080）
  python3 mock_iflow.py

  # 终端 2：使用示例配置运行 Chorus
  CHORUS_CONFIG=./test-config.toml cargo run
  ```

示例配置文件 `test-config.toml` 内容如:

```toml
[[model]]
api_base = "http://127.0.0.1:18080"
api_key = "sk-TEST-IFLOW-KEY"
name = "glm-4.6"

[workflow-integration]
json = """{
  "analyzer": {
    "ref": "glm-4.6",
    "auto_temperature": true
  },
  "workers": [
    {
      "name": "glm-4.6"
    }
  ],
  "synthesizer": {
    "ref": "glm-4.6"
  }
}"""

[workflow.timeouts]
analyzer_timeout_secs = 30000
worker_timeout_secs = 60000
synthesizer_timeout_secs = 60000
```

3. **编译项目**

```bash
# 开发模式
cargo build

# 生产模式（优化编译）
cargo build --release
```

4. **运行服务**

```bash
# 开发模式
cargo run

# 生产模式
./target/release/chorus
```

服务将在 `http://localhost:11435` 启动。

## ⚙️ 配置说明

### 服务器配置

```toml
[server]
host = "127.0.0.1"  # 监听地址
port = 11435         # 监听端口
```

### 模型配置

为每个模型添加配置块：

```toml
[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "your-api-key"
name = "model-name"
# temperature = 1.4           # 可选：固定 temperature 值（0.0-2.0）
# auto_temperature = false    # 可选：是否由大模型自动选择 temperature
```

#### Temperature 配置说明

Chorus 支持三种 temperature 配置方式：

1. **固定 temperature 值**
   ```toml
   [[model]]
   name = "qwen3-max"
   temperature = 0.8  # 固定使用 0.8
   ```
   - 适用场景：明确知道该模型适合的 temperature 值
   - 取值范围：0.0 - 2.0
   - 效果：
     - `0.0-0.3`：非常确定和保守，适合精确答案
     - `0.4-0.7`：平衡输出，适合大多数场景
     - `0.8-1.2`：更有创造性，适合创意写作
     - `1.3-2.0`：非常随机和创造性

2. **自动 temperature 选择**
   ```toml
   [[model]]
   name = "glm-4.6"
   auto_temperature = true  # 由分析器自动决定
   ```
   - 适用场景：希望根据用户问题类型动态调整
   - 工作原理：分析器模型会根据问题特点推荐最佳 temperature

3. **使用默认值**
   ```toml
   [[model]]
   name = "deepseek-v3.2"
   # 不设置任何 temperature 参数
   ```
   - 默认行为：使用 1.4 作为 temperature

**优先级规则**：
- 如果设置了 `temperature`，则使用该固定值（忽略 `auto_temperature`）
- 如果只设置了 `auto_temperature = true`，则由分析器决定
- 如果两者都未设置，则使用默认值 1.4

**示例配置**：查看 `config-example.toml` 了解更多配置示例。

### 工作流配置（支持嵌套节点与域名超时覆盖）

```toml
[workflow-integration]
json = """{
  "analyzer": {
    "ref": "glm-4.6",
    "auto_temperature": true
  },
  "workers": [
    {
      "name": "qwen3-max",
      "temperature": 0.8
    },
    {
      "name": "deepseek-v3.2"
    },
    {
      "analyzer": {
        "ref": "glm-4.6",
        "auto_temperature": true
      },
      "workers": [
        {
          "name": "kimi-k2-0905",
          "temperature": 0.5
        },
        {
          "name": "glm-4.6",
          "auto_temperature": true
        }
      ],
      "synthesizer": {
        "ref": "glm-4.6"
      }
    }
  ],
  "synthesizer": {
    "ref": "glm-4.6"
  }
}"""

# 全局超时（必填）
[workflow.timeouts]
analyzer_timeout_secs = 30         # 分析器默认超时（秒）
worker_timeout_secs = 60           # 工作者默认超时（秒）
synthesizer_timeout_secs = 60      # 综合器默认超时（秒）

# 域名覆盖：根据模型 api_base 的域名进行部分或全部覆盖
[workflow.domains]
[workflow.domains."api.example.com"]
analyzer_timeout_secs = 40
worker_timeout_secs = 80

[workflow.domains."app.example.com"]
analyzer_timeout_secs = 20
synthesizer_timeout_secs = 30
```

说明：
- `[workflow-integration]` 节点现在通过 `json` 字段保存完整的工作流结构，推荐使用三引号 `"""` 包裹多行 JSON，保证可读性。
- 分析器与综合器节点通过 `ref` 字段引用上方 `[[model]]` 中声明的模型名称；普通工作节点使用 `name` 字段引用模型。
- `workers` 数组可以混合模型节点和子工作流：只要对象内包含 `analyzer` / `workers` / `synthesizer` 字段，就会被视为一个递归子工作流。
- JSON 节点内的 `temperature` / `auto_temperature` 会优先于模型默认值；未设置时回落到模型配置或分析器产出的温度。
- 超时规则保持不变：先使用 `[workflow.timeouts]` 的全局默认值，再按域名覆盖缺省字段。

> 升级提示：若检测到旧版的 `workflow-integration` 配置（如 analyzer/workers/synthesizer 表格，或包含 `analyzer_model` / `worker_models` / `synthesizer_model` 字段），Chorus 会自动迁移到 `[workflow-integration].json` 格式，并在同目录生成 `config.toml.bak` 备份文件。

## 📚 API 文档

### 1. 生成接口（Generate API）

类似于 Ollama 的 `/api/generate` 接口。

**请求**

```bash
curl -H 'Content-Type: application/json' \
  http://localhost:11435/api/generate \
  -d '{
    "model": "chorus",
    "prompt": "你好"
  }'
```

**参数**

| 参数 | 类型 | 必需 | 描述 |
|------|------|------|------|
| `model` | string | 否 | 模型名称（默认: "chorus"） |
| `prompt` | string | 是 | 用户提示词 |
| `stream` | boolean | 否 | 是否使用 Server-Sent Events 流式返回（默认: false） |
| `include_workflow` | boolean | 否 | 是否在响应中返回工作流执行详情（默认: false） |

**响应（使用示例配置运行时会返回 mock 数据）**

```json
{
  "model": "chorus",
  "created_at": "2025-10-20T13:23:23.284964394+00:00",
  "response": "mock reply",
  "done": true
}
```
> 提示：使用真实 API Key 时，这里会返回实际模型的回答。

若请求体中将 `"include_workflow": true`，响应会包含一个 `workflow` 字段，描述分析器、工作节点和综合器的执行详情。例如：

```json
{
  "model": "chorus",
  "created_at": "2025-10-20T13:23:23.284964394+00:00",
  "response": "mock reply",
  "done": true,
  "workflow": {
    "analyzer": {
      "model": "glm-4.6",
      "temperature": 1.2,
      "auto_temperature": true
    },
    "workers": [
      {
        "name": "glm-4.6",
        "temperature": 1.2,
        "response": "mock worker reply",
        "success": true,
        "error": null
      }
    ],
    "synthesizer": {
      "model": "glm-4.6",
      "temperature": 1.2
    }
  }
}
```

### 2. 聊天接口（Chat API）

类似于 Ollama 的 `/api/chat` 接口。

**请求**

```bash
curl -H 'Content-Type: application/json' \
  http://localhost:11435/api/chat \
  -d '{
    "model": "chorus",
    "messages": [
      {
        "role": "user",
        "content": "你好"
      }
    ]
  }'
```

**返回工作流详情的请求**

```bash
curl -H 'Content-Type: application/json' \
  http://localhost:11435/api/chat \
  -d '{
    "model": "chorus",
    "messages": [
      {
        "role": "user",
        "content": "你好"
      }
    ],
    "include_workflow": true
  }'
```

**参数**

| 参数 | 类型 | 必需 | 描述 |
|------|------|------|------|
| `model` | string | 否 | 模型名称 |
| `messages` | array | 是 | 消息历史数组 |
| `stream` | boolean | 否 | 是否使用 Server-Sent Events 流式返回（默认: false） |
| `include_workflow` | boolean | 否 | 是否在响应中返回工作流执行详情（默认: false） |

**响应（示例配置下的 mock 数据）**

```json
{
  "model": "chorus",
  "created_at": "2025-10-20T13:27:54.677058219+00:00",
  "message": {
    "role": "assistant",
    "content": "mock reply"
  },
  "done": true
}
```
> 提示：使用真实 API Key 时，这里会返回实际模型的回答。

### 3. 列出模型（List Models）

**请求**

```bash
curl http://localhost:11435/api/tags
```

**响应**

```json
{
  "models": [
    {
      "name": "qwen3-max",
      "model": "qwen3-max",
      "modified_at": "2024-01-15T10:30:00Z"
    },
    ...
  ]
}
```

### 4. 健康检查

**请求**

```bash
curl http://localhost:11435/
```

**响应**

```json
{
  "status": "ok",
  "service": "Chorus",
  "version": "0.1.0"
}
```

## 🔄 工作流程详解

### 步骤 1：智能分析

Chorus 首先使用 GLM-4.6 分析用户的提示词，判断问题类型并决定最适合的 temperature 参数：

- **创意性问题**（如写作、头脑风暴）→ 较高 temperature (1.0-1.5)
- **事实性问题**（如知识问答）→ 较低 temperature (0.3-0.7)
- **代码生成**（如编程任务）→ 低 temperature (0.1-0.5)

### 步骤 2：多模型协同

使用分析得出的 temperature，**按顺序**调用 7 个不同的模型：

1. **qwen3-max** - 阿里通义千问最新模型
2. **qwen3-vl-plus** - 支持视觉理解的多模态模型
3. **kimi-k2-0905** - Moonshot AI 的长文本模型
4. **glm-4.6** - 智谱 AI 的对话模型
5. **deepseek-v3.2** - DeepSeek 最新版本
6. **deepseek-v3.1** - DeepSeek 稳定版本
7. **deepseek-r1** - DeepSeek 推理优化版本

每个模型独立处理问题，如果某个模型失败，会继续执行其他模型。

### 步骤 3：智能综合

使用 GLM-4.6 综合所有模型的响应，生成最终答案：

- 提取各模型回答的优点
- 去除重复和冗余信息
- 整合成连贯、准确的答案
- 确保逻辑清晰、结构完整

## 🛠️ 开发指南

### 项目结构

```
Chorus/
├── Cargo.toml           # 项目依赖配置
├── ~/.config/chorus/config.toml # 用户级服务配置文件（默认优先）
├── README.md            # 项目文档
└── src/
    ├── main.rs          # 程序入口
    ├── config.rs        # 配置管理
    ├── server.rs        # HTTP 服务器
    ├── llm.rs           # LLM 客户端
    └── workflow.rs      # 工作流引擎
```

### 运行测试（含域名覆盖单元测试）

```bash
cargo test
# 关键测试位置：src/config_tests.rs（覆盖全局、完整域名覆盖、部分覆盖三种场景）
```

### 测试指南

- 单元测试

```bash
cargo test
```

- 启动与健康检查（先在 ~/.config/chorus/config.toml 配好 api_key）

```bash
RUST_LOG=info cargo run
curl http://localhost:11435/
```

- 端到端验证

```bash
curl -H 'Content-Type: application/json' \
  http://localhost:11435/api/generate \
  -d '{"model":"chorus","prompt":"hello"}'

curl -H 'Content-Type: application/json' \
  http://localhost:11435/api/chat \
  -d '{"model":"chorus","messages":[{"role":"user","content":"hi"}]}'
```

- 代码检查（可选）

```bash
cargo fmt -- --check
cargo clippy -D warnings
```


### 代码格式化

```bash
cargo fmt
```

### 代码检查

```bash
cargo clippy
```

### 启用调试日志

```bash
RUST_LOG=debug cargo run
```

## 📊 性能优化

### 生产环境建议

1. **使用 Release 模式编译**

```bash
cargo build --release
```

2. **调整超时时间**

根据实际网络情况调整 `~/.config/chorus/config.toml` 中的超时配置：

```toml
[workflow.timeouts]
analyzer_timeout_secs = 30
worker_timeout_secs = 120    # 如果网络较慢，可以增加
synthesizer_timeout_secs = 90
```

3. **日志级别**

生产环境建议使用 `info` 级别：

```bash
RUST_LOG=info ./target/release/chorus
```

## 🐛 故障排除

### 问题 1：API Key 无效

**错误信息**：`LLM API request failed with status 401`

**解决方案**：检查 `~/.config/chorus/config.toml` 中的 `api_key` 是否正确配置。

### 问题 2：超时错误

**错误信息**：`request timeout`

**解决方案**：增加 `~/.config/chorus/config.toml` 中的超时时间配置。

### 问题 3：端口被占用

**错误信息**：`Address already in use`

**解决方案**：修改 `~/.config/chorus/config.toml` 中的端口号，或停止占用 11435 端口的程序。

### 问题 4：所有工作模型失败

**错误信息**：`All worker models failed`

**解决方案**：
- 检查网络连接
- 确认 API Key 有足够的配额
- 查看详细日志 `RUST_LOG=debug cargo run`

## 🔒 安全建议

1. **保护 API Key**
   - 不要将 API Key 提交到版本控制
   - 使用环境变量存储敏感信息

2. **网络安全**
   - 在生产环境中配置防火墙
   - 考虑使用反向代理（如 Nginx）

3. **访问控制**
   - 限制服务监听地址（默认 127.0.0.1 仅本地访问）
   - 实现 API 认证机制（可扩展）

## 🗺️ 路线图

- [ ] 支持流式响应（SSE）
- [ ] 添加请求缓存机制
- [ ] 支持自定义工作流配置
- [ ] 添加 Prometheus 监控指标
- [ ] 实现负载均衡
- [ ] 支持更多 LLM 提供商
- [ ] Web UI 管理界面
- [ ] Docker 容器化

## 🤝 贡献

欢迎贡献代码！请遵循以下步骤：

1. Fork 本仓库
2. 创建特性分支 (`git checkout -b feature/AmazingFeature`)
3. 提交更改 (`git commit -m 'Add some AmazingFeature'`)
4. 推送到分支 (`git push origin feature/AmazingFeature`)
5. 开启 Pull Request

## 📄 许可证

本项目采用 MIT 许可证 - 查看 [LICENSE](LICENSE) 文件了解详情。

## 🙏 致谢

- [Ollama](https://github.com/ollama/ollama) - API 设计灵感
- [iFlow](https://apis.iflow.cn/) - LLM API 提供商
- Rust 社区的所有贡献者

## 📧 联系方式

- 问题反馈：[GitHub Issues](https://github.com/yourusername/chorus/issues)
- 邮箱：your.email@example.com

---

<div align="center">

**[⬆ 回到顶部](#chorus-)**

用 ❤️ 和 Rust 构建

</div>
