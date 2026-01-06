# Introduction

## Project Alpha

我想基于 rust语言的 claude code sdk（https://github.com/soddygo/claude-agent-sdk-rs.git），来实现ACP协议（agent client protocol）的Agent。
我现在给几个参考的项目：
- https://github.com/zed-industries/claude-code-acp.git zed公司用ts语言实现的ACP协议的agent，使用的claude code ts语言的sdk
本地源码位置： vendors/claude-code-acp
- https://github.com/anthropics/claude-agent-sdk-python.git 这个是参考anthropic公司的python语言的sdk，实现的rust语言版本的sdk
本地源码位置： vendors/claude-agent-sdk-python
- https://github.com/agentclientprotocol/rust-sdk.git 这个zed公司，官方的rust语言的sdk，实现ACP协议，需要使用这个库
本地源码位置： vendors/acp-rust-sdk 
- https://github.com/agentclientprotocol/agent-client-protocol.git 这个是zed公司，对ACP协议的Scheme 定义，各个语言的ACP协议的SDK，都是基于这个协议，来进行实现。
本地源码位置： vendors/agent-client-protocol


先参考理解下我列的几个项目，我想参考 ：https://github.com/zed-industries/claude-code-acp.git 这个zed官方typescript语言实现的claude code的ACP协议agent，实现一个rust语言的ACP协议的agent。

设计原则：
- edition 使用 2024
- 使用 tokio 来作为并发的上下文管理
- 当前项目是 workspace 接口， crates 下有多个模块

业务要求：
- 可以传递进程环境变量： ANTHROPIC_BASE_URL，ANTHROPIC_AUTH_TOKEN，ANTHROPIC_MODEL，ANTHROPIC_SMALL_FAST_MODEL（可选） 来设置大模型配置，方便配置国内的大模型配置，给claude code使用。
- 记录agent本次任务执行的 token 的使用消耗，如果anthropic 的sdk接口支持的话
- mcp库的使用，可以使用rmcp这个最新的库
- 项目根目的cargo.toml 来统一管理依赖版本
