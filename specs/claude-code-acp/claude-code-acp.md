# Introduction

## Project Alpha

我想基于 rust语言的 claude code sdk（https://github.com/soddygo/claude-code-agent-sdk.git），来实现ACP协议（agent client protocol）的Agent。
我现在给几个参考的项目：
- https://github.com/zed-industries/claude-code-acp.git zed公司用ts语言实现的ACP协议的agent，使用的claude code ts语言的sdk
本地源码位置： vendors/claude-code-acp，对应功能整理的文档是： specs/claude-code-acp/claude-code-acp-feature.md （以后代码会更新，可能这个文档不一定会更新及时）
- https://github.com/anthropics/claude-agent-sdk-python.git 这个是参考anthropic公司的python语言的sdk，实现的rust语言版本的sdk
本地源码位置： vendors/claude-agent-sdk-python
- https://github.com/agentclientprotocol/rust-sdk.git 这个zed公司，官方的rust语言的sdk，实现ACP协议，需要使用这个库
本地源码位置： vendors/acp-rust-sdk ，对应功能整理的文档是 ：specs/claude-code-acp/claude-agent-sdk-feature.md （以后代码会更新，可能这个文档不一定会更新及时）
- https://github.com/agentclientprotocol/agent-client-protocol.git 这个是zed公司，对ACP协议的Scheme 定义，各个语言的ACP协议的SDK，都是基于这个协议，来进行实现。
本地源码位置： vendors/agent-client-protocol
- https://github.com/soddygo/claude-code-agent-sdk.git 项目源码，我放在了项目下： vendors/claude-code-agent-sdk


先参考理解下我列的几个项目，我想参考 ：https://github.com/zed-industries/claude-code-acp.git 这个zed官方typescript语言实现的claude code的ACP协议agent，实现一个rust语言的ACP协议的agent。

设计原则：
- edition 使用 2024
- 使用 tokio 来作为并发的上下文管理
- 当前项目是 workspace 接口， crates 下有多个模块，另外当前这个工程，我是要发布到crates.io 中央仓库里的，模块暂时先控制只有1个模块

业务要求：
- 可以传递进程环境变量： ANTHROPIC_BASE_URL，ANTHROPIC_AUTH_TOKEN，ANTHROPIC_MODEL，ANTHROPIC_SMALL_FAST_MODEL（可选） 来设置大模型配置，方便配置国内的大模型配置，给claude code使用。
- 记录agent本次任务执行的 token 的使用消耗，如果anthropic 的sdk接口支持的话
- mcp库的使用，可以使用rmcp这个最新的库
- 项目根目的cargo.toml 来统一管理依赖版本
- ACP协议接口，new_session,load_session 入参里有meta字段用于传递额外信息，meta字段里要支持传系统提示词： systemPrompt()， 还有恢复对话的 session_id
系统提示词参考：
```
let mut system_prompt_obj = serde_json::Map::new();
        system_prompt_obj.insert(
            "append".to_string(),
            serde_json::Value::String(system_prompt.clone()),
        );
        meta.insert(
            "systemPrompt".to_string(),
            serde_json::Value::Object(system_prompt_obj),
        );
    }
```
恢复会话session_id 参考：
```
// 添加 session_id 到 resume，用于恢复会话
        // 参考 agent 端的 TypeScript 代码:
        // resume: (params._meta as NewSessionMeta | undefined)?.claudeCode?.options?.resume
        if let Some(ref session_id) = self.resume_session_id {
            // 构建 claudeCode.options.resume 结构
            let mut options = serde_json::Map::new();
            options.insert(
                "resume".to_string(),
                serde_json::Value::String(session_id.clone()),
            );

            let mut claude_code = serde_json::Map::new();
            claude_code.insert("options".to_string(), serde_json::Value::Object(options));

            meta.insert(
                "claudeCode".to_string(),
                serde_json::Value::Object(claude_code),
            );
        }
```
