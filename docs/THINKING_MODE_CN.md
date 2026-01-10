# 如何在 Claude Code ACP Agent 中启用 Thinking 模式

## 快速开始

### 方法 1: 环境变量 (最简单)

```bash
# 设置 thinking tokens 上限
export MAX_THINKING_TOKENS=4096

# 设置模型 (必须使用支持 thinking 的模型)
export ANTHROPIC_MODEL="claude-sonnet-4-20250514"

# 设置 API Key
export ANTHROPIC_API_KEY="your-api-key"

# 启动 agent
./claude-code-acp-rs
```

### 方法 2: 使用示例脚本

```bash
# 构建项目
cargo build --release

# 设置 API Key
export ANTHROPIC_API_KEY="your-api-key"

# 运行示例脚本 (已预配置 thinking 模式)
./examples/thinking_mode.sh
```

### 方法 3: 通过 ACP 客户端配置

在创建会话时通过 `_meta` 字段配置:

```json
{
  "method": "session/new",
  "params": {
    "cwd": "/path/to/project",
    "_meta": {
      "claudeCode": {
        "options": {
          "maxThinkingTokens": 4096
        }
      }
    }
  }
}
```

## 什么是 Thinking 模式?

Thinking 模式允许 Claude 在回答前进行更深入的内部推理。这对以下场景特别有用:

- 🧩 **复杂编程问题** - 需要多步骤推理的算法设计
- 🔍 **深度代码分析** - 理解复杂的代码库和架构
- 📋 **任务规划** - 将大任务分解为可执行的步骤
- 🎯 **高质量代码生成** - 生成更健壮、更优雅的代码

## Token 数量建议

| 使用场景 | 推荐值 | 响应时间 | 成本 |
|---------|--------|---------|------|
| 快速响应 | 不设置 | 快 | 低 |
| 一般编程任务 | 4096 | 中等 | 中等 |
| 复杂问题分析 | 8000 | 较慢 | 较高 |
| 极端复杂推理 | 16000 | 慢 | 高 |

## 支持的模型

✅ 支持 Thinking 模式的模型:
- `claude-sonnet-4-20250514` (推荐)
- `claude-opus-4-20250514`
- `claude-sonnet-4-5-20250514`

❌ 不支持的模型:
- `claude-3-5-sonnet-20241022` (旧版本)
- `claude-3-5-haiku-20241022`

## 完整配置示例

```bash
#!/bin/bash

# API 配置
export ANTHROPIC_API_KEY="sk-ant-..."
export ANTHROPIC_BASE_URL="https://api.anthropic.com"

# 模型配置
export ANTHROPIC_MODEL="claude-sonnet-4-20250514"
export ANTHROPIC_SMALL_FAST_MODEL="claude-3-5-haiku-20241022"

# Thinking 模式配置
export MAX_THINKING_TOKENS=4096

# 启动 agent (带诊断日志)
./claude-code-acp-rs --diagnostic -vv
```

## 如何验证 Thinking 模式已启用?

### 1. 查看启动日志

启用诊断模式查看配置:

```bash
./claude-code-acp-rs --diagnostic -vv 2>&1 | grep -i thinking
```

您应该看到类似的日志:

```
Extended thinking mode enabled via meta
max_thinking_tokens=4096
```

### 2. 观察 ACP 通知

当 Claude 进行思考时,会通过 ACP 协议发送 thinking 通知:

```json
{
  "method": "session/notification",
  "params": {
    "sessionId": "xxx",
    "update": {
      "type": "thinking",
      "content": "让我分析一下这个问题的复杂性..."
    }
  }
}
```

## 代码集成示例

如果您基于本项目开发自己的 agent:

```rust
use claude_code_acp::types::AgentConfig;
use claude_code_acp::agent::ClaudeAcpAgent;

fn main() {
    // 创建配置
    let config = AgentConfig {
        base_url: None,
        api_key: Some("your-api-key".to_string()),
        model: Some("claude-sonnet-4-20250514".to_string()),
        small_fast_model: None,
        max_thinking_tokens: Some(4096),  // 启用 thinking 模式
    };

    // 创建 agent
    let agent = ClaudeAcpAgent::with_config(config);
    
    // 运行 agent
    // ...
}
```

## 性能与成本权衡

### 优点
- ✅ 显著提高复杂任务的输出质量
- ✅ 减少需要多轮对话才能解决的问题
- ✅ 提供可见的推理过程,增强可信度

### 缺点
- ⚠️ 增加响应延迟 (取决于 thinking tokens 数量)
- ⚠️ 增加 API 成本 (thinking tokens 计入使用量)
- ⚠️ 不适合需要快速响应的场景

## 故障排除

### 问题: Thinking 模式没有生效

**检查清单**:
1. ✓ 确认使用的模型支持 thinking 功能
2. ✓ 确认环境变量已正确设置
3. ✓ 确认 API key 有权限访问该模型

```bash
# 检查配置
echo "MAX_THINKING_TOKENS: $MAX_THINKING_TOKENS"
echo "ANTHROPIC_MODEL: $ANTHROPIC_MODEL"
echo "ANTHROPIC_API_KEY: ${ANTHROPIC_API_KEY:0:10}..."
```

### 问题: 响应太慢

**解决方案**: 降低 `MAX_THINKING_TOKENS` 值

```bash
# 从 16000 降低到 4096
export MAX_THINKING_TOKENS=4096
```

### 问题: API 成本过高

**解决方案**: 
1. 只在复杂任务时启用 thinking 模式
2. 使用较小的 `MAX_THINKING_TOKENS` 值
3. 考虑使用更便宜的模型处理简单任务

## 最佳实践

1. **按需启用**: 不是所有任务都需要 thinking 模式
2. **合理配置**: 从 4096 开始,根据实际效果调整
3. **监控成本**: 定期检查 API 使用量和成本
4. **测试对比**: 对比启用前后的输出质量差异

## 相关资源

- 📖 [详细文档](./THINKING_MODE.md)
- 🔧 [示例脚本](../examples/thinking_mode.sh)
- 🌐 [Claude 官方文档](https://docs.anthropic.com/en/docs/about-claude/models/extended-thinking-models)
- 💬 [项目 Issues](https://github.com/soddygo/claude-code-acp-rs/issues)

## 技术实现

本功能通过以下方式实现:

1. **环境变量**: `MAX_THINKING_TOKENS` → `AgentConfig.max_thinking_tokens`
2. **ACP Meta**: `_meta.claudeCode.options.maxThinkingTokens` → `NewSessionMeta`
3. **SDK 配置**: `ClaudeAgentOptions.max_thinking_tokens`

代码位置:
- `src/types/config.rs` - 环境变量解析
- `src/types/meta.rs` - ACP meta 字段解析
- `src/session/session.rs` - 应用配置到 SDK

## 更新日志

**v0.1.4** (2026-01-10)
- ✨ 新增 `max_thinking_tokens` 配置支持
- ✨ 支持通过环境变量 `MAX_THINKING_TOKENS` 配置
- ✨ 支持通过 ACP `_meta` 字段配置
- 📝 添加完整的中英文文档
- 🧪 添加单元测试覆盖

---

**问题反馈**: 如有问题,请在 [GitHub Issues](https://github.com/soddygo/claude-code-acp-rs/issues) 提交
