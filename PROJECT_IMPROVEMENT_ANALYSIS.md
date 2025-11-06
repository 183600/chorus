# Project Improvement Analysis

## Executive Summary

- **Correctness:** Worker-level `auto_temperature` settings are currently ignored, so operators cannot rely on configuration to tune individual models automatically.
- **Performance:** Worker executions and HTTP client creation happen sequentially, producing avoidable latency and CPU overhead for every request.
- **Resilience & Observability:** Several places panic or log overly verbose payloads; tightening error handling and log redaction will make the service safer to operate.

## Findings & Recommendations

### 1. Worker `auto_temperature` is ignored
- **Where:** [`WorkflowEngine::resolve_worker_temperature`](src/workflow.rs#L573-L586).
- **What happens:** The function only checks explicit `temperature` overrides and falls back to the analyzer temperature. The `auto_temperature` flag exposed in `WorkflowModelTarget` and the TOML schema is never consulted.
- **Impact:** Any config that enables `auto_temperature = true` on a worker silently behaves as if it were `false`, reducing answer quality and confusing operators.
- **Recommendation:** Respect the auto flag by either reusing the analyzer’s computed temperature, requesting a per-model recommendation, or at minimum rejecting configs that set it (to avoid silent failure). Extend unit tests to cover this path.

### 2. Workers run strictly sequentially and rebuild HTTP clients per call
- **Where:** [`WorkflowEngine::run_workers_with_details`](src/workflow.rs#L305-L471) iterates with `for` and awaits each worker call directly; [`LLMClient::new`](src/llm.rs#L40-L58) builds a fresh `reqwest::Client` for every invocation.
- **Impact:** A single request waits for all workers serially; with the default seven models this multiplies response latency. Recreating the HTTP client for every worker costs TLS handshakes and connection pools, wasting CPU.
- **Recommendation:** Dispatch worker calls concurrently (e.g. `FuturesUnordered` with an optional concurrency limit) and reuse `reqwest::Client` instances—either cache them by domain/model or store them in the engine. Profile latency before/after to confirm gains.

### 3. `LLMClient::new` can panic when the HTTP client fails to build
- **Where:** [`LLMClient::new`](src/llm.rs#L47-L52).
- **What happens:** The builder uses `.build().unwrap()`. Any TLS or config error will terminate the process.
- **Impact:** One misconfigured timeout or OS-level TLS issue can crash the whole service instead of returning a 500.
- **Recommendation:** Return a `Result<Self>` (or construct the client once during startup and propagate the error) so the server can surface configuration issues gracefully.

### 4. Debug logging emits full prompt payloads
- **Where:** [`LLMClient::chat_completion`](src/llm.rs#L75-L79) logs the entire JSON request body at `debug!` level.
- **Impact:** Prompts often contain sensitive data; even debug logs may be collected centrally. This conflicts with the README’s promise of “详细日志” without warning about PII risk.
- **Recommendation:** Redact prompt contents, gate verbose logging behind a compile-time feature, or truncate long payloads. Add guidance in the README about enabling sensitive logging.

### 5. SSE/streaming endpoints do not emit incremental tokens
- **Where:** [`generate`](src/server.rs#L143-L167) and [`chat`](src/server.rs#L209-L239) send the entire response in the first SSE event and a blank completion event.
- **Impact:** Clients expecting Ollama/OpenAI-style token streaming cannot render partial output, defeating the purpose of `stream = true`.
- **Recommendation:** Adjust the workflow to stream tokens as they arrive from the synthesizer (or at least chunk the final answer) to match client expectations. Update tests (e.g. `test_streaming.sh`) to assert streaming behavior.

## Additional Observations

- `AppError` logs errors with `{:?}`, which can include configuration structs containing API keys. Introducing redacted Debug implementations (or formatting `Display` only) would reduce leakage risk.
- Consider adding integration tests for the `/v1/responses` path and nested workflow execution to guard against regressions during future refactors.
- The README promises “自适应参数” for multiple stages; once the auto-temperature bug is fixed, add documentation describing analyzer/worker behavior so operators know what to expect.
