# Chorus

<div align="center">

**一个智能的 LLM API 聚合服务，通过多模型协同提供更高质量的 AI 响应**

[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![API](https://img.shields.io/badge/API-Ollama--compatible-green.svg)](https://github.com/ollama/ollama)

</div>

## 目录
- [简介](#简介)
- [核心特性](#核心特性)
- [架构与工作流](#架构与工作流)
- [快速上手](#快速上手)
- [配置指南](#配置指南)
- [API 使用](#api-使用)
- [工作流执行流程](#工作流执行流程)
- [开发者指南](#开发者指南)
- [故障排除](#故障排除)
- [安全建议](#安全建议)
- [路线图](#路线图)
- [贡献指南](#贡献指南)
- [许可证](#许可证)
- [联系方式](#联系方式)

## 简介

Chorus 是一个使用 Rust 和 Tokio 构建的高性能 LLM API 聚合服务，提供与 Ollama/OpenAI 兼容的接口。它通过四步智能工作流（分析 → 协同 → 甄选 → 综合）来组合多种模型的优势，产出更专业、可靠的回答。

- 面向开发者：一套配置灵活、易于集成的统一 API。
- 面向团队：可同步多个模型的能力，不断提升回答质量。
- 面向生产：内置日志、超时和错误处理机制，易于观测和维护。

## 核心特性

- 🚀 **高性能**：基于 Rust + Tokio 的异步运行时，启动快、占用低。
- 🎼 **四步智能工作流**：分析器、工作节点、选择器、综合器协同工作。
- 🎯 **自适应 Temperature**：自动根据问题类型推荐最优 temperature，亦可手动覆盖。
- 🤝 **多模型协作**：一次请求可串行/递归调用多个模型，降低单点风险。
- 🧠 **最佳答案甄选**：Selector 自动在多个回复中挑选最优候选。
- 🔌 **Ollama/OpenAI 兼容**：可直接连接 Cherry Studio、OpenAI SDK 等常见工具。
- 🧾 **可观测性**：详细的工作流执行日志，支持返回完整执行轨迹。
- 🔧 **灵活配置**：TOML + JSON 混合配置，自由定义嵌套工作流、超时与域名覆盖。

## 架构与工作流

```
┌─────────────┐
│   Client    │
└──────┬──────┘
       │ HTTP Request
       ▼
┌─────────────────────────────────────┐
│           Chorus Server             │
│  ┌───────────────────────────────┐  │
│  │ Step 1: Prompt Analysis       │  │
│  │  • 决定 temperature 与策略      │  │
│  └───────────┬───────────────────┘  │
│              ▼                       │
│  ┌───────────────────────────────┐  │
│  │ Step 2: Multi-Model Workers   │  │
│  │  • 串行调用多个模型             │  │
│  │  • 支持嵌套子工作流             │  │
│  └───────────┬───────────────────┘  │
│              ▼                       │
│  ┌───────────────────────────────┐  │
│  │ Step 3: Response Selector     │  │
│  │  • 评估并挑选最佳候选           │  │
│  └───────────┬───────────────────┘  │
│              ▼                       │
│  ┌───────────────────────────────┐  │
│  │ Step 4: Response Synthesizer  │  │
│  │  • 综合答案并输出               │  │
│  └───────────────────────────────┘  │
└─────────────────────────────────────┘
```

| 阶段 | 角色 | 默认模型 | 主要职责 |
| --- | --- | --- | --- |
| Step 1 | Analyzer | `glm-4.6` | 判断问题类型、推导 temperature 等全局策略。 |
| Step 2 | Workers | `[[model]]` 列表 | 按顺序执行，支持嵌套子工作流和自定义超时。 |
| Step 3 | Selector | 默认 `qwen3-max` | 评估全部候选回复并挑选最优答案。 |
| Step 4 | Synthesizer | `qwen3-max` | 将最佳候选与补充信息融合成最终响应。 |

## 快速上手

### 环境要求

- Rust 1.75 或更新版本
- 可访问互联网（调用第三方 LLM）
- 已获取可用的模型 API Key

### 安装

```bash
git clone https://github.com/yourusername/chorus.git
cd chorus
```

### 准备配置

1. 创建配置目录：
   ```bash
   mkdir -p ~/.config/chorus
   ```
2. 复制示例配置并根据实际身份验证信息修改：
   ```bash
   cp config-example.toml ~/.config/chorus/config.toml
   ```
3. 将 `your-api-key-here` 等占位符替换为真实的 API Key。
4. 推荐最小配置示例：
   ```toml
   [server]
   host = "127.0.0.1"
   port = 11435

   [[model]]
   name = "qwen3-max"
   api_base = "https://apis.iflow.cn/v1"
   api_key = "your-api-key"
   auto_temperature = true

   [workflow-integration]
   nested_worker_depth = 1
   json = """{
     "analyzer": {"ref": "glm-4.6", "auto_temperature": true},
     "workers": [{"name": "qwen3-max"}],
     "synthesizer": {"ref": "qwen3-max"},
     "selector": {"ref": "qwen3-max"}
   }"""

   [workflow.timeouts]
   analyzer_timeout_secs = 30
   worker_timeout_secs = 60
   synthesizer_timeout_secs = 60
   ```

> 提示：还可以参考 `config-json-format-example.toml` 获取嵌套工作流、域名覆盖等高级用法。

#### 临时测试配置（无需覆盖默认文件）

在联调或验收过程中，经常会收到一份“只在当前周期有效”的配置（比如本工单里附带的示例）。现在可以通过 CLI 参数或环境变量临时加载它，而不必改动 `~/.config/chorus/config.toml`：

1. 将临时配置保存到任意位置，例如 `/tmp/chorus-temp.toml`。
2. 启动 Chorus 时带上 `--config` 参数（优先级最高）：
   ```bash
   cargo run -- --config /tmp/chorus-temp.toml
   ```
   或者在运行编译后的二进制时使用环境变量：
   ```bash
   CHORUS_CONFIG=/tmp/chorus-temp.toml ./target/release/chorus
   ```
3. 测试完成后删除/重命名该文件即可，默认配置无需回滚，也不会把临时密钥写入版本库。

> `--config` CLI 参数的优先级高于环境变量 `CHORUS_CONFIG`，两者都未设置时才会回落到 `~/.config/chorus/config.toml`。

### 启动服务

```bash
# 开发模式
RUST_LOG=info cargo run

# 生产模式（优化编译）
cargo build --release
RUST_LOG=info ./target/release/chorus
```

服务默认监听 `http://127.0.0.1:11435`。

### 快速验证

```bash
curl -H 'Content-Type: application/json' \
  http://127.0.0.1:11435/api/generate \
  -d '{"model":"chorus","prompt":"你好"}'

curl -H 'Content-Type: application/json' \
  http://127.0.0.1:11435/api/chat \
  -d '{"model":"chorus","messages":[{"role":"user","content":"你好"}]}'
```

若需查看完整工作流执行轨迹，可在请求体中添加 `"include_workflow": true`。

## 配置指南

### 服务器设置

```toml
[server]
host = "127.0.0.1"  # 服务监听地址
port = 11435          # 服务监听端口
```

将 `host` 修改为 `0.0.0.0` 即可允许局域网访问。部署到公网时建议配合反向代理和认证机制。

### 模型定义

```toml
[[model]]
name = "qwen3-max"          # 唯一名称，用于 workflow 引用
api_base = "https://apis.iflow.cn/v1"
api_key = "your-api-key"
auto_temperature = true      # 可选：允许 analyzer 自动调节
# temperature = 0.8          # 可选：强制使用固定 temperature（高于 auto_temperature 优先级）
```

可按需新增多个 `[[model]]` 块，同时支持不同供应商的 API 地址。

#### Temperature 策略

- `temperature`：使用明确的固定值（0.0 ~ 2.0）。
- `auto_temperature = true`：交给 Analyzer 根据问题自动决策。
- 未配置时默认使用 `1.4`。
- 优先级：固定值 > 自动决策 > 默认值。

### 工作流配置

`[workflow-integration]` 使用 JSON 描述完整的嵌套工作流结构：

```toml
[workflow-integration]
json = """{
  "analyzer": {"ref": "glm-4.6", "auto_temperature": true},
  "workers": [
    {"name": "deepseek-v3.1", "temperature": 1.0},
    {
      "analyzer": {"ref": "glm-4.6", "auto_temperature": true},
      "workers": [
        {"name": "kimi-k2-0905"},
        {"name": "qwen3-coder", "temperature": 0.6}
      ],
      "synthesizer": {"ref": "qwen3-max"}
    }
  ],
  "selector": {"ref": "qwen3-max"},
  "synthesizer": {"ref": "qwen3-max"}
}"""
```

要点：

- `analyzer` / `selector` / `synthesizer` 使用 `ref` 引用上方的 `[[model]]` 名称。
- `workers` 可混合模型节点与子工作流，实现递归流程。
- JSON 内的 `temperature` / `auto_temperature` 优先级高于模型默认值。

#### 嵌套工作流层级（nested_worker_depth）

`nested_worker_depth` 用来控制系统自动构建的冗余嵌套层级，默认值为 `1`，表示每个 Worker 只执行一次，与当前行为一致。当该值大于 1 时，Chorus 会在配置解析阶段为每个 Worker 包装 `n-1` 层与父级相同的 analyzer/synthesizer（或 selector），并在每一层内复制两份同样的 Worker，使得单个 Worker 的实际执行次数增至 `2^(n-1)`，便于获取更多候选答案进行甄选和综合。

```toml
[workflow-integration]
nested_worker_depth = 1
json = """{
  "analyzer": {"ref": "glm-4.6", "auto_temperature": true},
  "workers": [
    {"name": "kimi-k2-0905"},
    {"name": "qwen3-coder", "temperature": 0.6}
  ],
  "synthesizer": {"ref": "qwen3-max"}
}"""
```

当 `nested_worker_depth = 2` 时，上述配置会被自动扩展为：

```json
{
  "analyzer": {"ref": "glm-4.6", "auto_temperature": true},
  "workers": [
    {
      "analyzer": {"ref": "glm-4.6", "auto_temperature": true},
      "workers": [
        {"name": "kimi-k2-0905"},
        {"name": "kimi-k2-0905"}
      ],
      "synthesizer": {"ref": "qwen3-max"}
    },
    {
      "analyzer": {"ref": "glm-4.6", "auto_temperature": true},
      "workers": [
        {"name": "qwen3-coder", "temperature": 0.6},
        {"name": "qwen3-coder", "temperature": 0.6}
      ],
      "synthesizer": {"ref": "qwen3-max"}
    }
  ],
  "synthesizer": {"ref": "qwen3-max"}
}
```

当 `nested_worker_depth = 3` 时，会在上一结构基础上再嵌套一层（每个 Worker 被复制 4 次），等价结构如下：

```json
{
  "analyzer": {"ref": "glm-4.6", "auto_temperature": true},
  "workers": [
    {
      "analyzer": {"ref": "glm-4.6", "auto_temperature": true},
      "workers": [
        {
          "analyzer": {"ref": "glm-4.6", "auto_temperature": true},
          "workers": [
            {"name": "kimi-k2-0905"},
            {"name": "kimi-k2-0905"}
          ],
          "synthesizer": {"ref": "qwen3-max"}
        },
        {
          "analyzer": {"ref": "glm-4.6", "auto_temperature": true},
          "workers": [
            {"name": "kimi-k2-0905"},
            {"name": "kimi-k2-0905"}
          ],
          "synthesizer": {"ref": "qwen3-max"}
        }
      ],
      "synthesizer": {"ref": "qwen3-max"}
    },
    {
      "analyzer": {"ref": "glm-4.6", "auto_temperature": true},
      "workers": [
        {
          "analyzer": {"ref": "glm-4.6", "auto_temperature": true},
          "workers": [
            {"name": "qwen3-coder", "temperature": 0.6},
            {"name": "qwen3-coder", "temperature": 0.6}
          ],
          "synthesizer": {"ref": "qwen3-max"}
        },
        {
          "analyzer": {"ref": "glm-4.6", "auto_temperature": true},
          "workers": [
            {"name": "qwen3-coder", "temperature": 0.6},
            {"name": "qwen3-coder", "temperature": 0.6}
          ],
          "synthesizer": {"ref": "qwen3-max"}
        }
      ],
      "synthesizer": {"ref": "qwen3-max"}
    }
  ],
  "synthesizer": {"ref": "qwen3-max"}
}
```

依此类推，可以通过调高 `nested_worker_depth` 快速获得更多冗余的 Worker 执行次数，而无需手写庞大的嵌套 JSON。

### 超时与域名覆盖

```toml
[workflow.timeouts]
analyzer_timeout_secs = 30
worker_timeout_secs = 60
synthesizer_timeout_secs = 60

[workflow.domains."api.example.com"]
worker_timeout_secs = 80

[workflow.domains."app.example.com"]
analyzer_timeout_secs = 20
synthesizer_timeout_secs = 30
```

- 所有超时配置均以秒为单位。
- 先应用全局超时，再按域名覆盖缺省字段。
- 域名读取自模型 `api_base` 的主机名，支持部分字段覆盖。

> 升级提醒：检测到旧版 workflow 配置时，Chorus 会自动迁移为 `[workflow-integration].json` 格式，并在同目录生成 `config.toml.bak` 备份文件。

## API 使用

### `/api/generate`

- **方法**：`POST`
- **说明**：与 Ollama `generate` 接口兼容，支持文本生成和可选流式输出（SSE）。

```bash
curl -H 'Content-Type: application/json' \
  http://127.0.0.1:11435/api/generate \
  -d '{
    "model": "chorus",
    "prompt": "写一段 Rust 程序打印 Hello World",
    "stream": false,
    "include_workflow": true
  }'
```

响应示例：
```json
{
  "model": "chorus",
  "created_at": "2025-10-20T13:23:23.284964394+00:00",
  "response": "...",
  "done": true,
  "workflow": { "analyzer": {"model": "glm-4.6", "temperature": 0.7}, ... }
}
```

当 `stream=true` 时，接口会以 SSE 推送分段响应。

### `/api/chat`

- **方法**：`POST`
- **说明**：兼容 Ollama `chat` 接口，支持对话上下文与流式输出。

```bash
curl -H 'Content-Type: application/json' \
  http://127.0.0.1:11435/api/chat \
  -d '{
    "model": "chorus",
    "messages": [
      {"role": "system", "content": "你是一名 Rust 专家"},
      {"role": "user", "content": "讲解一下所有权模型"}
    ],
    "include_workflow": true
  }'
```

### OpenAI 兼容接口

Chorus 同时实现了一组与 OpenAI API 保持兼容的端点：

| Endpoint | 对应功能 |
| --- | --- |
| `POST /v1/chat/completions` | 等同于 `/api/chat`，支持流式增量输出。 |
| `POST /v1/completions` | 等同于 `/api/generate`，支持字符串或字符串数组 prompt。 |
| `POST /v1/responses` | 兼容 OpenAI Responses API，支持标准 SSE 流式输出（`response.created` → `response.output_text.delta` → `response.completed` → `[DONE]`），也可非流式返回。 |
| `GET /v1/models` | 返回符合 OpenAI 规范的模型列表。 |

#### Cherry Studio 快速配置

1. 打开 **Settings → Provider**。
2. 选择 **Ollama** 作为提供商。
3. 模型名称填写 `chorus`（或任意自定义名称）。
4. API 地址设置为 `http://127.0.0.1:11435`。

保存后即可在 Cherry Studio 中直接调用 Chorus。

## 工作流执行流程

一次完整的请求大致包含以下阶段：

1. **智能分析**：Analyzer 根据提示词类型给出合适的 temperature 与策略。
2. **多模型协同**：按照配置顺序依次调用多个模型节点，失败的节点会记录错误但不影响后续执行。
3. **候选甄选**：Selector 基于评分、理由等维度选出最优候选答案，并可返回完整评估信息。
4. **答案综合**：Synthesizer 将最佳候选与其他辅助信息整合，输出结构化的最终回复。

可选地，响应内的 `workflow` 字段会详细记录每一步的执行结果、耗时与错误信息，便于调试和优化。

## 开发者指南

### 项目结构

```
Chorus/
├── Cargo.toml
├── README.md
├── src/
│   ├── main.rs          # 程序入口
│   ├── config.rs        # 配置解析与校验
│   ├── server.rs        # HTTP 服务及路由
│   ├── llm.rs           # 对接外部 LLM 的客户端
│   └── workflow.rs      # 工作流调度逻辑
└── ~/.config/chorus/    # 默认用户级配置目录
```

### 常用命令

```bash
cargo build            # 编译（开发模式）
cargo build --release  # 编译（发布模式）
cargo run              # 运行服务
cargo test             # 执行单元测试（含域名覆盖测试）
cargo fmt              # 格式化代码
cargo fmt -- --check   # 检查格式
cargo clippy -D warnings  # 静态检查
```

### 启用调试日志

```bash
RUST_LOG=debug cargo run
```

## 故障排除

| 场景 | 提示信息 | 排查建议 |
| --- | --- | --- |
| API Key 无效 | `LLM API request failed with status 401` | 检查 `api_key` 是否正确、是否具备访问权限。 |
| 请求超时 | `request timeout` | 增加 `workflow.timeouts` 或域名覆盖，确认网络状况。 |
| 端口冲突 | `Address already in use` | 修改配置端口或释放 11435 端口。 |
| 模型未在配置中定义 | `Workflow configuration references undefined model(s): deepseek-v3.2`、`Model 'xxx' not found in configuration. Did you define it under [[model]]?` 或 `Worker lookup failed worker=xxx` | 确认 workflow 中引用的所有模型名称（analyzer / workers / synthesizer）在 `[[model]]` 段都有一致的 `name` 字段；若缺少则新增对应模型配置，修改后重启服务。 |
| 所有工作节点失败 | `All worker models failed` | 核对网络、配额或模型状态，并查看 `RUST_LOG=debug` 日志。 |

## 安全建议

1. **保护凭据**：不要将 API Key 提交到版本库，推荐使用环境变量或密钥管理服务。
2. **网络安全**：生产环境中通过防火墙或反向代理限制访问来源，启用 TLS。
3. **访问控制**：保留默认的 `127.0.0.1` 监听地址或实现额外的认证机制。
4. **日志合规**：在日志中避免打印敏感提示词或用户输入。

## 路线图

- [ ] 支持完整的流式 Responses API
- [ ] 启用请求级缓存与重试策略
- [ ] 自定义工作流的图形化编辑器
- [ ] Prometheus 指标与可观测性增强
- [ ] 负载均衡与集群调度能力
- [ ] 更多 LLM 供应商适配器
- [ ] 官方 Docker 镜像与部署脚本

## 贡献指南

欢迎社区贡献力量！

1. Fork 本仓库。
2. 创建特性分支：`git checkout -b feature/awesome-feature`。
3. 提交变更：`git commit -m 'Add awesome feature'`。
4. 推送分支：`git push origin feature/awesome-feature`。
5. 在 GitHub 上发起 Pull Request，并描述变更背景、测试情况。

> 注：提交前请确保通过 `cargo fmt` 与 `cargo test`，并附上必要的文档更新。

## 许可证

本项目采用 MIT 许可证，详见 [LICENSE](LICENSE)。

## 联系方式

- 问题反馈：[GitHub Issues](https://github.com/yourusername/chorus/issues)
- 邮箱：your.email@example.com

---

<div align="center">

**[⬆ 回到顶部](#chorus)**

用 ❤️ 和 Rust 构建

</div>
