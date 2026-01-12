# 权限系统实现说明

## 概述

本文档说明当前权限系统的状态、存在的问题以及未来需要实现的功能。

## 当前状态

### 已实现

✅ **权限规则基础框架**
- `src/settings/permission_checker.rs` - 权限规则检查器
- `src/permissions/can_use_tool.rs` - SDK `can_use_tool` 回调实现
- `src/session/permission_manager.rs` - 异步权限管理器框架
- `src/hooks/pre_tool_use.rs` - PreToolUse Hook 框架

✅ **权限模式支持**
- `BypassPermissions` - 完全跳过权限检查
- `Plan` - 只读模式，阻止写操作
- `Default/AcceptEdits/DontAsk` - 正常权限检查模式

✅ **设置文件支持**
- `~/.claude/settings.json` 中的权限规则配置
- 支持工具级别的 allow/deny 规则
- 支持通配符模式匹配（如 `Bash:npm *`）

### ⚠️ 当前临时方案

**所有工具在 Default 模式下直接执行，不进行权限检查。**

#### 已临时禁用的代码位置：

1. **工具内部权限检查** (`src/mcp/tools/`)
   - `edit.rs:51-74` - `check_permission()` 方法始终返回 `None`
   - `write.rs:47-55` - `check_permission()` 方法始终返回 `None`
   - `bash.rs:98-106` - `check_permission()` 方法始终返回 `None`

2. **Hook 权限检查** (`src/hooks/pre_tool_use.rs:239-278`)
   - 所有决策路径统一返回 `permission_decision: "allow"`
   - 原有的 `Allow/Deny/Ask` 三路分支已注释

## 核心问题

### 问题 1: SDK 架构限制

**问题描述：** SDK 不为 MCP 工具调用 `can_use_tool` 回调。

#### 技术背景

```
┌─────────────────────────────────────────────────────────────────┐
│  SDK 控制协议                                                  │
├─────────────────────────────────────────────────────────────────┤
│  内置工具              │  MCP 工具 (mcp__acp__*)                 │
│  ─────────            │  ──────────────────────────             │
│  Hook → can_use_tool   │  Hook → 直接执行                        │
│  (权限对话框)           │  (无权限检查)                           │
└─────────────────────────────────────────────────────────────────┘
```

#### 根本原因

1. **SDK 的 `can_use_tool` 是控制请求处理器**，不是 SDK 主动发起的
2. **CLI 决定何时发送 `can_use_tool` 控制请求**
3. **MCP 工具通过 MCP 协议执行**，不经过 SDK 的权限控制流程

#### 证据

```python
# Python SDK - query.py
elif msg_type == "control_request":
    request: SDKControlRequest = message
    if self._tg:
        self._tg.start_soon(self._handle_control_request, request)
    continue
```

```rust
// Rust SDK - query_full.rs
"can_use_tool" => {
    // Handle permission request from CLI
    let result = callback(tool_name.to_string(), tool_input, context).await;
}
```

### 问题 2: Hook 阻塞导致死锁

**问题描述：** 在 Hook 中使用 `connection_cx.send_request().block_task()` 等待权限响应会导致死锁。

#### 尝试过的方案

```rust
// src/hooks/pre_tool_use.rs (已废弃)
PermissionDecision::Ask => {
    // 发送权限请求
    let outcome = session.request_permission(...).await; // ❌ 死锁
    ...
}
```

#### 死锁原因

1. **SDK 事件循环被阻塞** - Hook 在 SDK 的读取循环中执行
2. **SACP 协议需要返回响应** - `send_request().block_task()` 阻塞等待
3. **权限响应无法到达** - 阻塞期间无法读取响应消息

## 参考实现分析

### Python SDK - anyio.TaskGroup 模式

```python
# query.py
async def _read_messages(self):
    async for message in self.transport.read_messages():
        if msg_type == "control_request":
            # 后台任务处理，主循环永不阻塞
            if self._tg:
                self._tg.start_soon(self._handle_control_request, request)
```

**优势：**
- 主读取循环永不阻塞
- 控制请求在后台任务中并发处理
- 使用 `anyio.Event` 协调任务间通信

### Zed 编辑器 - 异步权限模式

```rust
// thread.rs
pub fn authorize(&self, title: impl Into<String>, cx: &mut App) -> Task<Result<()>> {
    let (response_tx, response_rx) = oneshot::channel();

    // 发送权限请求（不阻塞）
    self.stream.0.unbounded_send(Ok(ThreadEvent::ToolCallAuthorization(...)))?;

    // 后台任务等待响应
    cx.background_spawn(async move {
        match response_rx.await {
            Ok(option_id) => { /* 处理用户选择 */ }
            Err(Canceled) => { /* 处理取消 */ }
        }
    }).detach();
}
```

**关键模式：**
1. **One-shot channels** - 请求/响应通信
2. **Unbounded channels** - 事件流永不阻塞
3. **Background spawn** - 长时间等待在后台任务
4. **状态机** - 清晰管理状态转换

### TypeScript Agent - 两路径方案

```typescript
// mcp-server.ts
async function writeTextFile(input: FileWriteInput): Promise<void> {
    if (internalPath(input.file_path)) {
        // 内部路径：直接 fs 操作
        await fs.writeFile(input.file_path, input.content, "utf8");
    } else {
        // 用户文件：通过 agent.writeTextFile()（触发权限检查）
        await agent.writeTextFile({
            sessionId,
            path: input.file_path,
            content: input.content,
        });
    }
}
```

**关键发现：**
- TypeScript 的 MCP 工具使用 `agent.writeTextFile()`
- 这会通过 ACP 协议发送请求到客户端
- 客户端处理权限对话框

## 未来实现方案

### 方案概述

基于 Zed 的异步权限模式，实现无死锁的权限请求流程：

```
┌─────────────────────────────────────────────────────────────┐
│  Hook (立即返回)                                             │
│                                                             │
│  Ask决策 → 发送权限请求事件 → 返回{ continue: true }        │
└─────────────────────────────────────────────────────────────┘
                           ↓
┌─────────────────────────────────────────────────────────────┐
│  权限管理器 (后台任务)                                       │
│                                                             │
│  接收权限请求 → 显示对话框 → 等待用户响应                    │
│                    → 保存规则 → 通知等待的任务                │
└─────────────────────────────────────────────────────────────┘
                           ↓
┌─────────────────────────────────────────────────────────────┐
│  MCP工具执行                                                │
│                                                             │
│  检查权限规则 → 允许则执行 / 拒绝则返回错误                  │
└─────────────────────────────────────────────────────────────┘
```

### 设计原则

1. **无死锁**: Hook 立即返回，不等待权限响应
2. **事件驱动**: 使用通道进行异步通信
3. **状态管理**: 清晰的状态转换
4. **用户友好**: 交互式权限对话框
5. **规则缓存**: "Always Allow" 持久化到设置

### 实现步骤

#### 步骤 1: 完善权限管理器

**文件：** `src/session/permission_manager.rs`

```rust
impl PermissionManager {
    /// 发送权限请求到客户端（不阻塞）
    pub fn request_permission(
        &self,
        tool_name: String,
        tool_input: serde_json::Value,
        tool_call_id: String,
        session_id: String,
    ) -> oneshot::Receiver<PermissionManagerDecision> {
        let (tx, rx) = oneshot::channel();
        let request = PendingPermissionRequest {
            tool_name,
            tool_input,
            tool_call_id,
            session_id,
            response_tx: tx,
        };

        // 发送到后台任务（unbounded channel 永不阻塞）
        drop(self.pending_requests.send(request));
        rx
    }

    /// 后台任务：处理权限请求
    async fn handle_permission_requests(
        mut receiver: UnboundedReceiver<PendingPermissionRequest>,
        connection_cx: JrConnectionCx<AgentToClient>,
    ) {
        while let Some(request) = receiver.recv().await {
            // 发送权限请求到客户端
            // 等待用户响应
            // 发送响应到 response_tx
        }
    }
}
```

#### 步骤 2: 修改 Hook 发送权限请求

**文件：** `src/hooks/pre_tool_use.rs`

```rust
PermissionDecision::Ask => {
    // 发送权限请求到 PermissionManager（不阻塞）
    // Hook 立即返回 { continue: true }
    // 工具执行时会检查权限状态

    tracing::info!(
        tool_name = %tool_name,
        "Tool requires permission - request sent to PermissionManager"
    );

    HookJsonOutput::Sync(SyncHookJsonOutput {
        continue_: Some(true),
        hook_specific_output: Some(HookSpecificOutput::PreToolUse(
            PreToolUseHookSpecificOutput {
                permission_decision: Some("ask".to_string()),
                permission_decision_reason: Some(
                    "Permission request pending".to_string(),
                ),
                updated_input: None,
            },
        )),
        ..Default::default()
    })
}
```

#### 步骤 3: 工具执行前检查权限状态

**文件：** `src/mcp/tools/edit.rs`

```rust
async fn check_permission(
    &self,
    input: &serde_json::Value,
    context: &ToolContext,
) -> Option<ToolResult> {
    let Some(checker) = context.permission_checker.as_ref() else {
        return None;
    };

    let checker = checker.read().await;
    let result: PermissionCheckResult = checker.check_permission("Edit", input);

    match result.decision {
        PermissionDecision::Allow => None,
        PermissionDecision::Deny => Some(ToolResult::error(...)),
        PermissionDecision::Ask => {
            // 检查 PermissionManager 是否有待处理的请求
            // 如果已批准 → None（允许执行）
            // 如果已拒绝 → Some(error)
            // 如果待处理 → Some(error) 提示用户等待
        }
    }
}
```

### 关键文件清单

| 文件 | 修改内容 |
|------|----------|
| `src/hooks/pre_tool_use.rs` | Hook 发送权限请求事件，立即返回 |
| `src/session/permission_manager.rs` | 后台权限管理器，处理权限请求 |
| `src/mcp/tools/edit.rs` | 执行前检查权限状态 |
| `src/mcp/tools/write.rs` | 执行前检查权限状态 |
| `src/mcp/tools/bash.rs` | 执行前检查权限状态 |
| `src/mcp/registry.rs` | ToolContext 添加权限状态检查 |
| `src/mcp/acp_server.rs` | 传递权限管理器到工具 |

## 验证测试

### 测试场景

1. **Default 模式，无规则** → 显示权限对话框
2. **Default 模式，点击 Allow** → 工具执行一次
3. **Default 模式，点击 Always Allow** → 工具执行，规则保存
4. **Default 模式，已有规则** → 工具直接执行，不弹框
5. **Default 模式，点击 Deny** → 工具被拒绝
6. **Plan 模式** → 写操作被阻止
7. **BypassPermissions 模式** → 所有工具直接执行

## 相关文档

- **Plan 文件**: `/Users/soddy/.claude/plans/groovy-painting-truffle.md`
- **Python SDK**: `vendors/claude-agent-sdk-python/src/claude_agent_sdk/_internal/query.py`
- **TypeScript Agent**: `vendors/claude-code-acp/src/acp-agent.ts`
- **Zed 参考**: `vendors/zed/crates/agent/src/thread.rs`

## 临时使用说明

### 当前行为

所有工具在 **Default 模式**下会直接执行，不会显示权限对话框。

### 如果需要限制工具执行

编辑 `~/.claude/settings.json`：

```json
{
  "permissions": {
    "deny": ["Bash", "Edit", "Write"]
  }
}
```

**注意：** 当前临时方案下，即使配置了 deny 规则，工具仍会执行。需要等权限系统完全实现后才会生效。

### 推荐做法

1. **开发环境**: 使用 `BypassPermissions` 模式
2. **生产环境**: 等待权限系统实现完成
3. **测试环境**: 使用 `Plan` 模式（只读）

## 版本历史

| 版本 | 日期 | 说明 |
|------|------|------|
| 0.1.4 | 2025-01-13 | 临时禁用权限检查，添加本文档 |
| 未来 | TBD | 实现完整的交互式权限系统 |
