# Claude Code ACP Agent 功能清单

基于 Zed 官方 TypeScript 实现 (`vendors/claude-code-acp`) 的功能分析，本文档列出所有需要在 Rust 版本中实现的功能。

---

## 1. ACP 协议请求支持

### 1.1 核心生命周期请求

| 功能 | 请求类型 | 状态 | TS 源码位置 |
|------|----------|------|-------------|
| 初始化 | `InitializeRequest` → `InitializeResponse` | [ ] | acp-agent.ts:157-203 |
| 认证 | `AuthenticateRequest` | [ ] | acp-agent.ts:248-250 |
| 新建会话 | `NewSessionRequest` → `NewSessionResponse` | [ ] | acp-agent.ts:205-217 |
| Fork 会话 | `ForkSessionRequest` → `ForkSessionResponse` | [ ] | acp-agent.ts:219-231 |
| 恢复会话 | `ResumeSessionRequest` → `ResumeSessionResponse` | [ ] | acp-agent.ts:233-246 |

### 1.2 会话交互请求

| 功能 | 请求类型 | 状态 | TS 源码位置 |
|------|----------|------|-------------|
| 提示请求 | `PromptRequest` → `PromptResponse` | [ ] | acp-agent.ts:252-410 |
| 取消操作 | `CancelNotification` | [ ] | acp-agent.ts:412-418 |
| 设置会话模型 | `SetSessionModelRequest` → `SetSessionModelResponse` | [ ] | acp-agent.ts:420-427 |
| 设置会话模式 | `SetSessionModeRequest` → `SetSessionModeResponse` | [ ] | acp-agent.ts:429-453 |

### 1.3 文件操作请求

| 功能 | 请求类型 | 状态 | TS 源码位置 |
|------|----------|------|-------------|
| 读文本文件 | `ReadTextFileRequest` → `ReadTextFileResponse` | [ ] | acp-agent.ts:455-458 |
| 写文本文件 | `WriteTextFileRequest` → `WriteTextFileResponse` | [ ] | acp-agent.ts:460-463 |

---

## 2. 会话管理功能

### 2.1 会话生命周期

| 功能 | 描述 | 状态 | TS 源码位置 |
|------|------|------|-------------|
| 会话创建 | 创建新 Claude Code 会话，返回 sessionId、可用模型和模式 | [ ] | acp-agent.ts:593-838 |
| 会话 Fork | 从现有会话创建分支，保留上下文 | [ ] | acp-agent.ts:219-231 |
| 会话恢复 | 使用 `resume` 选项恢复之前的会话状态 | [ ] | acp-agent.ts:233-246, 215 |
| 会话跟踪 | 维护会话字典 `sessions: { [sessionId]: Session }` | [ ] | acp-agent.ts:141-143 |
| 用户输入管理 | 使用 `Pushable<SDKUserMessage>` 处理用户消息流 | [ ] | acp-agent.ts:608 |

### 2.2 会话配置

| 配置项 | 描述 | 状态 | TS 源码位置 |
|--------|------|------|-------------|
| 工作目录 | 通过 `params.cwd` 指定会话工作目录 | [ ] | acp-agent.ts:679 |
| 系统提示词 | 支持预设或自定义系统提示词（append/replace） | [ ] | acp-agent.ts:649-661 |
| MCP 服务器合并 | 合并用户提供和 ACP 内置的 MCP 服务器 | [ ] | acp-agent.ts:615-647 |
| 权限模式 | 默认为 "default" 模式 | [ ] | acp-agent.ts:663 |
| 设置源 | 支持 `["user", "project", "local"]` | [ ] | acp-agent.ts:675 |
| 取消信号 | 支持 `AbortController` 取消操作 | [ ] | acp-agent.ts:767-770 |

---

## 3. 权限系统功能

### 3.1 权限决策流程

| 功能 | 描述 | 状态 | TS 源码位置 |
|------|------|------|-------------|
| 权限检查入口 | `canUseTool()` 回调函数 | [ ] | acp-agent.ts:465-591 |
| ExitPlanMode 特殊处理 | 退出 Plan 模式的特殊权限逻辑 | [ ] | acp-agent.ts:476-525 |
| 自动批准规则 | bypassPermissions 和 acceptEdits 模式下的自动批准 | [ ] | acp-agent.ts:527-538 |
| 权限请求 | 向客户端发起权限请求对话 | [ ] | acp-agent.ts:540-589 |
| 规则应用 | 基于 SettingsManager 的预定义规则 | [ ] | tools.ts:652-696 |

### 3.2 权限模式

| 模式 | 描述 | 状态 | TS 源码位置 |
|------|------|------|-------------|
| default | 标准行为，对危险操作进行提示 | [ ] | acp-agent.ts:801-804 |
| acceptEdits | 自动接受文件编辑操作 | [ ] | acp-agent.ts:806-809 |
| plan | 规划模式，不执行实际工具 | [ ] | acp-agent.ts:811-814 |
| dontAsk | 不提示权限，未预批准的拒绝 | [ ] | acp-agent.ts:816-819 |
| bypassPermissions | 绕过所有权限检查（非 root 用户） | [ ] | acp-agent.ts:822-828 |

### 3.3 设置管理

| 功能 | 描述 | 状态 | TS 源码位置 |
|------|------|------|-------------|
| 多层级加载 | 用户→项目→本地→企业托管设置 | [ ] | settings.ts:244-252 |
| 权限规则格式 | `"ToolName"`, `"ToolName(argument)"`, `"ToolName(prefix:*)"` | [ ] | settings.ts:9-18 |
| 文件监视 | 自动监视所有设置文件的变化 | [ ] | settings.ts:380-412 |
| 规则优先级 | Deny > Allow > Ask | [ ] | settings.ts:451-473 |
| Glob 匹配 | 文件路径使用 glob 模式匹配 | [ ] | settings.ts:137-146 |
| Shell 运算符防护 | 防止通过 `&&`, `||`, `;`, `\|` 等进行命令注入 | [ ] | settings.ts:62-64 |

---

## 4. 工具调用支持

### 4.1 ACP 内置工具（MCP 服务器注册）

| 工具名 | 完整名称 | 功能 | 状态 | TS 源码位置 |
|--------|----------|------|------|-------------|
| Read | mcp__acp__Read | 读取文件内容（支持行偏移和限制） | [ ] | mcp-server.ts:105-209 |
| Write | mcp__acp__Write | 写入文件（需先读取） | [ ] | mcp-server.ts:212-270 |
| Edit | mcp__acp__Edit | 精确字符串替换编辑 | [ ] | mcp-server.ts:272-361 |
| Bash | mcp__acp__Bash | 执行 bash 命令（前台/后台） | [ ] | mcp-server.ts:365-512 |
| BashOutput | mcp__acp__BashOutput | 获取后台 bash 输出 | [ ] | mcp-server.ts:514-570 |
| KillShell | mcp__acp__KillShell | 杀死后台 bash 进程 | [ ] | mcp-server.ts:572-636 |

### 4.2 Claude Code SDK 原生工具（转换支持）

| 工具 | 功能 | 状态 | TS 源码位置 |
|------|------|------|-------------|
| Task | 任务规划 | [ ] | tools.ts:58-71 |
| NotebookRead | 读取 Jupyter 笔记本 | [ ] | tools.ts:73-79 |
| NotebookEdit | 编辑 Jupyter 笔记本 | [ ] | tools.ts:81-95 |
| Bash | 原生 bash（禁用内置时） | [ ] | tools.ts:97-111 |
| BashOutput | 原生输出查询 | [ ] | tools.ts:113-119 |
| KillShell | 原生进程杀死 | [ ] | tools.ts:121-127 |
| LS | 目录列表 | [ ] | tools.ts:167-173 |
| Glob | 文件模式搜索 | [ ] | tools.ts:242-256 |
| Grep | 文本搜索 | [ ] | tools.ts:258-321 |
| WebFetch | 网页抓取 | [ ] | tools.ts:323-336 |
| WebSearch | 网页搜索 | [ ] | tools.ts:338-354 |
| TodoWrite | TODO 列表管理 | [ ] | tools.ts:356-363 |
| ExitPlanMode | 退出规划模式 | [ ] | tools.ts:365-373 |

### 4.3 工具禁用机制

| 功能 | 描述 | 状态 | TS 源码位置 |
|------|------|------|-------------|
| 内置工具禁用 | 通过 `disableBuiltInTools` 元数据禁用 | [ ] | acp-agent.ts:716 |
| 选择性允许 | 根据客户端能力启用工具 | [ ] | acp-agent.ts:719-729 |
| 黑名单管理 | 禁用不必要的工具 | [ ] | acp-agent.ts:732-756 |

---

## 5. 通知类型（Session Updates）

### 5.1 消息通知

| 通知类型 | 描述 | 状态 | TS 源码位置 |
|----------|------|------|-------------|
| agent_message_chunk | 代理文本消息块（流式） | [ ] | acp-agent.ts:1001 |
| user_message_chunk | 用户文本消息块 | [ ] | acp-agent.ts:1001 |
| agent_thought_chunk | 代理内部思考过程 | [ ] | acp-agent.ts:1040 |

### 5.2 工具通知

| 通知类型 | 描述 | 状态 | TS 源码位置 |
|----------|------|------|-------------|
| tool_call | 工具调用开始（待处理） | [ ] | acp-agent.ts:1100 |
| tool_call_update | 工具调用更新/完成 | [ ] | acp-agent.ts:1073, 1133 |

### 5.3 会话管理通知

| 通知类型 | 描述 | 状态 | TS 源码位置 |
|----------|------|------|-------------|
| current_mode_update | 权限模式变化 | [ ] | acp-agent.ts:506 |
| available_commands_update | 可用斜杠命令更新 | [ ] | acp-agent.ts:793 |
| plan | 规划任务更新 | [ ] | acp-agent.ts:1055 |

### 5.4 内容类型

| 内容类型 | 描述 | 状态 | TS 源码位置 |
|----------|------|------|-------------|
| text | 纯文本内容 | [ ] | acp-agent.ts:1003 |
| image | 图像数据（base64 或 URL） | [ ] | acp-agent.ts:1026-1035 |
| diff | 文件差异（工具结果） | [ ] | tools.ts:186-190 |
| content | 通用内容块 | [ ] | tools.ts:65-70 |
| terminal | 终端输出 | [ ] | mcp-server.ts:433 |

---

## 6. Meta 字段支持

### 6.1 新建会话 Meta

| 字段路径 | 类型 | 描述 | 状态 | TS 源码位置 |
|----------|------|------|------|-------------|
| `_meta.claudeCode.options` | Options | Claude Code SDK 选项 | [ ] | acp-agent.ts:96-112 |
| `_meta.claudeCode.options.resume` | string | 恢复的会话 ID | [ ] | acp-agent.ts:215 |
| `_meta.disableBuiltInTools` | boolean | 禁用 ACP 内置工具 | [ ] | acp-agent.ts:640 |
| `_meta.systemPrompt` | string \| {append: string} | 自定义系统提示词 | [ ] | acp-agent.ts:650-661 |

### 6.2 工具调用 Meta

| 字段路径 | 描述 | 状态 | TS 源码位置 |
|----------|------|------|-------------|
| `_meta.claudeCode.toolName` | 执行的原生工具名称 | [ ] | acp-agent.ts:1096 |
| `_meta.claudeCode.toolResponse` | 工具结构化响应 | [ ] | acp-agent.ts:1068 |

---

## 7. MCP Server 功能

### 7.1 MCP 服务器集成

| 功能 | 描述 | 状态 | TS 源码位置 |
|------|------|------|-------------|
| 内置 ACP MCP 服务器 | 注册名为 "acp" 的 MCP 服务器 | [ ] | acp-agent.ts:641-646 |
| 用户 MCP 服务器合并 | 合并来自 ACP 请求的 MCP 服务器配置 | [ ] | acp-agent.ts:616-637 |
| stdio 类型支持 | 支持 stdio 类型的 MCP 服务器 | [ ] | acp-agent.ts:618-628 |
| HTTP 类型支持 | 支持 URL（HTTP）类型的 MCP 服务器 | [ ] | acp-agent.ts:629-635 |
| 条件工具注册 | 根据客户端能力有选择地注册工具 | [ ] | mcp-server.ts:104, 211, 364 |

### 7.2 MCP 工具特性

| 功能 | 描述 | 状态 | TS 源码位置 |
|------|------|------|-------------|
| 内部路径访问 | 代理可访问 ~/.claude 内部文件用于持久化 | [ ] | mcp-server.ts:50-56 |
| 设置文件保护 | 阻止访问 settings.json 等敏感文件 | [ ] | mcp-server.ts:53-54 |
| 行偏移和限制 | 文件读取支持行偏移和行数限制 | [ ] | mcp-server.ts:63-78 |
| 字节限制 | 强制执行文件读取的 50KB 限制 | [ ] | mcp-server.ts:29, 169 |

---

## 8. 特殊功能

### 8.1 后台终端管理

| 功能 | 描述 | 状态 | TS 源码位置 |
|------|------|------|-------------|
| 后台执行 | `run_in_background: true` 启用后台运行 | [ ] | mcp-server.ts:388-392 |
| 后台终端追踪 | 维护 `backgroundTerminals` 字典追踪活跃会话 | [ ] | acp-agent.ts:146 |
| 终端句柄 | 返回终端 ID 用于后续查询和控制 | [ ] | mcp-server.ts:459-493 |
| 输出查询 | BashOutput 工具检索新增输出 | [ ] | mcp-server.ts:533-569 |
| 进程控制 | 支持 kill、timeout、abort 三种终止方式 | [ ] | mcp-server.ts:447-484 |

### 8.2 文件编辑功能

| 功能 | 描述 | 状态 | TS 源码位置 |
|------|------|------|-------------|
| 精确替换 | Edit 工具支持精确字符串替换 | [ ] | mcp-server.ts:330-336 |
| 全局替换 | `replace_all` 参数支持全文替换 | [ ] | mcp-server.ts:294-298 |
| 差异生成 | 使用 diff 库生成补丁输出 | [ ] | mcp-server.ts:338 |
| 唯一性检查 | 确保 old_string 在文件中唯一 | [ ] | mcp-server.ts:286 |

### 8.3 Prompt 处理

| 功能 | 描述 | 状态 | TS 源码位置 |
|------|------|------|-------------|
| 多类型支持 | 支持文本、资源链接、资源、图像 | [ ] | acp-agent.ts:905-981 |
| MCP 命令转换 | `/mcp:server:command` → `/server:command (MCP)` | [ ] | acp-agent.ts:914-918 |
| 上下文包装 | 资源文本包装在 `<context>` 标签中 | [ ] | acp-agent.ts:937-940 |
| URI 格式化 | file:// 和 zed:// URI 格式化为 Markdown 链接 | [ ] | acp-agent.ts:888-903 |

### 8.4 钩子系统

| 钩子 | 描述 | 状态 | TS 源码位置 |
|------|------|------|-------------|
| PreToolUse | 工具使用前权限检查 | [ ] | tools.ts:652-696 |
| PostToolUse | 工具使用后捕获结构化响应 | [ ] | tools.ts:631-645 |
| 钩子回调注册 | 动态注册和清理钩子回调 | [ ] | tools.ts:613-645 |

### 8.5 斜杠命令

| 功能 | 描述 | 状态 | TS 源码位置 |
|------|------|------|-------------|
| 命令发现 | `getAvailableSlashCommands()` 获取所有支持的命令 | [ ] | acp-agent.ts:860-886 |
| 不支持命令过滤 | 过滤 context, cost, login, logout 等命令 | [ ] | acp-agent.ts:861-869 |
| MCP 命令识别 | 标记为 "(MCP)" 的命令处理 | [ ] | acp-agent.ts:876-877 |

### 8.6 模型管理

| 功能 | 描述 | 状态 | TS 源码位置 |
|------|------|------|-------------|
| 模型发现 | `getAvailableModels()` 获取支持的模型列表 | [ ] | acp-agent.ts:841-858 |
| 模型切换 | `setModel()` 在会话中切换模型 | [ ] | acp-agent.ts:426 |
| 初始模型设置 | 使用第一个可用模型作为初始选择 | [ ] | acp-agent.ts:845-846 |

### 8.7 流式处理

| 功能 | 描述 | 状态 | TS 源码位置 |
|------|------|------|-------------|
| 内容块流 | content_block_start/delta/stop 事件处理 | [ ] | acp-agent.ts:1171-1194 |
| 文本增量 | text_delta 块逐步累积文本 | [ ] | acp-agent.ts:1017-1024 |
| 思考块 | thinking_delta 捕获 Claude 的推理过程 | [ ] | acp-agent.ts:1037-1045 |
| NDJSON 流 | 使用 ndJsonStream 进行协议通信 | [ ] | acp-agent.ts:1206 |

---

## 功能统计

| 分类 | 功能数 | 已实现 | 完成率 |
|------|--------|--------|--------|
| ACP 协议请求 | 12 | 0 | 0% |
| 会话管理 | 11 | 0 | 0% |
| 权限系统 | 17 | 0 | 0% |
| 工具调用 | 22 | 0 | 0% |
| 通知类型 | 11 | 0 | 0% |
| Meta 字段 | 6 | 0 | 0% |
| MCP Server | 9 | 0 | 0% |
| 特殊功能 | 26 | 0 | 0% |
| **总计** | **114** | **0** | **0%** |

---

## TS 项目文件结构参考

```
vendors/claude-code-acp/src/
├── index.ts                    # 入口和导出
├── lib.ts                      # 库导出
├── acp-agent.ts                # ACP 代理实现（核心，~1209 行）
├── mcp-server.ts               # MCP 服务器和工具注册（~807 行）
├── tools.ts                    # 工具信息和转换逻辑（~697 行）
├── settings.ts                 # 权限系统和设置管理（~523 行）
├── utils.ts                    # 流转换和工具函数（~172 行）
└── tests/
    ├── acp-agent.test.ts       # 集成测试
    ├── settings.test.ts        # 设置管理测试
    ├── extract-lines.test.ts   # 行提取测试
    └── replace-and-calculate-location.test.ts
```

---

## 依赖关系参考

```
核心依赖:
- @agentclientprotocol/sdk (v0.12.0)       # ACP 协议实现
- @anthropic-ai/claude-agent-sdk (v0.1.73)  # Claude Code SDK
- @modelcontextprotocol/sdk (v1.25.1)      # MCP 协议实现
- diff (v8.0.2)                            # 文件差异生成
- minimatch (v10.1.1)                      # Glob 模式匹配

对应 Rust 依赖:
- sacp                         # ACP 协议 Rust SDK
- claude-agent-sdk-rs          # Claude Code Rust SDK
- rmcp                         # MCP 协议 Rust SDK
- similar / diffy              # 文件差异生成
- glob / globset               # Glob 模式匹配
```

---

## 更新日志

- 2024-01-07: 初始版本，基于 TS 实现分析
