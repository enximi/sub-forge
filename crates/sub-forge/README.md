# sub-forge

无状态的 Mihomo 订阅转换 HTTP 服务。浏览器将订阅链接、规则模板、转换器地址、转换参数和可选 JavaScript 编码为订阅地址；本服务在客户端访问地址时解码并生成 Mihomo 配置。

本 crate 是 Cargo workspace 中的核心可执行程序。它只提供 HTTP API，不包含前端、用户账户、数据库、用户配置文件或定时刷新任务，也不缓存订阅内容。[静态前端](../../frontend/README.md) 在浏览器本地生成链接，独立部署；命令行、其他前端或 HTTP 客户端同样可以调用核心。

## 运行

以下命令均在[仓库根目录](../..)执行。直接运行需要 Rust 1.97 或更新版本，也可以使用 Docker Compose。执行转换时，由调用方提供一个可公网访问的 SubConverter 兼容接口地址。

程序使用一套环境变量配置：直接运行时通过 `.env` 设置，Docker 运行时通过 `compose.yml` 的 `environment` 设置同名配置。镜像、端口映射和重启策略直接在 Compose 中设置。

### Docker Compose

在 `compose.yml` 的 `environment` 中，将 `SUB_FORGE_PUBLIC_BASE_URL` 改为用户和转换器可以访问的实际 API 地址，按需调整其他参数，然后启动；无需创建 `.env`：

```bash
docker compose pull sub-forge
docker compose up -d --no-build --wait sub-forge
curl --fail --include http://127.0.0.1:8002/health
```

健康接口返回 `204 No Content`。上述命令只启动 API，默认映射到宿主机 `127.0.0.1:8002`。HTTPS、域名和证书由部署环境的反向代理负责。

默认公开使用。个人部署可设置 `SUB_FORGE_ACCESS_TOKEN`，使所有业务接口要求访问 token，详见下文“可选访问控制”。

### Cargo

首次运行时复制环境变量示例：

```bash
cp -n .env.example .env
```

在 `.env` 中填写实际 API 地址和监听地址，已有 `.env` 时直接修改。配置项完整示例见 [`.env.example`](../../.env.example)。然后运行：

```bash
cargo run --locked -p sub-forge
```

用户提交的订阅、规则模板和转换器均须使用公网 HTTP(S) 地址，本机运行不会关闭这项校验。执行完整转换时，将 `SUB_FORGE_PUBLIC_BASE_URL` 设置为转换器和客户端均可访问的公开 API 地址，转换器通过这个地址获取订阅和规则。

## 订阅链接格式

用户在[前端](../../frontend/README.md)填写参数后，浏览器本地生成 `/api/v1/clash/<token>` 地址。核心不提供创建链接接口。生成过程不访问订阅、规则源或转换器，不能证明上游可用；客户端访问链接时才执行公网访问检查、脚本执行与转换。

令牌格式为 `v1.` + `Base64URL(zlib(UTF-8(JSON)))`，Base64URL 不带 `=` 填充。zlib 包含标准头和校验和，不能使用 gzip 或裸 DEFLATE。前端编码实现在 `frontend/generator-worker.js`，核心结构在 `src/tickets/compact.rs`。压缩字节不要求逐字节一致，解码后的内容与默认值必须一致。

订阅用途的最小 JSON：

```json
{
  "k": "f",
  "s": "https://provider.example.com/subscribe?token=your-token",
  "c": "https://converter.example.com/sub",
  "r": "https://example.com/rules.ini"
}
```

`s`、`c`、`r` 分别为订阅来源、转换器和规则模板，均必填。`o` 为转换选项对象，`j` 为脚本对象；全部为默认值或空值时省略整个对象。未知字段会被拒绝。URL 必须是可公网访问、不含用户名和密码的 HTTP(S) 地址，服务不提供默认规则源或转换器。

### 转换选项 `o`

| 字段 | 含义 | v1 固定默认值 |
| --- | --- | --- |
| `e` | 节点名称添加 Emoji | `true` |
| `t` | 启用 TCP Fast Open | `false` |
| `s` | 跳过节点证书验证 | `false` |
| `f` | 过滤目标格式不支持的节点 | `false` |
| `x` | 展开规则集 | `true` |
| `o` | 按节点名称排序 | `false` |
| `n` | 使用新版 Clash 字段名 | `true` |
| `i` | 订阅及 HTTP provider 刷新间隔 | `86400` 秒，范围 `60–604800` |

仅写入不同于默认值的选项。刷新间隔提供给转换器及生成配置，核心不会据此创建定时任务。后处理脚本可以继续修改最终配置。

### 脚本 `j`

`j.s` 对应原始订阅处理，`j.r` 对应规则预处理，`j.c` 对应配置后处理。空脚本省略。前端使用 Terser 去掉注释与多余空白，不改标识符和对象属性，不执行用户代码。

核心在转换过程中编码另外三种专用令牌：`k: "s"` 为订阅源、`k: "r"` 为规则模板、`k: "l"` 为规则资源；它们使用 `s` 保存来源 URL，前两种可使用字符串 `j` 保存对应阶段脚本。这些回调令牌仍由核心生成。

## 处理流程与脚本

```text
浏览器 → 校验参数与语法 → 压缩 JS → 紧凑 JSON → zlib → Base64URL → 订阅地址

GET /api/v1/clash/<token> → 调用转换器
                      ├─ GET /api/v1/subscription/<专用令牌> → 获取订阅 → subscription
                      └─ GET /api/v1/rules/<专用令牌> → 获取 INI → preprocess_rules
                  ← 基础 YAML → postprocess_config → 最终 YAML

客户端刷新 provider → GET /api/v1/subscription/<专用令牌> → 获取订阅 → subscription
```

转换器通过核心提供的专用地址获取订阅和规则。订阅处理与规则预处理分别在对应的取源请求中运行；后处理在转换器返回基础 YAML 后运行。客户端刷新 provider 时也会执行订阅处理，但不会重新执行规则预处理和 YAML 后处理。前端不参与这些请求。

| 脚本执行阶段          | 输入与返回值 | 用途                                   |
| -------------------- | ------------ | -------------------------------------- |
| `subscription`       | 文本 → 文本  | 调整原始订阅内容                       |
| `preprocess_rules`   | 文本 → 文本  | 调整 INI 模板、前置规则和代理组        |
| `postprocess_config` | 对象 → 对象  | 调整生成的 Mihomo 配置，如 DNS、TUN 等 |

每个脚本定义同步函数 `transform(input, context)`：

```javascript
function transform(config, context) {
  config.dns = {
    ...config.dns,
    enable: true,
    nameserver: ["https://cloudflare-dns.com/dns-query"]
  };
  return config;
}
```

脚本在独立的 QuickJS 环境运行，没有网络、文件系统、环境变量、Node.js、npm、模块导入或定时器，不支持 `async` 和 Promise。每次执行限制时间、内存和并发，不共享变量或保存全局状态。浏览器生成地址时只解析语法和压缩代码，保留标识符、属性名和运算逻辑。核心在实际使用时检查 QuickJS 兼容性、`transform` 入口、同步返回值与执行结果。链接只携带压缩后的代码，不保留源码注释和排版，报错位置以压缩后的代码为准；生成成功不代表脚本运行成功。

`context.version` 为 `1`，`context.stage` 为当前脚本字段名，`context.urls` 包含当前阶段相关回调地址，`context.secrets` 始终为空对象。私有模式的回调地址包含访问 token，脚本及外部转换器应可信。

用户脚本通过令牌的 `j` 字段随链接携带，核心不读取前端资源或本地脚本文件。后处理会将 YAML 解析成对象再序列化，原有注释、锚点写法和排版不会保留；最终配置仍需由 Mihomo 客户端验证。

## HTTP 接口

业务接口统一使用 `/api/v1` 前缀，`/health` 独立于业务版本。部署基础地址不包含 `/api/v1`，由前端和核心在生成请求、订阅及回调地址时添加。令牌内的 `v1.` 仅表示编码格式，与 API 版本独立。

| 接口                                | 响应与用途                                    |
| ----------------------------------- | --------------------------------------------- |
| `GET /api/v1/clash/{token}`         | 最终 Mihomo YAML，提供给客户端                |
| `GET /api/v1/subscription/{token}`  | 处理后的原始订阅内容，提供给转换器或 provider |
| `GET /api/v1/rules/{token}`         | 处理后的 INI 模板，提供给转换器               |
| `GET /api/v1/rule-resource/{token}` | 受限转发模板引用的远程规则列表                |
| `GET /health`                       | HTTP 204，仅检查进程存活                      |

各取源接口校验令牌用途，不能用 `/api/v1/clash` 的令牌直接访问其他接口。编码内容可自行修改，用途字段仅用于区分接口，不提供身份认证。核心不提供网页和脚本示例接口；访问 `/` 返回 404。健康检查不会验证转换器或执行整条转换链路。

业务错误返回 JSON，例如 `{"error":"订阅地址无效，请重新生成"}`：

| 状态码 | 含义                                 |
| ------ | ------------------------------------ |
| `400`  | URL、编排参数或脚本校验失败          |
| `401`  | 访问 token 缺失或错误，或配置令牌格式无效、用途不匹配 |
| `429`  | 超过请求频率限制                     |
| `500`  | 服务内部错误                         |
| `502`  | 上游获取、转换或脚本执行失败         |
| `503`  | 服务繁忙、达到并发上限或整体转换超时 |

429 和 503 响应带有 `Retry-After: 60`。核心不接收创建订阅的 JSON 请求体。

### 可选访问控制

`SUB_FORGE_ACCESS_TOKEN` 未设置或为空时，API 公开使用。设置非空值后，所有 `/api/*` 请求必须通过鉴权，再进入配置解码、脚本和上游请求；失败返回 `401`、`WWW-Authenticate: Bearer realm="SubForge"` 及 `{"error":"访问 token 缺失或无效"}`。IP 限流仍覆盖鉴权失败的业务请求。

支持 URL 查询参数 `access_token` 和 `Authorization: Bearer <访问 token>`。同时提供时两者必须均正确；重复 `access_token` 参数、重复 Authorization 请求头或畸形凭据一律拒绝。`/health` 保持无需鉴权，CORS 预检由 CORS 层处理，允许的来源可读取鉴权失败响应。

访问 token 须为 32 至 256 个无空白的可打印 ASCII 字符；无效的服务端配置会拒绝启动。建议用 `openssl rand -hex 32` 生成，不要提交真实值。前端提供可选密码输入框，自动生成 `/api/v1/clash/<配置令牌>?access_token=<访问 token>`；程序调用也可通过 Bearer 请求头传入。

核心生成的订阅源、规则模板、规则资源及 provider URL 自动携带服务端 token，保证转换器回调和客户端刷新可用。参数经过 URL 编码，不写入 `v1.` 配置 JSON。URL 中的 token 可被读取，且具有整个服务的访问权限；只使用可信转换器，不要公开分享链接、配置或日志。Caddy 等反向代理不需要另设相同鉴权，静态前端可以公开托管。

更换 token 后重启服务会使旧 token 立即失效，包含旧 token 的订阅和 provider 回调也需要更新。未配置 token 则恢复公开模式。此机制不提供账户、过期时间或单条订阅撤销；多副本须保持 token 一致。

## 服务端配置

核心从环境变量读取部署配置，直接运行时也会加载 `.env`。用户的订阅、规则、编排参数和脚本通过编码链接提供。

| 环境变量                       | 默认值         | 说明                                       |
| ------------------------------ | -------------- | ------------------------------------------ |
| `SUB_FORGE_PUBLIC_BASE_URL`    | 必填           | 用户和外部转换器可访问的 API 基础地址      |
| `SUB_FORGE_LISTEN_ADDR`        | `0.0.0.0:8080` | 核心监听地址                               |
| `SUB_FORGE_ACCESS_TOKEN`       | 空             | 留空公开使用；非空时启用共享访问 token 鉴权 |
| `SUB_FORGE_ALLOWED_ORIGINS`    | 空             | 允许浏览器跨域调用的来源，逗号分隔         |
| `TRUSTED_PROXY_CIDRS`          | 空             | 可信反向代理网络，逗号分隔                 |
| `REQUESTS_PER_MINUTE`          | `60`           | 单 IP 每分钟动态请求上限，包含转换器回调   |
| `CONVERSION_CONCURRENCY`       | `4`            | 同时转换的上限                             |
| `FETCH_CONCURRENCY`            | `16`           | 上游获取等操作的并发上限                   |
| `SUB_FORGE_TIMEOUT_SECS`       | `60`           | 普通上游请求及整体转换超时，秒             |
| `SUB_FORGE_RULES_TIMEOUT_SECS` | `30`           | 规则请求超时，秒                           |
| `MAX_GENERATED_CONFIG_BYTES`   | `20971520`     | 生成配置大小上限，20 MiB                   |
| `MAX_SUBSCRIPTION_BYTES`       | `10485760`     | 原始订阅大小上限，10 MiB                   |
| `MAX_ACL4SSR_CONFIG_BYTES`     | `2097152`      | 规则模板及规则响应大小上限，2 MiB          |
| `SCRIPT_TIMEOUT_MS`            | `2000`         | 单次脚本执行超时，毫秒                     |
| `SCRIPT_MEMORY_BYTES`          | `134217728`    | 单次脚本运行内存上限，128 MiB              |
| `SCRIPT_MAX_BYTES`             | `1048576`      | 单个脚本源码大小上限；令牌另有脚本总量限制 |
| `SCRIPT_CONCURRENCY`           | `4`            | 同时执行脚本的上限                         |

`.env.example` 只包含程序配置。Docker 使用相同的环境变量名，在 `compose.yml` 的 `environment` 中直接设置，不从 `.env` 引用变量。默认容器监听 `0.0.0.0:8080`，宿主机端口映射为 `127.0.0.1:8002:8080`，发布镜像为 `ghcr.io/enximi/sub-forge:v1.0.0`；调整 Docker 网络和镜像时直接修改 Compose。

浏览器下载配置时，设置 `SUB_FORGE_ALLOWED_ORIGINS=https://ui.example.com`，多个来源用逗号分隔。留空禁用跨域访问；不接受 `*`、凭据、路径、查询参数或片段，不启用凭据型 CORS。浏览器本地生成和复制链接不需要 CORS；Clash / Mihomo 客户端刷新不受浏览器 CORS 限制。

## 无状态与运行边界

订阅来源、转换器选择、编排参数和用户脚本转换为链接专用的紧凑 JSON，经 zlib 压缩和 URL 安全 Base64 编码后封装在生成地址中。服务不保存用户记录，编码本身不需要密钥；私有模式另外校验共享访问 token。使用同一 API 地址且访问 token 配置一致的副本可以处理已有链接，重启不会丢失用户配置。修改配置或脚本后重新生成地址。读取链接时仍校验输入、脚本长度和公网访问边界。

链接编码使用短字段名、按用途区分的结构，并将规则源与转换参数展开，减少嵌套。默认转换参数和空脚本不写入链接，解码时按 `v1` 固定默认值恢复，避免以后业务默认值变化影响已有链接。用户脚本先在浏览器中进行 JS 代码压缩，再写入紧凑 JSON 并整体压缩编码。

当前令牌使用 `v1.` 前缀，不提供加密或签名；拿到链接的人可以解码读取原始地址、参数和全部脚本，也可以修改内容，请勿公开分享。令牌没有自动过期时间或单条撤销功能。只接受当前格式。

进程内的并发许可和 IP 限流计数只用于控制资源占用，不是持久化用户状态。多副本统一限流应由网关负责。反向代理只在确实可信时加入 `TRUSTED_PROXY_CIDRS`，并避免记录完整订阅路径和请求体。

用户提交的订阅、规则模板及其规则列表、外部转换器仅允许公网 HTTP(S) 目标。服务校验 DNS 及跳转目标，阻止本机、私网等受限地址。外部转换器须能访问公开回调地址，也会获得订阅内容。规则模板只支持单个 `[custom]` 区段中的 `ruleset`、`custom_proxy_group`、`enable_rule_generator` 与 `overwrite_original_rules`，不支持转换器自身的脚本、文件导入或自定义基础配置指令。

链接内 JSON 上限为 128 KiB，输入 URL 上限为 2048 字节，三个用户脚本合计最多 64 KiB。压缩编码后的令牌最多 8192 字符，完整 URL 还包含域名和路径，解压输出最多 128 KiB；脚本即使未达到源码上限，也可能超过链接长度限制。客户端和反向代理可能有更低的 URL 长度上限，编码优化不保证任意长度的脚本都可生成可用链接。HTTP 响应设置 `Cache-Control: no-store`，核心不记录完整请求 URI、请求体或用户订阅内容。

## 开发

在仓库根目录检查和构建核心：

```bash
cargo fmt --all --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --release --locked -p sub-forge
```

产物位于 `target/release/sub-forge`。核心依赖根 workspace 的版本和依赖声明，不能只复制本 crate 目录进行构建。

主要模块：

| 模块                                                         | 职责                                           |
| ------------------------------------------------------------ | ---------------------------------------------- |
| `api.rs`、`app.rs`、`config.rs`、`http.rs`                   | 版本前缀与地址、启动、部署配置、路由与请求编排 |
| `network.rs`、`upstream.rs`、`limits.rs`                     | 受限网络访问、响应读取与资源限制               |
| `tickets.rs`、`policy.rs`、`user_scripts.rs`                 | 编码令牌、编排参数与用户脚本结构               |
| `converter.rs`、`subscription.rs`、`rules.rs`、`pipeline.rs` | 转换器调用、取源、规则处理与最终配置加工       |
| `scripts.rs`、`error.rs`                                     | QuickJS 执行及错误处理                         |

验证使用编译、静态检查及隔离的 HTTP 和浏览器交互。[仓库 README](../../README.md) 提供整体部署说明，前端的依赖、运行与部署见[前端 README](../../frontend/README.md)。
