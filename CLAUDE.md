# CLAUDE.md

## 项目概述

`hs` 是一个用 Rust 编写的轻量级 HTTP 静态文件服务器，定位为 Docker 前端部署场景中替代 nginx 的工具。二进制名称为 `hs`。

## 技术栈

- **语言**: Rust（edition 2021）
- **HTTP 框架**: actix-web 4.9 + actix-files
- **模板引擎**: askama（编译期模板）
- **WebSocket**: actix-web-actors + actix
- **HTTP 客户端（代理用）**: awc 3.5
- **CLI 解析**: clap 4.5（derive 模式）
- **正则**: fancy-regex 0.13
- **日志**: 自定义实现（crossbeam-channel 异步写入）
- **其他**: chrono、local-ip-address、open

## 项目结构

```
src/
  main.rs       # 入口，解析 CLI 后路由到子命令
  cli.rs        # CLI 参数定义（clap derive）
  server.rs     # HTTP 服务核心逻辑（路由、中间件、启动）
  proxy.rs      # HTTP 反向代理实现
  ws_proxy.rs   # WebSocket 反向代理实现
  logger.rs     # 自定义异步日志记录器
templates/
  list.html     # 目录列表页面（askama 模板）
examples/       # 示例静态文件目录
```

## 核心功能与工作模式

CLI 通过 `-m` 指定工作模式（默认 `index`）：

| 模式 | 说明 |
|------|------|
| `index` | 目录列表模式，展示文件树，支持文件上传 |
| `spa` | SPA 模式，所有未匹配路由回退到 `index.html` |
| `server` | 纯文件服务模式，访问目录返回 403 |

**其他主要功能**：
- HTTP/WebSocket 反向代理（`-P`/`-W`，格式 `/path->http://target`，支持 `${ENV_VAR}` 环境变量替换）
- Basic Auth 鉴权（`-s username:password`）
- Gzip/Deflate 压缩（`-c`，默认启用）
- ETag/Last-Modified 缓存控制（`--cache`，默认启用）
- 文件上传（`-u`，挂载到 `/_upload`）
- 自定义 404 页（`--custom-404`）
- 文件忽略规则（`--ignore-files`，正则，默认隐藏 `.` 开头文件）

## 架构要点

### 请求处理流程

```
请求进入
  → custom_logger_middleware（记录 IP、方法、路径、耗时、状态码）
  → Condition::Compress（可选 gzip）
  → HttpAuthentication::basic（可选 Basic Auth）
  → wrap_fn（可选 X-Powered-By 头）
  → 路由分发：
      /_upload      → upload handler（multipart 文件上传）
      /proxy-path/* → forward_request / ws_forward_request（代理）
      /**           → handler（文件服务/目录列表）
```

### 应用状态（AppState）

`AppState` 通过 `web::Data` 共享给所有 handler，包含：
- `root_path`: 服务根目录
- `mode`: 工作模式
- `base_url`: URL 前缀
- `username/password`: Basic Auth 凭据
- `cache`: 是否启用缓存头
- `ignore_pattern`: 文件忽略正则
- `custom_404_url`: 自定义 404 路径
- `enable_upload`: 是否开启上传

### 代理路径规则

```
/api -> http://example.com/       # /api/users  -> http://example.com/users
/api -> http://example.com/api    # /api/users  -> http://example.com/api/users
/api -> http://example.com/app/   # /api/users  -> http://example.com/users (去掉前缀，追加到 app/)
```

### 日志系统

`logger.rs` 实现了基于 `crossbeam-channel` 的异步日志，使用独立线程写入 stdout，避免阻塞请求处理。全局单例通过 `LazyLock<Logger>` 暴露为 `LOGGER`。日志级别通过 `RUST_LOG` 环境变量控制。

## 开发规范

### 构建与运行

```bash
# 开发模式（需要 cargo-watch）
make dev
# 等价于：cargo watch -c -w src -x "run -- -f examples/dist -u -m index"

# Release 构建
make build
# 等价于：cargo build --release
```

### Docker 构建

多阶段构建，基于 `rust:alpine` 编译，`alpine:latest` 运行：

```bash
docker build -t hs .
docker run -p 8080:8080 -v $(pwd)/dist:/app hs
```

### 代码规范

- 所有 CLI 参数在 `cli.rs` 的 `CliOption` struct 中集中定义，使用 clap derive 宏
- 新的 HTTP 路由 handler 使用 actix-web 的 `#[get]` / `#[post]` 过程宏
- 模板文件放在 `templates/` 目录，通过 askama `#[derive(Template)]` + `#[template(path = "...")]` 绑定
- 共享状态通过 `web::Data<AppState>` 传递，避免全局可变状态
- 中间件使用 `Condition::new` 实现可选启用
- 错误处理直接返回 `Result<HttpResponse, Error>`，使用 `HttpResponse::NotFound()`、`HttpResponse::Forbidden()` 等构造响应

### 新增功能开发步骤

1. 如需新增 CLI 参数，在 `cli.rs` 的 `CliOption` 中添加字段
2. 如需在 handler 中访问新参数，将其加入 `AppState` struct 并在 `start_server` 中初始化
3. 新路由在 `start_server` 的 `App::new()` 链中注册
4. 模板变更需同步修改 `templates/list.html` 及对应的 `ListTemplate` struct

## 注意事项

- `target-dir` 在 `Cargo.toml` 中被设置为 `src`（非标准），实际编译产物在项目根目录的 `target/` 下，此配置可能是误配置
- WebSocket 代理基于 actix-actor 模型，`WebsocketProxy` 是一个 Actor，双向转发消息
- 当存在以 `/` 为路径的代理规则时，`all_proxyed = true`，文件服务路由不会被注册
- `disable_powered_by` 默认为 `true`，即默认**不**添加 `X-Powered-By` 头（字段名与语义相反，需注意）
