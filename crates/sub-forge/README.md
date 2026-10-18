# sub-forge

使用 SQLite 保存用户参数的 Mihomo 订阅转换 HTTP 服务。前端调用生成接口提交参数，后端返回短链接；客户端访问时读取记录并生成 Mihomo 配置。

本 crate 是 Cargo workspace 中的核心可执行程序。它在同一端口提供内置网页和 HTTP API，使用 SQLite 持久化参数，不包含用户账户或定时刷新任务，也不缓存订阅内容。[前端资源](../../frontend/README.md) 编译时嵌入二进制，运行时无需静态目录或 Nginx；命令行和其他 HTTP 客户端同样可以调用 API。

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

健康接口返回 `204 No Content`。服务默认映射到宿主机 `127.0.0.1:8002`，访问 `/` 即可使用内置网页。HTTPS、域名和证书由部署环境的反向代理负责。

默认公开使用。个人部署可设置 `SUB_FORGE_ACCESS_TOKEN`，使生成接口要求正确的 token，详见下文“可选访问控制”。

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

## 生成订阅链接

`POST /api/v1/subscriptions` 保存参数到 SQLite，返回 HTTP 201 和 `{ "id": "32位随机十六进制ID", "url": "https://sub.example.com/api/v1/clash/<id>" }`。URL 使用服务端 `SUB_FORGE_PUBLIC_BASE_URL`，不是请求 Host 或浏览器输入地址；支持基础地址已有路径前缀。

```bash
curl --fail-with-body https://sub.example.com/api/v1/subscriptions \
  -H 'Content-Type: application/json' \
  -d '{
    "source_url": "https://provider.example.com/subscribe?token=your-token",
    "converter_url": "https://converter.example.com/sub",
    "filename": "my-sub",
    "policy": {
      "rules": { "source_url": "https://example.com/rules.ini" },
      "converter": { "emoji": true, "interval_secs": 86400 }
    },
    "scripts": {
      "subscription": "",
      "preprocess_rules": "",
      "postprocess_config": "function transform(config) { return config; }"
    }
  }'
```

启用生成鉴权时添加 `Authorization: Bearer <管理 token>`。返回的订阅 URL 及后续回调仅包含随机 ID，访问时无需 token。

`source_url`、`converter_url` 和 `policy.rules.source_url` 必填。`policy.converter` 可省略，选项为 `emoji=true`、`tfo=false`、`scv=false`、`fdn=false`、`expand=true`、`sort=false`、`new_name=true`、`interval_secs=86400`；刷新间隔范围 60–604800 秒。`scripts` 和其中各阶段可省略，默认空字符串。`filename` 可省略，默认 `sub-forge`，用于 `/clash` 和 `/subscription` 的 `Content-Disposition` 及转换器 `filename` 参数；只填写不含扩展名的单段名称，最多 64 个字符，不能包含路径分隔符、控制字符或 `: * ? " < > |`，配置下载为 `<filename>.yaml`，原始订阅为 `<filename>.txt`。中文名称会同时输出 ASCII 回退名和 RFC 5987 `filename*=UTF-8''...`。未知字段会被拒绝。

生成时校验 URL 格式、选项、脚本长度和 QuickJS 语法，不抓取上游或执行用户代码。脚本按原文保存，保留注释和排版。三个脚本合计最多 64 KiB，JSON 请求体最多 512 KiB，每个 URL 最多 2048 字节；超过请求体限制也返回 JSON 错误。生成成功不代表上游可用、`transform` 入口有效或实际转换成功。

所有链接使用随机 UUID v4 的 32 字符十六进制形式；每次新建都会生成独立 ID，避免相同参数的用户共用可修改配置。后端为订阅源、规则模板和规则资源生成独立 ID，记录只包含对应阶段需要的参数，重复刷新复用记录。URL 不再携带参数或脚本，不需要压缩编码。

## 载入和更新订阅

`POST /api/v1/subscriptions/load` 接收 `{ "url": "https://sub.example.com/api/v1/clash/<id>" }`，返回 `{ "id": "...", "url": "...", "config": { ... } }`。只解析服务端 `SUB_FORGE_PUBLIC_BASE_URL` 对应的订阅链接，不抓取用户粘贴的 URL；不接受外站链接、回调链接、查询参数或片段。

`PUT /api/v1/subscriptions/<id>` 接收与生成接口相同的完整配置 JSON。保存前校验参数和脚本语法，成功返回 HTTP 200 和 `{ "id": "...", "url": "..." }`，原 ID 和创建时间不变。未知 ID 或回调用途的 ID 返回 401；验证失败不修改已有配置。允许不同订阅保存相同配置，并发编辑采用最后一次成功保存的内容。

以上两个接口使用与生成接口相同的 Bearer token；配置读取不会通过公开 GET 接口暴露。客户端下次更新整份订阅读取新配置，已下载 YAML 中的旧回调仍指向原参数。

数据库 schema 2 移除订阅内容的全局唯一约束，仅不可变回调按内容去重；升级保留所有 ID 和创建时间。旧版因去重而共用一个 ID 的客户端仍共享该配置，另存为新链接可建立独立副本。

## 处理流程与脚本

```text
浏览器 → POST /api/v1/subscriptions → 校验参数与语法 → SQLite → 订阅地址

GET /api/v1/clash/<id> → 调用转换器
                      ├─ GET /api/v1/subscription/<专用 ID> → 获取订阅 → subscription
                      └─ GET /api/v1/rules/<专用 ID> → 获取 INI → preprocess_rules
                  ← 基础 YAML → postprocess_config → 最终 YAML

客户端刷新 provider → GET /api/v1/subscription/<专用 ID> → 获取订阅 → subscription
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

脚本在独立的 QuickJS 环境运行，没有网络、文件系统、环境变量、Node.js、npm、模块导入或定时器，不支持 `async` 和 Promise。每次执行限制时间、内存和并发，不共享变量或保存全局状态。生成接口只编译脚本检查语法，不执行代码；原文保存，不压缩。核心在实际使用时检查 `transform` 入口、同步返回值与执行结果，生成成功不代表脚本运行成功。

`context.version` 为 `1`，`context.stage` 为当前脚本字段名，`context.urls` 包含当前阶段相关回调地址，`context.secrets` 始终为空对象。回调地址不包含管理 token，但可读取对应内容，脚本及外部转换器应可信。

用户脚本与参数一起保存到 SQLite，核心不读取前端资源或本地脚本文件。后处理会将 YAML 解析成对象再序列化，原有注释、锚点写法和排版不会保留；最终配置仍需由 Mihomo 客户端验证。

## HTTP 接口

业务接口统一使用 `/api/v1` 前缀，`/health` 独立于业务版本。部署基础地址不包含 `/api/v1`，由前端和核心在生成请求、订阅及回调地址时添加。ID 不包含用户配置。

| 接口                                | 响应与用途                                    |
| ----------------------------------- | --------------------------------------------- |
| `POST /api/v1/subscriptions` | 保存参数并返回订阅链接，HTTP 201 |
| `GET /api/v1/clash/{id}`         | 最终 Mihomo YAML，提供给客户端                |
| `GET /api/v1/subscription/{id}`  | 处理后的原始订阅内容，提供给转换器或 provider |
| `GET /api/v1/rules/{id}`         | 处理后的 INI 模板，提供给转换器               |
| `GET /api/v1/rule-resource/{id}` | 受限转发模板引用的远程规则列表                |
| `GET /`、`GET /index.html` | 内置网页，HTTP 200 |
| `GET /health`                       | HTTP 204，仅检查进程存活                      |

各取源接口校验 ID 用途，不能用 `/api/v1/clash` 的 ID 直接访问其他接口。各随机 ID 对应固定用途的数据库记录，未知 ID 或用途不匹配返回 401。访问 `/` 或 `/index.html` 返回内置网页；未知静态路径返回 404，不提供脚本示例接口。健康检查不会验证转换器或执行整条转换链路。

业务错误返回 JSON，例如 `{"error":"订阅地址无效，请重新生成"}`：

| 状态码 | 含义                                 |
| ------ | ------------------------------------ |
| `400`  | URL、编排参数或脚本校验失败          |
| `401`  | 管理 token 缺失或错误，或 ID 未知、格式无效、用途不匹配 |
| `429`  | 超过请求频率限制                     |
| `500`  | 服务内部错误                         |
| `502`  | 上游获取、转换或脚本执行失败         |
| `503`  | 服务繁忙、达到并发上限或整体转换超时 |

429 和 503 响应带有 `Retry-After: 60`。创建接口接收 JSON，请求体上限 512 KiB。

### 可选访问控制

`SUB_FORGE_ACCESS_TOKEN` 未设置或为空时，允许公开生成、载入和更新。设置非空值后，生成、载入和更新接口均要求 `Authorization: Bearer <管理 token>`，鉴权成功后才校验和保存参数。缺失、错误、畸形或重复 Authorization 请求头返回 `401`、`WWW-Authenticate: Bearer realm="SubForge"` 及 `{"error":"管理 token 缺失或无效"}`。不接受 URL 查询参数传递生成凭据。

token 须为 32 至 256 个无空白的可打印 ASCII 字符；无效的服务端配置会拒绝启动。建议用 `openssl rand -hex 32` 生成，不要提交真实值。前端根据服务端启动配置决定是否渲染“管理 token”输入框：未设置 token 时隐藏并禁用该字段，设置后才显示；生成、载入和更新请求的 Bearer 头中提交 token，下载不携带凭据。

所有 GET 接口均无需管理 token，订阅、订阅源、规则模板、规则资源及 provider URL 仅包含随机 ID。持有链接即可读取对应内容，请妥善保管链接和配置。管理 token 不会写入数据库记录、链接或脚本上下文。网页、健康检查和 CORS 预检无需鉴权；IP 限流覆盖所有业务请求，包括鉴权失败的生成请求。

更换 token 并重启只影响后续生成、载入和更新，已有链接继续有效。此机制不提供账户、过期时间或单条订阅撤销；多副本须保持管理 token 一致。

## 服务端配置

核心从环境变量读取部署配置，直接运行时也会加载 `.env`。用户的订阅、规则、编排参数和脚本通过生成接口提交并持久化。

| 环境变量                       | 默认值         | 说明                                       |
| ------------------------------ | -------------- | ------------------------------------------ |
| `SUB_FORGE_PUBLIC_BASE_URL`    | 必填           | 用户和外部转换器可访问的 API 基础地址      |
| `SUB_FORGE_DATABASE_PATH` | `data/sub-forge.sqlite3` | SQLite 文件路径，父目录自动创建 |
| `SUB_FORGE_LISTEN_ADDR`        | `0.0.0.0:8080` | 核心监听地址                               |
| `SUB_FORGE_ACCESS_TOKEN`       | 空             | 留空公开管理；非空时生成、载入和更新均验证 token |
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
| `SCRIPT_MAX_BYTES`             | `1048576`      | 单个脚本源码大小上限；三个脚本合计另有 64 KiB 限制 |
| `SCRIPT_CONCURRENCY`           | `4`            | 同时执行脚本的上限                         |

`.env.example` 只包含程序配置。Docker 使用相同的环境变量名，在 `compose.yml` 的 `environment` 中直接设置，不从 `.env` 引用变量。默认容器监听 `0.0.0.0:8080`，宿主机端口映射为 `127.0.0.1:8002:8080`，镜像标签为 `ghcr.io/enximi/sub-forge:v0.1.0`；调整 Docker 网络和镜像时直接修改 Compose。

内置网页使用同源 API，无需设置 CORS 或后端地址。`SUB_FORGE_ALLOWED_ORIGINS` 仅用于其他浏览器调用方，多个来源用逗号分隔；留空禁用跨域调用。不接受 `*`、凭据、路径、查询参数或片段，不启用凭据型 CORS。网页生成、载入和更新请求携带用户填写的 Bearer token，下载无需 token。

前端文件通过 `http/frontend.rs` 中的 `include_str!` 编译进程序，修改后需重新构建。Docker 的构建阶段复制前端源码，运行镜像只携带单个可执行文件。网页 CSP 允许内置脚本、样式和同源 API；API 响应继续禁止执行内容。

## 数据持久化与运行边界

SQLite 使用 WAL，启动时通过 `user_version` 进行事务迁移，保留已有短链接 ID 和创建时间。迁移失败全部回滚，遇到更新版本的数据库则拒绝启动。数据库初始化失败会终止启动。磁盘 I/O 在阻塞线程执行。Compose 用命名卷 `sub-forge-data` 挂载 `/app/data`，镜像预设非 root 用户的目录权限。重启和升级时保留数据卷，删除数据库会使已有链接失效。记录不自动过期，不提供账户或单条撤销接口；修改配置可通过管理接口保留原链接。

数据库包含订阅凭据和脚本原文，请保护文件权限。在线备份用 SQLite `.backup`，或停服后备份整个目录，避免遗漏 WAL。不同数据库的副本不能互相读取链接，推荐单实例和本地数据卷。IP 限流和并发许可仍是进程内状态。

旧 `v1.` 编码链接不再支持，需重新填写参数生成。`0.1.0` 起使用 SQLite 与单体镜像；验证源码可使用 `docker compose -f compose.yml -f compose.dev.yml up -d --build`。

用户提交的订阅、规则模板及其规则列表、外部转换器仅允许公网 HTTP(S) 目标。服务校验 DNS 及跳转目标，阻止本机、私网等受限地址。外部转换器须能访问公开回调地址，也会获得订阅内容。规则模板只支持单个 `[custom]` 区段中的 `ruleset`、`custom_proxy_group`、`enable_rule_generator` 与 `overwrite_original_rules`，不支持转换器自身的脚本、文件导入或自定义基础配置指令。

创建请求 JSON 上限 512 KiB，输入 URL 上限 2048 字节，三个脚本合计最多 64 KiB。链接 ID 固定 32 字符，长度不随脚本增长。HTTP 响应设置 `Cache-Control: no-store`，核心不记录完整 URI、请求体或用户内容。

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
| `app.rs`、`config.rs` | 依赖组装、启动与环境配置 |
| `http/` | 内置页面与资源、路由、请求提取、鉴权、限流、安全响应头与错误映射 |
| `service.rs` | 创建订阅、转换、取源的业务流程及并发限制 |
| `model.rs` | 配置模型、参数校验、四种用途的类型化记录 |
| `links.rs` | ID 对应用途校验和链接组装 |
| `store.rs`、`store/migrations.rs` | SQLite 读写、内容去重、事务迁移和版本检查 |
| `network.rs`、`upstream.rs`、`limits.rs` | 公网边界、响应读取与限流 |
| `converter.rs`、`subscription.rs`、`rules.rs`、`pipeline.rs` | 转换器、取源、规则保护和 YAML 加工 |
| `scripts.rs`、`error.rs` | QuickJS 语法校验、执行限制和业务错误定义 |

验证使用编译、静态检查及隔离的 HTTP 和浏览器交互。[仓库 README](../../README.md) 提供整体部署说明，前端的依赖、运行与部署见[前端 README](../../frontend/README.md)。
