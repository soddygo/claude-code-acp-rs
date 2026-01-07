# Docker 开发环境

本目录包含用于本地开发测试的 Docker 配置文件，支持完整的 Jaeger SPM（Service Performance Monitoring）功能。

## 架构概览

```
┌─────────────────────┐
│  claude-code-acp-rs │
│     (你的应用)       │
└──────────┬──────────┘
           │ OTLP (4317)
           ▼
┌─────────────────────┐
│  OpenTelemetry      │
│  Collector          │──────► Prometheus (9090)
│  - spanmetrics      │        存储 RED 指标
└──────────┬──────────┘              │
           │ OTLP                    │
           ▼                         │
┌─────────────────────┐              │
│  Jaeger (16686)     │◄─────────────┘
│  - Trace 存储       │   查询 metrics
│  - SPM Monitor Tab  │
└─────────────────────┘
```

## 服务说明

| 服务 | 端口 | 用途 |
|------|------|------|
| OTel Collector | 4317 | OTLP gRPC 接收（**你的应用连接这里**） |
| OTel Collector | 4318 | OTLP HTTP 接收 |
| OTel Collector | 8889 | spanmetrics 指标端点 |
| Jaeger UI | 16686 | Web 界面（含 Monitor Tab） |
| Prometheus | 9090 | Prometheus UI |

## 快速启动

```bash
# 进入 docker 目录
cd docker

# 启动所有服务
docker-compose up -d

# 查看状态
docker-compose ps

# 查看日志
docker-compose logs -f
```

## 使用方法

### 1. 启动监控环境

```bash
docker-compose up -d
```

### 2. 运行你的应用（启用 otel feature）

```bash
# 在项目根目录
cargo build --features otel

# 运行并连接 OTel Collector
./target/debug/claude-code-acp-rs --otel-endpoint http://localhost:4317
```

### 3. 查看追踪数据

- **Jaeger UI**: http://localhost:16686
  - **Search** tab: 搜索和查看 traces
  - **Monitor** tab: 查看 RED 指标（SPM 功能）

- **Prometheus UI**: http://localhost:9090
  - 查询原始 spanmetrics 指标

## SPM (Service Performance Monitoring)

SPM 功能在 Jaeger UI 的 **Monitor** tab 中，提供以下 RED 指标：

| 指标 | 说明 |
|------|------|
| **R**equest Rate | 请求速率 (QPS) |
| **E**rror Rate | 错误率 |
| **D**uration | 延迟分布 (P50, P75, P95) |

### 使用 SPM

1. 访问 http://localhost:16686
2. 点击顶部的 **Monitor** tab
3. 选择服务（如 `claude-code-acp-rs`）
4. 查看该服务的 RED 指标和趋势图

### 注意事项

- SPM 需要一定量的 trace 数据才能显示有意义的指标
- 首次启动后，需要等待几分钟让 Prometheus 收集足够的数据
- 如果 Monitor tab 没有数据，检查 Prometheus 是否正常抓取到指标

## 常用命令

```bash
# 启动服务
docker-compose up -d

# 停止服务
docker-compose down

# 重启服务
docker-compose restart

# 查看日志
docker-compose logs -f

# 查看特定服务日志
docker-compose logs -f otel-collector
docker-compose logs -f jaeger
docker-compose logs -f prometheus

# 查看状态
docker-compose ps

# 清理（包括数据卷）
docker-compose down -v
```

## 环境变量配置

你也可以通过环境变量指定 OTLP 端点：

```bash
export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
./target/debug/claude-code-acp-rs
```

## 故障排查

### 1. 检查服务是否运行

```bash
docker-compose ps
curl http://localhost:16686  # Jaeger UI
curl http://localhost:9090   # Prometheus UI
```

### 2. 检查 OTel Collector 是否接收到 traces

```bash
# 查看 OTel Collector 日志
docker-compose logs otel-collector

# 检查 spanmetrics 指标
curl http://localhost:8889/metrics | grep traces_span
```

### 3. 检查 Prometheus 是否抓取到指标

访问 http://localhost:9090，查询：
- `traces_span_metrics_calls_total`
- `traces_span_metrics_duration_milliseconds_bucket`

### 4. 检查 Jaeger SPM 连接

```bash
# 查看 Jaeger 日志
docker-compose logs jaeger | grep -i prometheus
```

### 5. Monitor Tab 没有数据

可能原因：
- 还没有发送足够的 traces
- Prometheus 还没有抓取到数据（等待 15-30 秒）
- 服务名称不匹配

解决方法：
1. 确保应用已发送 traces
2. 在 Prometheus UI 查询 `traces_span_metrics_calls_total` 确认有数据
3. 等待 1-2 分钟让数据累积

## 配置文件说明

| 文件 | 说明 |
|------|------|
| `docker-compose.yml` | Docker 服务编排配置 |
| `otel-collector-config.yaml` | OTel Collector 配置（含 spanmetrics） |
| `prometheus.yml` | Prometheus 抓取配置 |
| `jaeger-config.yaml` | Jaeger v2 配置（含 SPM） |
| `jaeger-ui.json` | Jaeger UI 配置（启用 Monitor tab） |

## 注意事项

- 此配置仅用于本地开发测试，不适用于生产环境
- Jaeger 使用内存存储，重启后 trace 数据会丢失
- Prometheus 数据存储在 Docker volume 中，`docker-compose down -v` 会删除
- 生产环境请使用持久化存储（如 Elasticsearch、Cassandra）
