# OpenTelemetry 配置指南

本指南帮助你配置应用，将追踪数据发送到 Jaeger UI。

## 🎯 快速开始

### 1. 启动监控环境

```bash
cd docker
./start-with-otel.sh
```

或手动启动：

```bash
cd docker
docker-compose up -d
```

### 2. 编译应用（启用 otel 功能）

```bash
cd /Users/soddy/RustroverProjects/claude-code-acp-rs
cargo build --features otel
```

### 3. 运行应用

**方式 A：命令行参数**

```bash
./target/debug/claude-code-acp-rs --otel-endpoint http://localhost:4317
```

**方式 B：环境变量**

```bash
export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
./target/debug/claude-code-acp-rs
```

**完整参数示例：**

```bash
./target/debug/claude-code-acp-rs \
  --otel-endpoint http://localhost:4317 \
  --otel-service-name "claude-code-agent" \
  -v
```

### 4. 查看数据

打开浏览器：

- **Jaeger UI**: http://localhost:16686
  - **Search Tab**: 查看 traces（链路追踪）
  - **Monitor Tab**: 查看 SPM 指标（RED metrics）
  
- **Prometheus**: http://localhost:9090
  - 查询 spanmetrics 原始数据

## 📊 架构说明

```
┌─────────────────────┐
│  你的应用            │
│  claude-code-acp-rs │
└──────────┬──────────┘
           │ 发送 traces (OTLP gRPC)
           │ localhost:4317
           ▼
┌─────────────────────┐
│  OpenTelemetry      │
│  Collector          │
│  - 接收 traces      │
│  - 生成 spanmetrics │────► Prometheus (9090)
└──────────┬──────────┘      存储 RED 指标
           │                       │
           │ 转发 traces          │
           ▼                       │
┌─────────────────────┐            │
│  Jaeger (16686)     │◄───────────┘
│  - 存储 traces      │  查询 metrics
│  - 提供 UI         │
│  - SPM Monitor Tab │
└─────────────────────┘
```

## 🔧 配置说明

### CLI 参数

| 参数 | 说明 | 示例 |
|------|------|------|
| `--otel-endpoint` | OTLP endpoint 地址 | `http://localhost:4317` |
| `--otel-service-name` | 服务名称（默认：`claude-code-acp-rs`） | `my-agent` |

### 环境变量

| 变量 | 说明 |
|------|------|
| `OTEL_EXPORTER_OTLP_ENDPOINT` | OTLP endpoint 地址 |

**注意**: CLI 参数优先级高于环境变量。

### 服务端口

| 服务 | 端口 | 用途 |
|------|------|------|
| OTel Collector | **4317** | **OTLP gRPC（应用连接这里）** |
| OTel Collector | 4318 | OTLP HTTP |
| OTel Collector | 8889 | Spanmetrics 指标导出 |
| Jaeger UI | **16686** | **Web 界面** |
| Prometheus | 9090 | Metrics 查询 |

## 🔍 验证配置

### 1. 检查应用日志

启动应用时应该看到：

```
OpenTelemetry enabled: endpoint=http://localhost:4317, service=claude-code-acp-rs
```

### 2. 检查 OTel Collector

```bash
# 查看 OTel Collector 日志
docker-compose logs -f otel-collector

# 检查是否接收到数据
curl http://localhost:8889/metrics | grep traces_span
```

### 3. 检查 Jaeger

访问 http://localhost:16686

- 在 **Search** Tab 选择服务：`claude-code-acp-rs`
- 点击 "Find Traces"
- 应该能看到追踪数据

### 4. 检查 Prometheus

访问 http://localhost:9090，执行查询：

```promql
# 请求总数
traces_span_metrics_calls_total{service_name="claude-code-acp-rs"}

# 请求延迟
traces_span_metrics_duration_milliseconds_bucket{service_name="claude-code-acp-rs"}
```

## 📈 在 Jaeger UI 中查看数据

### Search Tab（追踪查看）

1. 访问 http://localhost:16686
2. 在 **Service** 下拉菜单选择 `claude-code-acp-rs`
3. 点击 **Find Traces**
4. 点击任意 trace 查看详细信息：
   - Span 调用链
   - 时序图
   - Tags 和 Logs

### Monitor Tab（SPM 性能监控）

1. 访问 http://localhost:16686
2. 点击顶部的 **Monitor** tab
3. 选择服务 `claude-code-acp-rs`
4. 查看 RED 指标：
   - **Request Rate**: 请求速率（QPS）
   - **Error Rate**: 错误率
   - **Duration**: 延迟分布（P50, P75, P95）

**注意**：Monitor Tab 需要一定量的数据才能显示，首次运行可能需要等待 1-2 分钟。

## 🐛 故障排查

### 问题 1: Monitor Tab 没有数据

**原因**：
- 还没有发送足够的 traces
- Prometheus 还没抓取到数据

**解决**：
1. 等待 1-2 分钟让数据累积
2. 在 Prometheus UI 查询 `traces_span_metrics_calls_total` 确认有数据
3. 检查 Jaeger 配置中的 Prometheus endpoint

### 问题 2: Search Tab 没有 traces

**原因**：
- 应用没有连接到 OTel Collector
- OTLP endpoint 配置错误

**解决**：
1. 检查应用启动日志是否有 "OpenTelemetry enabled"
2. 确认 `--otel-endpoint` 或环境变量设置正确
3. 检查 Docker 容器是否运行：`docker-compose ps`

### 问题 3: 服务无法启动

**原因**：
- 端口被占用

**解决**：
```bash
# 检查端口占用
lsof -i :4317
lsof -i :16686

# 停止并重启
docker-compose down
docker-compose up -d
```

### 问题 4: 编译错误

**原因**：
- otel feature 未启用

**解决**：
```bash
# 确保使用 --features otel
cargo build --features otel

# 或者在 Cargo.toml 中已经设置 default = ["otel"]
cargo build
```

## 🛠️ 常用命令

```bash
# 启动所有服务
cd docker && docker-compose up -d

# 查看服务状态
docker-compose ps

# 查看日志
docker-compose logs -f

# 查看特定服务日志
docker-compose logs -f otel-collector
docker-compose logs -f jaeger

# 重启服务
docker-compose restart

# 停止服务
docker-compose down

# 停止并清理数据
docker-compose down -v

# 编译应用
cargo build --features otel

# 运行应用
./target/debug/claude-code-acp-rs --otel-endpoint http://localhost:4317

# 查看 OTel metrics
curl http://localhost:8889/metrics

# 检查 Jaeger 健康状态
curl http://localhost:16686
```

## 📚 相关文档

- [OpenTelemetry Rust](https://github.com/open-telemetry/opentelemetry-rust)
- [Jaeger Documentation](https://www.jaegertracing.io/docs/)
- [OTLP Specification](https://opentelemetry.io/docs/specs/otlp/)

## 💡 最佳实践

1. **开发环境**：使用 `--otel-endpoint` 参数便于临时调试
2. **生产环境**：使用环境变量 `OTEL_EXPORTER_OTLP_ENDPOINT`
3. **服务名称**：使用有意义的名称，便于在 Jaeger UI 中识别
4. **日志级别**：开发时使用 `-v` 或 `-vv` 查看详细日志
5. **优雅关闭**：应用会在退出时自动 flush 所有待发送的 spans

## 🎓 示例

### 基础用法

```bash
# 1. 启动监控
cd docker && docker-compose up -d

# 2. 编译
cd .. && cargo build --features otel

# 3. 运行
./target/debug/claude-code-acp-rs --otel-endpoint http://localhost:4317
```

### 完整配置

```bash
# 使用所有选项
./target/debug/claude-code-acp-rs \
  --otel-endpoint http://localhost:4317 \
  --otel-service-name "claude-agent-prod" \
  --diagnostic \
  --log-dir ./logs \
  -vv
```

### 使用环境变量

```bash
# 设置环境变量
export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
export RUST_LOG=debug

# 运行
./target/debug/claude-code-acp-rs
```

---

**提示**: 如果你是第一次使用，建议按照「快速开始」部分的步骤操作。
