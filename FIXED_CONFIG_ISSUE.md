# 修复说明：API 输出回答问题

## 问题描述

终端显示错误：
```
Worker lookup failed worker=deepseek-v3.2 depth=0 error=Model 'deepseek-v3.2' not found in configuration. Did you define it under [[model]]?
```

## 问题原因

配置文件的 workflow-integration 部分引用了 `deepseek-v3.2` 作为 worker 模型，但在 `[[model]]` 部分没有定义该模型的配置信息（API 端点、密钥等）。

## 解决方案

在配置文件的 `[[model]]` 部分添加 `deepseek-v3.2` 的定义：

```toml
[[model]]
api_base = "https://apis.iflow.cn/v1"
api_key = "your-api-key-here"
name = "deepseek-v3.2"
```

## 完整配置示例

已创建修复后的配置文件在 `~/.config/chorus/config.toml`，包含以下关键部分：

1. **模型定义** - 在 `[[model]]` 部分定义所有使用的模型
2. **工作流配置** - 在 `[workflow-integration]` 的 JSON 中引用已定义的模型

确保工作流中引用的所有模型名称（analyzer、workers、synthesizer）都在 `[[model]]` 部分有对应定义。

## 验证

修复后，系统可以正常：
1. 启动服务器
2. 调用 worker 模型（deepseek-v3.2）
3. 返回 API 响应（非流式和流式都正常）

## 注意事项

每当在工作流配置中添加新的模型引用时，都需要确保：
- 在 `[[model]]` 部分有该模型的定义
- 模型名称完全一致（大小写敏感）
- API 端点和密钥正确配置
