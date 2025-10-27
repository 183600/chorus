# Temperature 配置功能说明

## 概述

本次更新为 Chorus 添加了灵活的 temperature 配置功能，支持三种配置方式：
1. 固定 temperature 值
2. 由大模型自动选择 temperature
3. 使用默认值

## 配置项说明

### 1. temperature（可选）

- **类型**：浮点数（f32）
- **取值范围**：0.0 - 2.0
- **默认值**：无（如果不设置）
- **说明**：为模型设置固定的 temperature 值

**取值建议**：
- `0.0-0.3`：非常确定和保守的输出，适合需要精确答案的场景（如数学计算、代码生成）
- `0.4-0.7`：平衡的输出，适合大多数场景（如一般问答、文档编写）
- `0.8-1.2`：更有创造性的输出，适合创意写作等场景（如故事创作、头脑风暴）
- `1.3-2.0`：非常随机和创造性的输出（如艺术创作、实验性内容）

### 2. auto_temperature（可选）

- **类型**：布尔值（bool）
- **取值**：true 或 false
- **默认值**：false
- **说明**：是否让分析器模型自动决定最佳 temperature

**工作原理**：
- 当设置为 `true` 时，分析器模型（analyzer_model）会分析用户的提示词
- 根据问题类型（创意性、事实性、技术性等）推荐合适的 temperature
- 推荐的 temperature 会应用到所有工作模型（worker_models）

## 配置示例

### 示例 1：固定 temperature

```toml
[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "your-api-key-here"
name = "qwen3-max"
temperature = 0.8  # 固定使用 0.8
```

适用场景：
- 已知该模型在特定 temperature 下表现最佳
- 需要稳定、可预测的输出

### 示例 2：自动 temperature

```toml
[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "your-api-key-here"
name = "glm-4.6"
auto_temperature = true  # 由分析器自动决定
```

适用场景：
- 处理多样化的用户问题
- 希望根据问题类型动态调整输出风格
- 不确定最佳 temperature 值

### 示例 3：使用默认值

```toml
[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "your-api-key-here"
name = "deepseek-v3.2"
# 不设置任何 temperature 参数，将使用默认值 1.4
```

适用场景：
- 使用模型的默认行为
- 不需要特殊调整

### 示例 4：混合配置

```toml
# 分析器使用自动 temperature
[[model]]
name = "glm-4.6"
auto_temperature = true

# 某些工作模型使用固定值
[[model]]
name = "qwen3-max"
temperature = 0.8

# 其他工作模型使用默认值
[[model]]
name = "deepseek-v3.2"
```

## 优先级规则

配置的优先级从高到低：

1. **固定 temperature 值** - 如果设置了 `temperature`，则始终使用该值
2. **自动选择** - 如果设置了 `auto_temperature = true` 且没有设置 `temperature`，则由分析器决定
3. **默认值** - 如果两者都未设置，则使用 1.4

**注意**：如果同时设置了 `temperature` 和 `auto_temperature = true`，`temperature` 优先，`auto_temperature` 会被忽略。

## 工作流程

### 启用 auto_temperature 时的流程

```
用户提示
    ↓
分析器模型（analyzer_model）
    ↓
分析问题类型并推荐 temperature
    ↓
应用到所有工作模型（worker_models）
    ↓
生成响应
    ↓
综合器模型（synthesizer_model）
    ↓
最终答案
```

### 使用固定 temperature 时的流程

```
用户提示
    ↓
直接使用配置的 temperature
    ↓
应用到对应的模型
    ↓
生成响应
    ↓
综合器模型
    ↓
最终答案
```

## 代码实现

### 配置结构（src/config.rs）

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub name: String,
    pub api_base: String,
    pub api_key: String,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub auto_temperature: Option<bool>,
}
```

### 工作流逻辑（src/workflow.rs）

1. **分析阶段**：
   - 检查是否配置了固定 temperature → 直接使用
   - 检查是否启用 auto_temperature → 调用分析器
   - 否则使用默认值 1.4

2. **工作阶段**：
   - 每个工作模型优先使用自己配置的 temperature
   - 如果没有配置，则使用分析器推荐的 temperature

3. **综合阶段**：
   - 综合器模型使用自己配置的 temperature
   - 如果没有配置，则使用默认值 1.4

## 测试建议

### 测试固定 temperature

```bash
# 修改配置文件，设置固定 temperature
[[model]]
name = "glm-4.6"
temperature = 0.3

# 运行服务并测试
RUST_LOG=debug cargo run

# 发送请求
curl -H 'Content-Type: application/json' \
  http://localhost:11435/api/generate \
  -d '{"model":"chorus","prompt":"写一首诗"}'

# 检查日志，应该看到 "Using temperature 0.3"
```

### 测试自动 temperature

```bash
# 修改配置文件，启用自动选择
[[model]]
name = "glm-4.6"
auto_temperature = true

# 运行服务并测试
RUST_LOG=debug cargo run

# 发送不同类型的请求
curl -H 'Content-Type: application/json' \
  http://localhost:11435/api/generate \
  -d '{"model":"chorus","prompt":"1+1等于几？"}'

curl -H 'Content-Type: application/json' \
  http://localhost:11435/api/generate \
  -d '{"model":"chorus","prompt":"写一个科幻故事"}'

# 检查日志，应该看到不同的 temperature 值
```

## 常见问题

### Q1: 如何为不同类型的问题设置不同的 temperature？

A: 使用 `auto_temperature = true`，让分析器根据问题类型自动选择。

### Q2: 可以为不同的模型设置不同的 temperature 吗？

A: 可以。每个模型配置块都可以独立设置 `temperature` 或 `auto_temperature`。

### Q3: temperature 设置对哪些模型生效？

A: 
- analyzer_model：使用固定的 0.3（用于分析）
- worker_models：使用各自配置的 temperature
- synthesizer_model：使用配置的 temperature 或默认 1.4

### Q4: 如果我想让所有模型都使用相同的 temperature，怎么办？

A: 为每个模型配置块设置相同的 `temperature` 值。

### Q5: auto_temperature 会增加延迟吗？

A: 会略微增加延迟，因为需要先调用分析器模型。但这个延迟通常很小（几秒），且可以通过 `analyzer_timeout_secs` 配置控制。

## 更新日志

### 2025-10-22

- ✅ 添加 `temperature` 配置项到 `ModelConfig`
- ✅ 添加 `auto_temperature` 配置项到 `ModelConfig`
- ✅ 更新 `workflow.rs` 以支持新的配置逻辑
- ✅ 更新默认配置模板（`DEFAULT_CONFIG`）
- ✅ 更新 `test-config.toml` 示例
- ✅ 创建 `config-example.toml` 详细示例
- ✅ 更新 `README.md` 文档
- ✅ 创建本说明文档

## 参考资料

- [OpenAI Temperature 参数说明](https://platform.openai.com/docs/api-reference/chat/create#chat-create-temperature)
- [Chorus 项目 README](README.md)
- [配置示例文件](config-example.toml)
