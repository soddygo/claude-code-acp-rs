# Claude Agent SDK for Rust 功能清单

基于 `vendors/claude-agent-sdk-rs` 项目分析，本文档列出该 SDK 支持的所有功能。

**SDK 信息**:
- 版本: 0.5.0
- Edition: 2024
- Rust 最低版本: 1.90
- 特性: 100% 与 Python SDK 功能对等

---

## 1. 项目结构

```
vendors/claude-agent-sdk-rs/src/
├── lib.rs              # 主库入口，公共 API 导出
├── client.rs           # ClaudeClient 双向流式客户端
├── query.rs            # 简单查询 API 函数
├── errors.rs           # 错误类型定义
├── version.rs          # 版本和兼容性检查
├── types/              # 类型定义
│   ├── config.rs       # 配置选项
│   ├── messages.rs     # 消息类型
│   ├── hooks.rs        # 钩子系统
│   ├── permissions.rs  # 权限管理
│   ├── mcp.rs          # MCP 协议支持
│   └── plugin.rs       # 插件配置
└── internal/           # 内部实现
    ├── client.rs       # 内部客户端实现
    ├── query_full.rs   # 完整查询和双向控制
    ├── message_parser.rs # 消息解析
    └── transport/      # 传输层
        ├── trait_def.rs # Transport trait
        └── subprocess.rs # 子进程传输实现
```

---

## 2. 核心客户端

### 2.1 ClaudeClient

| 功能 | 方法 | 描述 | 源文件位置 |
|------|------|------|------------|
| 创建客户端 | `ClaudeClient::new()` | 创建新客户端实例 | client.rs |
| 带验证创建 | `ClaudeClient::try_new()` | 创建并进行早期验证 | client.rs |
| 连接 | `connect()` | 连接到 Claude CLI | client.rs |
| 断开连接 | `disconnect()` | 断开与 CLI 的连接 | client.rs |
| 简单查询 | `query()` | 发送文本查询 | client.rs |
| 带会话查询 | `query_with_session()` | 使用指定会话 ID 查询 | client.rs |
| 内容查询 | `query_with_content()` | 发送多模态内容 | client.rs |
| 内容+会话查询 | `query_with_content_and_session()` | 多模态内容+会话 | client.rs |
| 新会话 | `new_session()` | 创建新会话 | client.rs |
| 接收消息流 | `receive_messages()` | 持续接收所有消息 | client.rs |
| 接收响应 | `receive_response()` | 接收直到 ResultMessage | client.rs |
| 中断 | `interrupt()` | 中断当前操作 | client.rs |
| 设置权限模式 | `set_permission_mode()` | 动态改变权限模式 | client.rs |
| 设置模型 | `set_model()` | 动态切换模型 | client.rs |
| 回退文件 | `rewind_files()` | 回退文件到指定消息状态 | client.rs |
| 获取服务器信息 | `get_server_info()` | 获取服务器初始化信息 | client.rs |

### 2.2 简单查询 API

| 功能 | 函数 | 返回类型 | 源文件位置 |
|------|------|---------|------------|
| 基础查询 | `query()` | `Vec<Message>` | query.rs |
| 流式查询 | `query_stream()` | `Stream<Result<Message>>` | query.rs |
| 内容查询 | `query_with_content()` | `Vec<Message>` | query.rs |
| 流式内容查询 | `query_stream_with_content()` | `Stream<Result<Message>>` | query.rs |

---

## 3. 消息类型

### 3.1 顶级消息枚举

| 类型 | 描述 | 源文件位置 |
|------|------|------------|
| `Message::Assistant` | Claude 的回复 | types/messages.rs |
| `Message::User` | 用户消息 | types/messages.rs |
| `Message::System` | 系统消息 | types/messages.rs |
| `Message::Result` | 查询完成结果 | types/messages.rs |
| `Message::StreamEvent` | 流式事件 | types/messages.rs |

### 3.2 内容块类型

| 类型 | 描述 | 字段 | 源文件位置 |
|------|------|------|------------|
| `ContentBlock::Text` | 文本块 | `text: String` | types/messages.rs |
| `ContentBlock::Thinking` | 思维块（扩展思维） | `thinking: String, signature: String` | types/messages.rs |
| `ContentBlock::ToolUse` | 工具调用块 | `id, name, input` | types/messages.rs |
| `ContentBlock::ToolResult` | 工具结果块 | `tool_use_id, content, is_error` | types/messages.rs |
| `ContentBlock::Image` | 图像块 | `source: ImageSource` | types/messages.rs |

### 3.3 用户内容块类型

| 类型 | 描述 | 源文件位置 |
|------|------|------------|
| `UserContentBlock::Text` | 用户文本 | types/messages.rs |
| `UserContentBlock::Image` | 用户图像 | types/messages.rs |

### 3.4 消息结构详情

| 结构 | 关键字段 | 源文件位置 |
|------|---------|------------|
| `AssistantMessage` | message, parent_tool_use_id, session_id, uuid | types/messages.rs |
| `UserMessage` | text, content, uuid, parent_tool_use_id | types/messages.rs |
| `SystemMessage` | subtype, cwd, session_id, tools, mcp_servers, model | types/messages.rs |
| `ResultMessage` | duration_ms, is_error, num_turns, session_id, total_cost_usd, usage | types/messages.rs |
| `StreamEvent` | uuid, session_id, event, parent_tool_use_id | types/messages.rs |

---

## 4. 钩子系统 (Hooks)

### 4.1 钩子事件类型

| 事件 | 触发时机 | 输入类型 | 源文件位置 |
|------|---------|---------|------------|
| `PreToolUse` | 工具使用前 | `PreToolUseHookInput` | types/hooks.rs |
| `PostToolUse` | 工具使用后 | `PostToolUseHookInput` | types/hooks.rs |
| `UserPromptSubmit` | 用户提交提示时 | `UserPromptSubmitHookInput` | types/hooks.rs |
| `Stop` | 执行停止时 | `StopHookInput` | types/hooks.rs |
| `SubagentStop` | 子代理停止时 | `SubagentStopHookInput` | types/hooks.rs |
| `PreCompact` | 对话压缩前 | `PreCompactHookInput` | types/hooks.rs |

### 4.2 钩子输入结构

| 结构 | 关键字段 | 源文件位置 |
|------|---------|------------|
| `PreToolUseHookInput` | session_id, transcript_path, cwd, permission_mode, tool_name, tool_input | types/hooks.rs |
| `PostToolUseHookInput` | 同上 + tool_response | types/hooks.rs |
| `UserPromptSubmitHookInput` | session_id, transcript_path, cwd, permission_mode, prompt | types/hooks.rs |

### 4.3 钩子输出类型

| 类型 | 描述 | 源文件位置 |
|------|------|------------|
| `HookJsonOutput::Sync` | 同步输出，阻塞执行 | types/hooks.rs |
| `HookJsonOutput::Async` | 异步输出，后台执行 | types/hooks.rs |

### 4.4 同步钩子输出字段

| 字段 | 类型 | 描述 | 源文件位置 |
|------|------|------|------------|
| `continue_` | `Option<bool>` | 是否继续执行 | types/hooks.rs |
| `suppress_output` | `Option<bool>` | 是否抑制输出 | types/hooks.rs |
| `stop_reason` | `Option<String>` | 停止原因 | types/hooks.rs |
| `decision` | `Option<String>` | 权限决策 | types/hooks.rs |
| `system_message` | `Option<String>` | 系统消息 | types/hooks.rs |
| `reason` | `Option<String>` | 决策原因 | types/hooks.rs |

### 4.5 Hooks Builder API

| 方法 | 描述 | 源文件位置 |
|------|------|------------|
| `Hooks::new()` | 创建新的钩子构造器 | types/hooks.rs |
| `add_pre_tool_use()` | 添加全局 PreToolUse 钩子 | types/hooks.rs |
| `add_pre_tool_use_with_matcher()` | 添加针对特定工具的钩子 | types/hooks.rs |
| `add_post_tool_use()` | 添加 PostToolUse 钩子 | types/hooks.rs |
| `add_post_tool_use_with_matcher()` | 针对特定工具的后处理 | types/hooks.rs |
| `add_user_prompt_submit()` | 添加用户提示钩子 | types/hooks.rs |
| `add_stop()` | 添加停止钩子 | types/hooks.rs |
| `add_subagent_stop()` | 添加子代理停止钩子 | types/hooks.rs |
| `add_pre_compact()` | 添加压缩前钩子 | types/hooks.rs |
| `build()` | 构建最终配置 | types/hooks.rs |

---

## 5. 权限系统

### 5.1 权限模式

| 模式 | 描述 | 源文件位置 |
|------|------|------------|
| `PermissionMode::Default` | 提示用户确认 | types/permissions.rs |
| `PermissionMode::AcceptEdits` | 自动接受编辑 | types/permissions.rs |
| `PermissionMode::Plan` | 计划模式 | types/permissions.rs |
| `PermissionMode::BypassPermissions` | 绕过所有权限检查 | types/permissions.rs |

### 5.2 权限回调

| 类型 | 描述 | 源文件位置 |
|------|------|------------|
| `CanUseToolCallback` | 工具使用权限检查回调 | types/permissions.rs |

### 5.3 权限结果类型

| 类型 | 描述 | 源文件位置 |
|------|------|------------|
| `PermissionResult::Allow` | 允许执行（可带修改后的输入） | types/permissions.rs |
| `PermissionResult::Deny` | 拒绝执行（带消息和中断标志） | types/permissions.rs |

### 5.4 权限更新类型

| 类型 | 描述 | 源文件位置 |
|------|------|------------|
| `AddRules` | 添加权限规则 | types/permissions.rs |
| `ReplaceRules` | 替换权限规则 | types/permissions.rs |
| `RemoveRules` | 移除权限规则 | types/permissions.rs |
| `SetMode` | 设置权限模式 | types/permissions.rs |
| `AddDirectories` | 添加目录 | types/permissions.rs |
| `RemoveDirectories` | 移除目录 | types/permissions.rs |

### 5.5 权限行为

| 行为 | 描述 | 源文件位置 |
|------|------|------------|
| `PermissionBehavior::Allow` | 允许 | types/permissions.rs |
| `PermissionBehavior::Deny` | 拒绝 | types/permissions.rs |
| `PermissionBehavior::Ask` | 询问用户 | types/permissions.rs |

---

## 6. MCP 服务器支持

### 6.1 服务器类型

| 类型 | 配置结构 | 描述 | 源文件位置 |
|------|---------|------|------------|
| Stdio | `McpStdioServerConfig` | 标准 IO 协议 | types/mcp.rs |
| SSE | `McpSseServerConfig` | 服务端事件 | types/mcp.rs |
| HTTP | `McpHttpServerConfig` | HTTP 协议 | types/mcp.rs |
| Sdk | `McpSdkServerConfig` | 进程内 MCP 服务器 | types/mcp.rs |

### 6.2 服务器配置

| 配置 | 字段 | 源文件位置 |
|------|------|------------|
| `McpStdioServerConfig` | command, args, env | types/mcp.rs |
| `McpSseServerConfig` | url, headers | types/mcp.rs |
| `McpHttpServerConfig` | url, headers | types/mcp.rs |
| `McpSdkServerConfig` | name, instance | types/mcp.rs |

### 6.3 SDK MCP 工具

| 类型 | 描述 | 源文件位置 |
|------|------|------------|
| `SdkMcpTool` | SDK MCP 工具定义 | types/mcp.rs |
| `ToolHandler` | 工具处理 trait | types/mcp.rs |
| `ToolResult` | 工具执行结果 | types/mcp.rs |
| `ToolResultContent` | 工具结果内容（Text/Image） | types/mcp.rs |

### 6.4 辅助函数

| 函数 | 描述 | 源文件位置 |
|------|------|------------|
| `create_sdk_mcp_server()` | 创建 SDK MCP 服务器 | types/mcp.rs |

---

## 7. 配置选项 (ClaudeAgentOptions)

### 7.1 基础配置

| 配置项 | 类型 | 描述 | 源文件位置 |
|--------|------|------|------------|
| `model` | `Option<String>` | 模型选择 | types/config.rs |
| `fallback_model` | `Option<String>` | 备用模型 | types/config.rs |
| `max_turns` | `Option<u32>` | 最大回合数 | types/config.rs |
| `max_budget_usd` | `Option<f64>` | USD 预算限制 | types/config.rs |
| `max_thinking_tokens` | `Option<u32>` | 扩展思维令牌限制 | types/config.rs |

### 7.2 工具配置

| 配置项 | 类型 | 描述 | 源文件位置 |
|--------|------|------|------------|
| `tools` | `Option<Tools>` | 工具配置（列表或预设） | types/config.rs |
| `allowed_tools` | `Vec<String>` | 允许的工具列表 | types/config.rs |
| `disallowed_tools` | `Vec<String>` | 禁止的工具列表 | types/config.rs |
| `mcp_servers` | `McpServers` | MCP 服务器配置 | types/config.rs |

### 7.3 系统配置

| 配置项 | 类型 | 描述 | 源文件位置 |
|--------|------|------|------------|
| `system_prompt` | `Option<SystemPrompt>` | 系统提示（文本或预设） | types/config.rs |
| `permission_mode` | `Option<PermissionMode>` | 权限模式 | types/config.rs |
| `permission_prompt_tool_name` | `Option<String>` | 权限提示工具名 | types/config.rs |

### 7.4 会话管理

| 配置项 | 类型 | 描述 | 源文件位置 |
|--------|------|------|------------|
| `resume` | `Option<String>` | 恢复会话 ID | types/config.rs |
| `fork_session` | `bool` | 是否每次重新开始 | types/config.rs |
| `continue_conversation` | `bool` | 是否继续对话 | types/config.rs |

### 7.5 环境配置

| 配置项 | 类型 | 描述 | 源文件位置 |
|--------|------|------|------------|
| `cwd` | `Option<PathBuf>` | 工作目录 | types/config.rs |
| `cli_path` | `Option<PathBuf>` | CLI 路径 | types/config.rs |
| `settings` | `Option<String>` | 设置文件 | types/config.rs |
| `add_dirs` | `Vec<PathBuf>` | 添加目录 | types/config.rs |
| `env` | `HashMap<String, String>` | 环境变量 | types/config.rs |

### 7.6 高级功能

| 配置项 | 类型 | 描述 | 源文件位置 |
|--------|------|------|------------|
| `hooks` | `Option<HashMap<HookEvent, Vec<HookMatcher>>>` | 钩子配置 | types/config.rs |
| `can_use_tool` | `Option<CanUseToolCallback>` | 权限检查回调 | types/config.rs |
| `plugins` | `Vec<SdkPluginConfig>` | 插件配置 | types/config.rs |
| `sandbox` | `Option<SandboxSettings>` | 沙箱配置 | types/config.rs |
| `enable_file_checkpointing` | `bool` | 文件检查点 | types/config.rs |

### 7.7 流式处理

| 配置项 | 类型 | 描述 | 源文件位置 |
|--------|------|------|------------|
| `include_partial_messages` | `bool` | 包含部分消息 | types/config.rs |

### 7.8 输出配置

| 配置项 | 类型 | 描述 | 源文件位置 |
|--------|------|------|------------|
| `output_format` | `Option<serde_json::Value>` | 输出格式（JSON Schema） | types/config.rs |

### 7.9 其他配置

| 配置项 | 类型 | 描述 | 源文件位置 |
|--------|------|------|------------|
| `user` | `Option<String>` | 用户标识 | types/config.rs |
| `setting_sources` | `Option<Vec<SettingSource>>` | 设置来源 | types/config.rs |
| `agents` | `Option<HashMap<String, AgentDefinition>>` | 自定义代理 | types/config.rs |
| `betas` | `Vec<SdkBeta>` | Beta 功能 | types/config.rs |
| `extra_args` | `HashMap<String, Option<String>>` | 额外 CLI 参数 | types/config.rs |
| `max_buffer_size` | `Option<usize>` | 缓冲区大小 | types/config.rs |
| `stderr_callback` | `Option<Arc<dyn Fn(String) + Send + Sync>>` | 标准错误回调 | types/config.rs |

### 7.10 系统提示配置

| 类型 | 描述 | 源文件位置 |
|------|------|------------|
| `SystemPrompt::Text(String)` | 直接文本 | types/config.rs |
| `SystemPrompt::Preset(SystemPromptPreset)` | 预设（含 append） | types/config.rs |

### 7.11 工具配置

| 类型 | 描述 | 源文件位置 |
|------|------|------------|
| `Tools::List(Vec<String>)` | 工具名列表 | types/config.rs |
| `Tools::Preset(ToolsPreset)` | 预设 | types/config.rs |

### 7.12 设置来源

| 来源 | 描述 | 源文件位置 |
|------|------|------------|
| `SettingSource::User` | ~/.claude/settings.json | types/config.rs |
| `SettingSource::Project` | .claude/settings.json | types/config.rs |
| `SettingSource::Local` | .claude/settings.local.json (最高优先级) | types/config.rs |

### 7.13 代理定义

| 字段 | 类型 | 描述 | 源文件位置 |
|------|------|------|------------|
| `description` | `String` | 代理描述 | types/config.rs |
| `prompt` | `String` | 代理提示 | types/config.rs |
| `tools` | `Option<Vec<String>>` | 可用工具 | types/config.rs |
| `model` | `Option<AgentModel>` | 代理模型 | types/config.rs |

### 7.14 代理模型

| 模型 | 描述 | 源文件位置 |
|------|------|------------|
| `AgentModel::Sonnet` | Claude Sonnet | types/config.rs |
| `AgentModel::Opus` | Claude Opus | types/config.rs |
| `AgentModel::Haiku` | Claude Haiku | types/config.rs |
| `AgentModel::Inherit` | 继承父模型 | types/config.rs |

### 7.15 沙箱配置

| 字段 | 类型 | 描述 | 源文件位置 |
|------|------|------|------------|
| `enabled` | `Option<bool>` | 启用沙箱 | types/config.rs |
| `auto_allow_bash_if_sandboxed` | `Option<bool>` | 自动允许沙箱内 bash | types/config.rs |
| `excluded_commands` | `Option<Vec<String>>` | 排除命令 | types/config.rs |
| `allow_unsandboxed_commands` | `Option<bool>` | 允许非沙箱命令 | types/config.rs |
| `network` | `Option<SandboxNetworkConfig>` | 网络配置 | types/config.rs |

### 7.16 Builder API

| 方法 | 描述 | 源文件位置 |
|------|------|------------|
| `ClaudeAgentOptions::builder()` | 创建 Builder | types/config.rs |
| `.model()` | 设置模型 | types/config.rs |
| `.fallback_model()` | 设置备用模型 | types/config.rs |
| `.max_budget_usd()` | 设置预算 | types/config.rs |
| `.max_thinking_tokens()` | 设置思维令牌 | types/config.rs |
| `.max_turns()` | 设置回合数 | types/config.rs |
| `.permission_mode()` | 设置权限模式 | types/config.rs |
| `.plugins()` | 设置插件 | types/config.rs |
| `.build()` | 构建配置 | types/config.rs |

---

## 8. 错误处理

### 8.1 主错误类型

| 错误类型 | 描述 | 源文件位置 |
|---------|------|------------|
| `ClaudeError::Connection` | CLI 连接错误 | errors.rs |
| `ClaudeError::Process` | 进程错误 | errors.rs |
| `ClaudeError::JsonDecode` | JSON 解码错误 | errors.rs |
| `ClaudeError::MessageParse` | 消息解析错误 | errors.rs |
| `ClaudeError::Transport` | 传输错误 | errors.rs |
| `ClaudeError::ControlProtocol` | 控制协议错误 | errors.rs |
| `ClaudeError::InvalidConfig` | 配置无效 | errors.rs |
| `ClaudeError::CliNotFound` | CLI 未找到 | errors.rs |
| `ClaudeError::ImageValidation` | 图像验证错误 | errors.rs |
| `ClaudeError::Io` | IO 错误 | errors.rs |
| `ClaudeError::Other` | 其他错误 | errors.rs |

### 8.2 具体错误结构

| 错误结构 | 字段 | 源文件位置 |
|---------|------|------------|
| `ConnectionError` | message | errors.rs |
| `ProcessError` | message, exit_code, stderr | errors.rs |
| `JsonDecodeError` | message, line | errors.rs |
| `MessageParseError` | message, data | errors.rs |
| `CliNotFoundError` | message, cli_path | errors.rs |
| `ImageValidationError` | message | errors.rs |

### 8.3 图像验证约束

| 约束 | 值 | 源文件位置 |
|------|-----|------------|
| 支持的 MIME 类型 | image/jpeg, image/png, image/gif, image/webp | errors.rs |
| Base64 最大大小 | 15MB（解码后约 20MB） | errors.rs |

---

## 9. 版本管理

| 功能 | 描述 | 源文件位置 |
|------|------|------------|
| `SDK_VERSION` | 当前 SDK 版本 | version.rs |
| `MIN_CLI_VERSION` | 最低 CLI 版本要求 (2.0.0) | version.rs |
| `SKIP_VERSION_CHECK_ENV` | 跳过版本检查的环境变量 | version.rs |
| `parse_version()` | 解析版本字符串 | version.rs |
| `check_version()` | 检查 CLI 版本兼容性 | version.rs |

---

## 10. 特色功能

### 10.1 多模态输入支持

| 功能 | 方法 | 描述 | 源文件位置 |
|------|------|------|------------|
| Base64 图像 | `UserContentBlock::image_base64()` | 从 Base64 创建图像 | types/messages.rs |
| URL 图像 | `UserContentBlock::image_url()` | 从 URL 创建图像 | types/messages.rs |

### 10.2 扩展思维支持

| 功能 | 描述 | 源文件位置 |
|------|------|------------|
| `ThinkingBlock` | 获取模型的推理过程 | types/messages.rs |
| `max_thinking_tokens` | 限制思维令牌数量 | types/config.rs |

### 10.3 成本控制

| 功能 | 描述 | 源文件位置 |
|------|------|------------|
| `max_budget_usd` | USD 预算限制 | types/config.rs |
| `fallback_model` | 主模型失败时的备用模型 | types/config.rs |
| `ResultMessage.total_cost_usd` | 查询成本 | types/messages.rs |

### 10.4 文件检查点

| 功能 | 描述 | 源文件位置 |
|------|------|------------|
| `enable_file_checkpointing` | 启用文件变更跟踪 | types/config.rs |
| `rewind_files()` | 将文件回退到特定用户消息状态 | client.rs |

### 10.5 插件系统

| 功能 | 描述 | 源文件位置 |
|------|------|------------|
| `SdkPluginConfig::local()` | 加载本地插件 | types/plugin.rs |

---

## 功能统计

| 分类 | 功能数 |
|------|--------|
| 客户端方法 | 16 |
| 简单查询 API | 4 |
| 消息类型 | 5 |
| 内容块类型 | 5 |
| 用户内容块类型 | 2 |
| 钩子事件 | 6 |
| 钩子 Builder 方法 | 10 |
| 权限模式 | 4 |
| 权限更新类型 | 6 |
| MCP 服务器类型 | 4 |
| 配置项 | 30+ |
| 错误类型 | 11 |
| **总计** | **100+** |

---

## 依赖关系

```
主要依赖:
- tokio (1.48)         # 异步运行时
- async-trait (0.1)    # 异步 trait
- futures (0.3)        # Future 工具
- serde (1.0)          # 序列化
- serde_json (1.0)     # JSON
- thiserror (2.0)      # 错误处理
- anyhow (1.0)         # 错误处理
- tracing (0.1)        # 日志
- uuid (1.19)          # UUID 生成
- typed-builder        # Builder 模式
```

---

## 更新日志

- 2024-01-07: 初始版本，基于 claude-agent-sdk-rs v0.5.0 分析
