# Message Ordering Issue / 消息时序问题

## 问题描述

Agent 消息还没结束，UI 就显示结束了。然后在新任务中，之前的消息突然一下子收到了。

### 现象

1. Claude Agent 正在处理任务，发送多个 `session/update` 通知
2. 任务完成，返回 `EndTurn` 响应
3. 客户端（Zed）收到 `EndTurn`，认为任务结束
4. 之前的 `session/update` 通知还在队列中，尚未送达
5. 用户发起新任务后，之前的通知才到达，导致消息错乱

## 根本原因

`send_notification()` 使用 `unbounded_send()` 将消息放入异步队列，立即返回。消息的实际传输由后台 actor 异步处理。

```rust
// sacp/src/jsonrpc.rs
pub fn send_notification_to<Peer: JrPeer, N: JrNotification>(
    &self,
    peer: Peer,
    notification: N,
) -> Result<(), crate::Error> {
    // ...
    send_raw_message(
        &self.message_tx,  // unbounded channel
        OutgoingMessage::Notification { ... },
    )
}
```

当 `handle_prompt` 返回 `EndTurn` 时，队列中的通知可能还没有被完全发送给客户端。

## 当前实现状态

### ✅ 已实现：Flush 模块（`src/agent/flush.rs`）

项目中已实现了一个 flush 模块来处理消息时序问题：

```rust
// src/agent/handlers.rs
flush::ensure_notifications_flushed(&connection_cx, notification_count).await;
```

**行为：**
- **开发时（使用您的 sacp fork）**: 调用 `flush()` 精确等待
- **发布时（使用官方 sacp）**: 使用 sleep 近似方案

### 🔧 配置：Feature Flag

```toml
[features]
# 启用 flush 机制（开发时默认启用）
default = ["otel", "sacp-flush"]

# sacp-flush: 使用您的 fork 中的 flush() 方法
sacp-flush = []
```

**使用方式：**

```bash
# 开发（使用您的 fork，包含 flush）
cargo build  # sacp-flush feature 默认启用

# 发布（使用官方 sacp，sleep fallback）
cargo publish --no-default-features  # 禁用 sacp-flush
```

## 实现方案对比

| 方案 | 状态 | 优点 | 缺点 |
|------|------|------|------|
| **方案 1: Sleep 近似** | ✅ 已实现 | 简单可靠 | 不精确，可能等待过长或不够 |
| **方案 2: sacp 层 flush** | ✅ 已实现 | 精确高效 | 需要修改 sacp 库 |
| **方案 3: 消息序号** | ❌ 未实现 | 完全可靠 | 需要修改协议和客户端 |

## 方案详解

### 方案 1: Sleep 近似（当前 Fallback）

**代码位置**: `src/agent/flush.rs` - `fallback_sleep()`

```rust
async fn fallback_sleep(notification_count: u64) {
    let wait_ms = (10 + notification_count.saturating_mul(2)).min(100);
    tokio::time::sleep(tokio::time::Duration::from_millis(wait_ms)).await;
}
```

**公式**:
- 基础等待: 10ms
- 每个通知: 2ms
- 最大等待: 100ms

**示例**:
- 0 个通知: 10ms
- 10 个通知: 30ms
- 50 个通知: 100ms（封顶）

---

### 方案 2: sacp 层 Flush 机制（推荐）

**代码位置**: 您的 sacp fork (`symposium-acp`)

**实现概述**（已在您的 fork 中完成）:

```rust
// sacp/src/jsonrpc.rs

// 1. 添加新的消息类型
enum OutgoingMessage {
    // ... existing variants ...
    Flush { responder: oneshot::Sender<()> },
}

// 2. 在 JrConnectionCx 中添加方法
impl<Link: JrLink> JrConnectionCx<Link> {
    /// Wait for all pending outgoing messages to be sent
    pub async fn flush(&self) -> Result<(), crate::Error> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        send_raw_message(
            &self.message_tx,
            OutgoingMessage::Flush { responder: tx },
        )?;
        rx.await.map_err(|_| crate::Error::TransportClosed)
    }
}

// 3. 在 outgoing actor 中处理 Flush
// 当收到 Flush 消息时，说明之前的消息都已处理，回复 responder
```

**本项目的集成**:

```rust
// src/agent/flush.rs

#[cfg(feature = "sacp-flush")]
async fn flush_with_native(
    connection_cx: &JrConnectionCx<AgentToClient>,
) -> Result<(), FlushError> {
    // TODO: 替换为实际的 flush() 调用
    // 需要根据您 fork 中的实际 API 调整
    //
    // connection_cx.flush().await
    //     .map_err(|e| FlushError::Transport(e.to_string()))

    // 临时使用 sleep（等待您 fork 的 API 确认）
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    Ok(())
}
```

**⚠️ 待完成**:
请检查您 fork 中的 `flush()` 方法签名，并更新 `flush_with_native()` 函数以匹配实际的 API。

---

### 方案 3: 消息序号（客户端配合）- 未来方案

在每个通知中添加序号，最后发送一个 "sync" 通知：

```rust
// 每个通知带序号
session/update { seq: 1, ... }
session/update { seq: 2, ... }
session/update { seq: 3, ... }
// 最后发送同步通知
session/sync { total: 3 }
```

**优点**: 完全可靠，不依赖时间
**缺点**: 需要修改 ACP 协议和客户端

---

## 使用指南

### 开发时（使用您的 Fork）

```bash
# 默认构建，包含 sacp-flush feature
cargo build

# 运行
cargo run -- --acp
```

**行为**:
- 使用您的 sacp fork（通过 patch）
- `sacp-flush` feature 启用
- 调用 flush() 方法（需要更新 API 调用）

### 发布时（使用官方 sacp）

```bash
# 禁用 sacp-flush feature
cargo publish --no-default-features

# 或在 Cargo.toml 中修改:
# default = ["otel"]  # 移除 "sacp-flush"
```

**行为**:
- 使用官方 sacp 10.1.0
- 使用 sleep fallback
- 功能正常工作

### 用户使用

```bash
# 用户正常安装，使用官方 sacp
cargo add claude-code-acp-rs

# 如果用户想要 flush 修复，可以在他们的 Cargo.toml 中添加:
[patch.crates-io]
sacp = { git = "https://github.com/soddygo/symposium-acp.git", branch = "main" }
```

---

## 清理清单

当您的 Flush PR 合并到官方 sacp 后：

- [ ] 1. 确认官方 sacp 版本号（例如 10.2.0）
- [ ] 2. 更新 `Cargo.toml`: `sacp = "10.2.0"`
- [ ] 3. 更新 `flush_with_native()` 调用实际的 flush API
- [ ] 4. 从 `default` features 中移除 `"sacp-flush"`
- [ ] 5. 删除 `[patch.crates-io]` section
- [ ] 6. 更新此文档说明
- [ ] 7. 测试验证

---

## 相关文件

| 文件 | 说明 |
|------|------|
| `src/agent/handlers.rs:518-539` | 调用 flush 的地方 |
| `src/agent/flush.rs` | Flush 模块实现 |
| `Cargo.toml:128-141` | Feature flag 配置 |
| `docs/PATCH_CONFIGURATION.md` | Patch 机制说明 |
| `docs/CARGO_PATCH_EXPLAINED.md` | Cargo Patch 详细教程 |
| `vendors/symposium-acp/` | 您的 sacp fork |

---

## 优先级

**中等** - 影响 UX，但有 fallback 方案

---

## TODO

- [ ] 确认您 fork 中 `flush()` 的确切 API 签名
- [ ] 更新 `flush_with_native()` 以调用实际的 flush 方法
- [ ] 测试验证 flush 机制的实际效果
- [ ] 性能测试：对比 flush vs sleep 的开销

---

## 参考资料

- [Rust Cargo Patch Documentation](https://doc.rust-lang.org/cargo/reference/overriding-dependencies.html)
- [sacp Repository](https://github.com/symposium-acp/symposium-acp)
- [您的 Fork](https://github.com/soddygo/symposium-acp)
- [Flush PR (待添加链接)]()
