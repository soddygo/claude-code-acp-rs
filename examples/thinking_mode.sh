#!/usr/bin/env bash
# Extended Thinking Mode 示例脚本
# 
# 此脚本演示如何启用 Claude Code ACP 的 Extended Thinking 模式

set -e

# 颜色输出
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}Claude Code ACP - Extended Thinking Mode${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""

# 检查必需的环境变量
if [ -z "$ANTHROPIC_API_KEY" ]; then
    echo -e "${YELLOW}警告: ANTHROPIC_API_KEY 未设置${NC}"
    echo "请设置您的 API key:"
    echo "  export ANTHROPIC_API_KEY='your-api-key'"
    echo ""
fi

# 配置 Thinking 模式
echo -e "${GREEN}配置 Extended Thinking 模式...${NC}"
export MAX_THINKING_TOKENS=4096
export ANTHROPIC_MODEL="claude-sonnet-4-20250514"

echo "  MAX_THINKING_TOKENS: $MAX_THINKING_TOKENS"
echo "  ANTHROPIC_MODEL: $ANTHROPIC_MODEL"
echo ""

# 可选: 配置其他参数
# export ANTHROPIC_BASE_URL="https://api.anthropic.com"
# export ANTHROPIC_SMALL_FAST_MODEL="claude-3-5-haiku-20241022"

# 启动 agent
echo -e "${GREEN}启动 Claude Code ACP Agent...${NC}"
echo "Agent 将使用 Extended Thinking 模式处理复杂任务"
echo ""

# 如果需要诊断日志,取消注释下面这行
# exec ./target/release/claude-code-acp-rs --diagnostic -vv

# 正常启动
exec ./target/release/claude-code-acp-rs
