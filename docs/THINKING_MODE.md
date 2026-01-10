# Claude Code ACP - Extended Thinking 模式使用指南

## 概述

Extended Thinking (扩展思考) 模式允许 Claude 在回答问题前进行更深入的推理和思考。通过配置 `max_thinking_tokens`,您可以控制 Claude 用于内部推理的 token 数量,从而提高复杂任务的质量。

## 什么是 Thinking 模式?

Thinking 模式是 Claude 的一个高级功能,它允许模型在生成最终回答之前进行"内部思考"。这些思考过程对用户可见,可以帮助:

- 解决复杂的编程问题
- 进行深度代码分析
- 规划多步骤任务
- 提高代码生成质量

## 配置方式

### 方式 1: 环境变量 (推荐)

最简单的方式是通过环境变量配置:

```bash
# 设置 thinking tokens 上限 (典型值: 4096, 8000, 16000)
export MAX_THINKING_TOKENS=4096

# 启动 agent
./claude-code-acp-rs
```

### 方式 2: 通过 ACP 协议的 _meta 字段

客户端可以在创建会话时通过 `_meta` 字段配置:

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

### 方式 3: 在代码中直接配置

如果您基于本项目开发自己的 agent,可以直接在代码中配置:

```rust
use claude_code_acp::types::AgentConfig;

let config = AgentConfig {
    base_url: None,
    api_key: Some("your-api-key".to_string()),
    model: Some("claude-sonnet-4-20250514".to_string()),
    small_fast_model: None,
    max_thinking_tokens: Some(4096),  // 启用 thinking 模式
};

let agent = ClaudeAcpAgent::with_config(config);
```

## Token 数量建议

根据不同的使用场景,推荐以下配置:

| 场景 | 推荐值 | 说明 |
|------|--------|------|
| 快速响应 | 不设置 | 标准模式,无额外思考时间 |
| 中等复杂度 | 4096 | 适合大多数编程任务 |
| 复杂问题 | 8000 | 需要深度分析的场景 |
| 极端复杂 | 16000 | 最复杂的推理任务 |

**注意**: `max_thinking_tokens` 越大,响应时间越长,API 成本也越高。

## 支持的模型

Extended Thinking 模式需要模型支持。以下模型已知支持此功能:

- `claude-sonnet-4-20250514` (Claude Sonnet 4)
- `claude-opus-4-20250514` (Claude Opus 4)
- 其他支持 thinking 的 Claude 模型

## 完整示例

### 示例 1: 使用环境变量

```bash
# 配置环境变量
export ANTHROPIC_API_KEY="your-api-key"
export ANTHROPIC_MODEL="claude-sonnet-4-20250514"
export MAX_THINKING_TOKENS=4096

# 启动 agent
./claude-code-acp-rs
```

### 示例 2: 通过 ACP 客户端配置

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "session/new",
  "params": {
    "cwd": "/Users/developer/my-project",
    "_meta": {
      "systemPrompt": {
        "append": "请使用中文回答,并进行深入思考。"
      },
      "claudeCode": {
        "options": {
          "maxThinkingTokens": 8000
        }
      }
    }
  }
}
```

### 示例 3: 结合其他配置

```bash
# 完整配置示例
export ANTHROPIC_API_KEY="sk-ant-..."
export ANTHROPIC_MODEL="claude-sonnet-4-20250514"
export ANTHROPIC_BASE_URL="https://api.anthropic.com"
export MAX_THINKING_TOKENS=4096

# 启用诊断日志
./claude-code-acp-rs --diagnostic -vv
```

## 观察 Thinking 过程

当启用 thinking 模式后,Claude 的思考过程会通过 ACP 协议的通知消息返回:

```json
{
  "method": "session/notification",
  "params": {
    "sessionId": "...",
    "update": {
      "type": "thinking",
      "content": "让我分析一下这个问题..."
    }
  }
}
```

## 故障排除

### 问题: Thinking 模式没有生效

**可能原因**:
1. 使用的模型不支持 thinking 功能
2. 环境变量未正确设置
3. API key 没有访问支持 thinking 的模型的权限

**解决方案**:
```bash
# 检查环境变量
echo $MAX_THINKING_TOKENS
echo $ANTHROPIC_MODEL

# 确保使用支持 thinking 的模型
export ANTHROPIC_MODEL="claude-sonnet-4-20250514"
```

### 问题: 响应太慢

**原因**: `max_thinking_tokens` 设置过高

**解决方案**: 降低 token 数量
```bash
# 从 16000 降低到 4096
export MAX_THINKING_TOKENS=4096
```

## 性能考虑

1. **延迟**: Thinking 模式会增加响应延迟,因为模型需要额外的时间进行思考
2. **成本**: Thinking tokens 会计入 API 使用量,增加成本
3. **质量**: 对于复杂任务,额外的思考时间通常能显著提高输出质量

## 相关文档

- [Claude Extended Thinking 官方文档](https://docs.anthropic.com/en/docs/about-claude/models/extended-thinking-models)
- [Claude Code SDK 文档](https://docs.claude.com/zh-CN/docs/claude-code/sdk)
- [ACP 协议规范](../specs/claude-code-acp/)

## 更新日志

- **v0.1.4**: 添加 `max_thinking_tokens` 支持
  - 支持通过环境变量配置
  - 支持通过 ACP `_meta` 字段配置
  - 支持在 `AgentConfig` 中直接配置
