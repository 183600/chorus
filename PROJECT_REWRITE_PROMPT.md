# Chorus 项目重写 Prompt

> 用途：将下面的 prompt 投喂给能够根据规格从零实现项目的 LLM/Agent，确保生成的新实现覆盖 Chorus 的全部特性。

---
你是一名资深 Rust 平台工程师，擅长构建高并发 HTTP 服务与多模型编排系统。请**从零重写一个名为 Chorus 的 LLM API 聚合服务**，保证与以下规格完全一致。

## 1. 技术栈与基础要求
1. 使用 **Rust 1.75+**、Tokio 异步运行时和 Axum 构建。
2. 项目结构需至少包含：`main.rs`（入口/CLI）、`config.rs`（配置读取与校验）、`server.rs`（路由 + 控制器）、`llm.rs`（模型客户端）、`workflow.rs`（调度引擎）。保持模块化，便于单元测试。
3. 产出可执行文件 `chorus`，默认监听 `127.0.0.1:11435`，通过 `RUST_LOG` 控制日志级别。
4. 所有外部依赖（reqwest、serde、axum 等）须使用稳定版本，并启用 `tokio`+`rustls` 的异步 HTTP 客户端。

## 2. 配置系统
1. 配置文件优先级：CLI `--config` > 环境变量 `CHORUS_CONFIG` > 默认路径 `~/.config/chorus/config.toml`。
2. 配置格式采用 TOML，关键段落：
   - `[server]`：`host`、`port`。
   - `[[model]]`（可重复）：`name`、`api_base`、`api_key`、`auto_temperature`、可选 `temperature`。
   - `[workflow-integration]`：`nested_worker_depth`，以及 `json = """{ ... }"""` 描述完整 workflow。
   - `[workflow.timeouts]`：`analyzer_timeout_secs`、`worker_timeout_secs`、`synthesizer_timeout_secs`。
   - `[workflow.domains."host"]`：允许基于域名覆盖超时。
3. **必须**在启动时校验：Workflow JSON 中引用的 analyzer/workers/selector/synthesizer `ref` 全部在 `[[model]]` 列表中存在。若缺失，启动失败并报错 `Workflow configuration references undefined model(s): ...`。
4. 提供示例配置（basic + 带嵌套 workflow + JSON 格式示例），并支持“旧版配置自动迁移”说明。

## 3. 工作流引擎（核心能力）
1. 四个阶段依次执行：
   - **Analyzer**：读取用户请求，判定任务类型、计算温度、可覆写全局策略。
   - **Workers**：按配置顺序/嵌套结构调用多个模型，允许递归子流程与自定义温度；同一工作节点失败不应阻塞整个流程。
   - **Selector**：从所有 worker 输出中选出最优答案，解释理由。
   - **Synthesizer**：把最佳答案与补充事实结合成最终响应。
2. `nested_worker_depth` > 1 时，自动复制嵌套结构，使得每个 worker 产生 `2^(depth-1)` 份候选。
3. 温度策略：
   - 优先使用 worker 节点上显式 `temperature`；
   - 其次若 `auto_temperature = true` 则根据 analyzer 的建议温度（或独立策略）动态调整；
   - 否则回退到模型默认温度（默认 1.4）。
4. Worker 执行需**支持并发**（例如 `FuturesUnordered` 或带并发上限的任务队列），并且 HTTP 客户端要按模型/域名复用，避免重复构建。
5. 工作流执行结果可封装为 `WorkflowExecutionDetails`，包含各阶段耗时、成功/失败状态、错误原因，供 API 返回。

## 4. LLM 客户端层
1. 通过 `reqwest::Client` 复用连接池、使用 `rustls`。
2. 支持 OpenAI/Ollama 兼容的 `generate`/`chat` 请求，必要字段：`model`、`prompt`/`messages`、`stream`、`temperature` 等。
3. 调用链需要：
   - 自动注入 `Authorization: Bearer <api_key>`。
   - 对于 JSON Body 的 `debug` 日志需默认 **脱敏/截断**，避免完整 prompt 泄露。
   - 错误时返回结构化 `LLMError`，不要 panic。

## 5. API 契约
实现以下端点，统一通过 `axum` 构建，支持 JSON 与 SSE：

| Endpoint | 方法 | 说明 |
| --- | --- | --- |
| `/api/generate` | POST | 兼容 Ollama `generate`；`stream=true` 时逐 token SSE (`data: {"response":"..."}`)，最后 `data: [DONE]`。`
| `/api/chat` | POST | 兼容 Ollama `chat`；支持 `messages` 队列与 `include_workflow`。|
| `/v1/completions` | POST | OpenAI 文本补全等价接口。|
| `/v1/chat/completions` | POST | OpenAI Chat Completion，支持增量输出。|
| `/v1/responses` | POST | OpenAI Responses API，解析 `instructions`/`input`/`messages` 等复合字段生成 prompt。|
| `/v1/models` | GET | 返回现有 `[[model]]` 列表，字段与 OpenAI 格式兼容。|

附加要求：
1. 所有 API 均允许 `include_workflow=true`，返回 `workflow` 节点详情。
2. `/v1/responses` 端点需要鲁棒的 prompt 构造（messages/input/parts 嵌套、数组 + `type=text` 块），缺失内容时报 `invalid request: missing input/messages/prompt/instructions`。
3. Streaming 模式必须逐块推送（非整段一次性发送），最后补发送完成事件。
4. 所有响应包含 `model`、`created_at`、`response`（或增量字段），并在错误时返回 `AppError` 结构 `{error: {message, code}}`。

## 6. 日志、可观测性与安全
1. 统一使用 `tracing`：Info 级别记录请求摘要、Debug 级别记录裁剪后的 payload、Error 级别附带 `workflow_id`/`request_id`。
2. 对含敏感字段（API Key、用户 Prompt）默认脱敏；允许通过 feature flag or env 打开详细日志，并在 README 中强调风险。
3. 所有错误走 `AppError` 封装，禁止 panic / unwrap。HTTP 层返回 4xx/5xx。
4. 提供工作流执行轨迹（JSON），便于观测每个阶段的耗时与失败原因。

## 7. 文档 & 示例
1. 维护一个详尽的 `README.md`（中文），内容至少包括：简介、核心特性、架构图（ASCII）、快速上手、配置指南、API 用法、工作流解释、开发者指南、故障排除、安全建议、路线图。
2. `TEMPERATURE_CONFIG.md`：解释温度策略以及样例。
3. `FIXED_CONFIG_ISSUE.md` / `FIX_SUMMARY.md`：说明常见配置错误与解决方案。
4. `PROJECT_IMPROVEMENT_ANALYSIS.md`：给出未来优化建议（并发 worker、HTTP client 复用、日志脱敏、SSE token 化等）。
5. 提供 `config-example.toml`、`config-json-format-example.toml`、`test-old-config.toml` 等示例，涵盖嵌套 workflow、域名覆盖、模型定义完整性。

## 8. 测试与脚本
1. 使用 `cargo test` 覆盖：配置解析、workflow 校验（缺模型时报错）、`auto_temperature` 逻辑、`/v1/responses` prompt 提取、流式输出（可用集成测试脚本 `test_stream.sh`、`test_streaming.sh`）。
2. Shell 脚本 `test_api.sh`、`test_migration.sh` 用于手动回归。
3. 针对 `/v1/responses` 写专门单元测试，覆盖 `instructions`、`input`、`messages`、文本块数组、空 payload 等情况。

## 9. 非功能性要求
1. **性能**：Worker 并发执行 + HTTP Client 复用，避免串行等待；对长时间任务设置超时（默认 analyzer 30s、worker 60s、synthesizer 60s，域名级覆盖可调整）。
2. **可靠性**：部分 worker 失败不会中断整体流程；所有错误需落日志并在最终响应中留下痕迹。
3. **可扩展性**：Workflow JSON 支持嵌套子流程与未来节点（例如 reranker）。
4. **安全**：不在版本库中硬编码真实 API Key；README 中加入安全建议（保护凭据、网络安全、启用 TLS 等）。

## 10. 交付物
1. 完整可编译的 Rust 项目，符合上述模块划分。
2. 通过 `cargo fmt`, `cargo clippy -D warnings`, `cargo test`。
3. 附带示例配置/脚本/文档，仓库内不得包含敏感密钥。
4. README 顶部提供一句话简介以及徽章（Rust 版本、License、API 兼容性）。

请严格按照以上规格实现 Chorus；当需求存在歧义时，以 README 中的描述和行业最佳实践为准，优先保证安全/正确性/可调试性。
---
