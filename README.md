# sub-forge

无状态的 Mihomo 订阅转换 HTTP 服务。用户在独立网页填写订阅来源、规则模板、转换器地址、转换参数与可选 JavaScript，浏览器本地生成可直接导入客户端的订阅地址。客户端访问地址时，核心读取链接内的配置并执行转换。核心没有网页、数据库、用户配置文件或定时任务，不缓存订阅内容。

## Workspace

```text
crates/
└── sub-forge/                 Rust 核心 HTTP API
    └── src/                   网络校验、编码链接、脚本执行与转换编排
frontend/                      原生 HTML / CSS / JavaScript 单页
├── index.html                 表单与结果区域
├── styles.css                 桌面与移动端样式
├── app.js                     表单交互、复制与下载
└── generator-worker.js        脚本压缩与 v1 链接编码（固定版本 CDN 依赖）
```

Cargo workspace 只包含 Rust 核心 crate；前端是无需构建的静态文件，可独立部署。生成链接无需访问后端，下载配置和客户端刷新时才调用核心。`cargo run` 只启动 API；主 Compose 使用两个发布镜像分别运行 API 和网页，也可单独启动其中一个。核心使用方式见 [sub-forge 核心](crates/sub-forge/README.md)，前端使用方式见 [静态前端](frontend/README.md)。

## 调用方式

打开前端，填写 SubForge 后端基础地址、订阅来源、规则模板和转换器地址，按需调整选项与脚本，再点击“生成订阅地址”。浏览器使用 Terser 去除脚本注释和多余空白，将配置转换为紧凑 JSON，使用 fflate 进行 zlib 压缩，再编码成带 `v1.` 前缀的 URL 安全 Base64 令牌。

生成过程不请求后端、不检查 DNS 或抓取订阅。将结果复制到 Clash / Mihomo 客户端即可使用；实际转换发生在访问生成的地址时。转换器必须是可公网访问的 SubConverter 兼容接口，不提供默认转换器或规则源。

```text
浏览器 → 本地校验、压缩与编码 → /api/v1/clash/<编码令牌>
客户端 → GET /api/v1/clash/<令牌> → 核心 → SubConverter
                                  ├─→ /api/v1/subscription/<专用令牌> → 用户订阅 → 订阅 JS
                                  └─→ /api/v1/rules/<专用令牌> → 规则模板 → 预处理 JS
                        ← 最终 YAML ← 后处理 JS ← 基础 YAML
客户端刷新 provider → /api/v1/subscription/<专用令牌> → 用户订阅 → 订阅 JS
```

转换器通过独立令牌从公开 API 地址获取订阅和模板，必须能访问该地址。客户端刷新 provider 也使用公开 API 地址。所有回调由核心处理，静态前端不参与客户端订阅刷新。

服务检查模板确认标记，防止转换器取模板失败后静默退回默认配置；该临时标记不会进入最终 YAML。原始订阅、规则模板和参数始终由调用方提供。

## API

业务接口统一使用 `/api/v1` 前缀，`/health` 独立于业务版本。部署基础地址不包含 `/api/v1`，由前端和核心在生成请求、订阅及回调地址时添加。令牌内的 `v1.` 仅表示编码格式，与 API 版本独立。

| 接口 | 用途 |
| --- | --- |
| `GET /api/v1/clash/{token}` | 按该链接内的编排设置和代码生成最终 YAML |
| `GET /api/v1/subscription/{token}` | 获取原始内容，执行该用户的订阅处理脚本 |
| `GET /api/v1/rules/{token}` | 获取模板，执行该用户的规则预处理脚本 |
| `GET /api/v1/rule-resource/{token}` | 受限转发模板引用的远程规则列表 |
| `GET /health` | 204，仅检查进程存活 |

核心不提供创建订阅地址的 POST 接口。浏览器编码协议及短字段映射见[核心 README](crates/sub-forge/README.md#订阅链接格式)。

各接口校验令牌用途，格式无效或用途不匹配时返回 401；链接内容可自行解码修改，读取时仍校验输入。输入错误返回 400，超过频率返回 429，达到并发上限返回 503，上游或执行问题返回 502。脚本错误只反馈阶段和截断后的异常信息，不记录脚本内容或敏感 URL。

## 公开或私有部署

`SUB_FORGE_ACCESS_TOKEN` 默认留空，任何人都可以使用 API。私有部署时，在服务端设置随机 token，并在前端“访问 token（可选）”中填写同一值。token 须为 32 至 256 个可打印 ASCII 字符，不含空白；可用 `openssl rand -hex 32` 生成。不要将真实 token 写入提交到仓库的配置或静态前端。

启用后，所有 `/api/*` 请求需要携带 `?access_token=...` 或 `Authorization: Bearer ...`，缺失或错误返回 401。重复查询参数、重复 Authorization 请求头或相互冲突的凭据会被拒绝。`/health` 与 CORS 预检无需 token；静态页面仍可公开访问，但不能绕过后端鉴权。

前端自动把访问 token 放进订阅链接，核心自动为订阅源、规则模板、规则资源与 provider 回调补上 token。该 token 独立于 `v1.` 配置令牌，不改变编码协议。外部转换器、客户端及脚本的相关回调 URL 会包含它，因此只使用可信转换器，不要公开分享链接或生成的配置。

访问 token 是整个服务的共享凭据，不提供用户账户或单条订阅权限。更换 token 并重启服务可使所有旧凭据失效，随后应重新生成订阅并更新客户端配置中的回调地址；多副本须设置相同 token。Docker 在服务器私有 Compose 的 `environment` 中设置，直接运行在环境变量或 `.env` 中设置。

## 编排与用户脚本

用户在表单中填写编排参数和三个阶段的同步 JavaScript，前端按 v1 协议编码进链接；核心解码为类型化的 `Policy` 和 `UserScripts`。下表使用核心内部字段名，链接中的短字段见协议说明。核心固定阶段顺序与网络边界。

| 编排字段 | 类型与用途 | 默认值 |
| --- | --- | --- |
| `rules.source_url` | 规则模板 URL，必填；核心不预设规则源 | 无 |
| `converter.emoji` | 节点名称添加 Emoji 布尔值 | 开启 |
| `converter.tfo` | TCP Fast Open 布尔值 | 关闭 |
| `converter.scv` | 跳过证书验证布尔值 | 关闭 |
| `converter.fdn` | 过滤目标格式不支持的节点布尔值 | 关闭 |
| `converter.expand` | 展开规则集布尔值 | 开启 |
| `converter.sort` | 按节点名称排序布尔值 | 关闭 |
| `converter.new_name` | 使用新版 Clash 字段名布尔值 | 开启 |
| `converter.interval_secs` | 刷新间隔整数 | 86400 秒，范围 60–604800 |

刷新间隔传递给转换器，并应用到本次订阅对应的 HTTP provider；用户后处理脚本仍可覆盖它。本站不会保存任务或定时刷新。

| 脚本 | 输入与返回值 | 执行时机 |
| --- | --- | --- |
| `subscription`，可选 | 原始订阅文本 → 文本 | 转换器取订阅或客户端刷新 provider |
| `preprocess_rules`，可选 | INI 模板文本 → 文本 | 转换器获取规则模板 |
| `postprocess_config`，可选 | 已解析的 YAML 对象 → 对象 | 转换器返回基础 YAML 后 |

所有脚本定义同步的 `transform(input, context)`。可选脚本留空时跳过；每次执行都有独立环境，不共享变量或保留全局状态。阶段顺序由服务固定，用户不需要填写文件路径。

后处理示例：

```javascript
const nameservers = ["https://cloudflare-dns.com/dns-query"];
function transform(config) {
  config.dns = { ...config.dns, enable: true, nameserver: nameservers };
  return config;
}
```

前置规则和代理组由用户的规则预处理脚本提供，DNS、TUN 和 Tailscale 等配置由后处理脚本提供。Tailscale 登录密钥可由用户写进自己的后处理脚本，也会出现在最终 YAML；订阅和规则回调只携带各自阶段的代码，不携带后处理脚本。

前端提供三个空白脚本编辑器，用户直接填写代码。核心只执行编码链接携带的代码。

服务端没有用户数据库、脚本文件、订阅记录或浏览器本地存储。原始链接、转换器选择、编排设置和全部用户脚本转换为链接专用的紧凑 JSON，经 zlib 压缩和 URL 安全 Base64 编码后封装在订阅地址中，编码本身不需要密钥；私有模式另外校验访问 token。链接内容可解码读取和修改，不提供加密或签名，请勿公开分享。更新设置或脚本后重新生成地址；使用同一 API 地址且访问 token 配置一致的副本可以处理已有链接，重启不会丢失用户配置，统一限流放到网关。

链接内部使用短字段名和扁平结构，省略默认转换参数与空脚本；解码时按 `v1` 固定默认值恢复。生成链接前，浏览器使用 Terser 去除脚本注释与多余空白，保留标识符、属性名和运算逻辑，不执行用户代码。语法解析不保证 QuickJS 兼容性、入口存在或执行成功；这些检查由核心在实际使用时完成。链接只携带压缩后的代码，不保留原始排版，报错位置以压缩后的代码为准。

脚本合计最多 64 KiB，编码前 JSON 最大 128 KiB；压缩编码后的令牌最多 8192 字符，解压输出最多 128 KiB。重复较少的长脚本仍可能超过地址长度限制。当前使用带 `v1.` 前缀的编码令牌，只接受当前格式。

### 执行环境

QuickJS 只提供 JavaScript 标准能力，没有文件系统、环境变量、网络、Node.js、npm、模块导入或定时器，不接受 Promise 或 `async transform`。每次执行限制时间、内存、栈和并发。浏览器生成地址时校验参数和脚本语法；核心在实际取源和转换时检查脚本入口、同步返回值与执行结果。依赖实际节点、模板或 YAML 字段的错误在使用链接时反馈。

`context.version` 为 1；`context.stage` 为对应的阶段名称；`context.urls` 提供当前阶段相关的回调地址；`context.secrets` 始终为空对象。不同脚本之间不共享上下文修改。私有模式的回调地址包含访问 token，请使用可信脚本及转换器。

YAML 经对象解析后重新序列化，注释、锚点写法和排版不会保留。程序检查最终顶层对象及大小，不替代 Mihomo 内核验证所有配置字段。转换器的管理员插入节点功能固定关闭。

## 使用发布镜像部署

需要 Docker Compose 和能访问订阅、规则源及转换器的服务器。GitHub Release 发布后，Actions 构建并推送以下 AMD64 / ARM64 镜像：

| 镜像 | 用途 |
| --- | --- |
| `ghcr.io/enximi/sub-forge:v1.0.0` | Rust 核心 API，容器内端口 8080 |
| `ghcr.io/enximi/sub-forge-web:v1.0.0` | 静态网页，容器内端口 8080 |

服务器只需要 `compose.yml` 和私有部署参数，无需 Git、Rust、Node.js 或仓库源码。下载本次发布的 Compose 模板：

```bash
mkdir -p sub-forge && cd sub-forge
curl -fL https://raw.githubusercontent.com/enximi/sub-forge/v1.0.0/compose.yml -o compose.yml
chmod 600 compose.yml
```

在 Compose 的 `environment` 中设置实际 `SUB_FORGE_PUBLIC_BASE_URL`。私有部署填写随机 `SUB_FORGE_ACCESS_TOKEN`，浏览器跨域下载时设置 `SUB_FORGE_ALLOWED_ORIGINS`。这些值只保存在自己的服务器上，不提交到公开仓库。然后拉取并启动：

```bash
docker compose pull
docker compose up -d --no-build --wait
```

API 绑定宿主机 `127.0.0.1:8002`，网页绑定 `127.0.0.1:8081`。已有 HTTPS 代理可将 `/api/*`、`/health` 转发到 API，其他路径转发到网页；同域部署仍需在页面填写公开后端地址。若只需要 API，使用 `docker compose up -d --no-build --wait sub-forge`。也可将前端四个静态文件独立托管。

HTTPS、域名和证书由外部反向代理负责。API 的 `GET /health` 返回 204，网页首页返回 200。核心的 `/` 返回 404。仅将真实代理网络填入 `TRUSTED_PROXY_CIDRS`，避免代理记录完整订阅路径、查询参数或请求体。

升级时将 Compose 中两个镜像标签改成新的发布版本，再执行 `pull` 和 `up -d --no-build --wait`。回滚改回上一版本标签，或使用已记录的镜像摘要。生产部署建议使用明确版本或摘要；`latest` 仅用于跟随最近发布。

调用方自行选择可公网访问的 SubConverter 兼容服务，转换器必须能访问本站回调地址。Sub-Store 是可选上游，配置见 `compose.sub-store.yml`，不属于核心的运行依赖。其持久化数据独立于核心，用户提交的订阅地址必须公网可达。

服务器上更新部署文件时保留原所有者和权限；普通管理账号持有部署配置，含 token 的文件使用 600。不要用 root 创建临时文件替换后遗失原文件所有者。

## 独立前端与跨域调用

前端直接部署 `frontend/` 下的 HTML、CSS 和 JS 静态文件，不需要 npm 安装或构建。页面中的“后端地址”可随时修改；部署者也可修改 `index.html` 中 `sub-forge-api-base-url` 元标签来设置默认值。基础地址可包含部署路径前缀，不包含 `/api/v1`、查询参数或片段。

本地预览：

```bash
python3 -m http.server 8081 --bind 127.0.0.1 --directory frontend
```

打开 `http://127.0.0.1:8081/`。生成和复制链接不需要后端在线，也不需要 CORS。使用“下载配置”时，页面会跨域 GET 生成的地址，需要将核心的 `SUB_FORGE_ALLOWED_ORIGINS` 设置为前端来源，例如 `http://127.0.0.1:8081` 或 `https://ui.example.com`；多个来源用逗号分隔，留空禁用跨域访问。不接受 `*`、凭据、路径或查询参数，不使用会话 Cookie。HTTPS 页面下载时也需使用 HTTPS 后端。

客户端刷新订阅直接访问核心，前端关闭或重新部署不会影响已有链接。公网转换仍要求核心配置正确的 `SUB_FORGE_PUBLIC_BASE_URL`，供转换器获取回调内容。

## 本机运行

核心从环境变量读取部署配置，直接运行时会加载根目录 `.env`。首次使用复制示例：

```bash
cp -n .env.example .env
```

在 `.env` 中填写实际 API 地址和程序监听地址，然后运行：

```bash
cargo run --locked -p sub-forge
```

订阅链接必须包含可公网访问的转换器地址。本机运行也会校验用户输入 URL 的公网访问边界；执行完整转换时，将 `SUB_FORGE_PUBLIC_BASE_URL` 设置为转换器和客户端均可访问的公开 API 地址。

## 无状态范围

用户订阅、参数和脚本只在当前请求内使用，服务不保存用户记录。编码链接携带重建配置所需内容；使用同一 API 地址且访问 token 配置一致的另一副本可以直接处理原链接。链接没有自动过期时间，也没有单条撤销功能，请像原始订阅一样妥善保管。

进程保留临时并发许可和 IP 限流计数，用于限制公开服务的资源占用；它们不是用户配置或持久化状态。多副本统一限流放到网关。静态前端同样不保存用户输入，不使用浏览器本地存储。

## 公网运行边界

用户的订阅来源和外部转换器只允许 HTTP/HTTPS 公网目标，禁止 URL 用户名/密码、本机、私网、链路本地、组播和保留地址。DNS 结果中任意地址不合规即拒绝。每次请求重新解析 DNS、绑定已校验的 IP；跳转逐次检查，最多 5 次；不使用进程代理环境变量绕过检查。用户规则模板及其远程规则列表同样通过受限网络转发。模板只支持单个 `[custom]` 区段中的 `ruleset`、`custom_proxy_group`、`enable_rule_generator` 与 `overwrite_original_rules`，不支持转换器自身的脚本指令、文件导入或自定义基础配置；用户 JS 在本站 QuickJS 中执行。

默认单 IP 每分钟最多 60 次动态请求（包括转换器回调），最多同时转换 4 份配置、取上游内容 16 次、执行 JS 4 次。限流状态仅在当前进程内保存；多副本部署需在网关统一限流。部署者还应按机器容量调整限制，并管理转换器的资源与网络权限。

| 参数 | 默认值 |
| --- | --- |
| `SUB_FORGE_TIMEOUT_SECS` | 60 秒 |
| `SUB_FORGE_RULES_TIMEOUT_SECS` | 30 秒 |
| `REQUESTS_PER_MINUTE` | 60 |
| `CONVERSION_CONCURRENCY` | 4 |
| `FETCH_CONCURRENCY` | 16 |
| `MAX_GENERATED_CONFIG_BYTES` | 20 MiB |
| `MAX_SUBSCRIPTION_BYTES` | 10 MiB |
| `MAX_ACL4SSR_CONFIG_BYTES` | 2 MiB |
| `SCRIPT_TIMEOUT_MS` | 2000 毫秒 |
| `SCRIPT_MEMORY_BYTES` | 128 MiB |
| `SCRIPT_MAX_BYTES` | 1 MiB |
| `SCRIPT_CONCURRENCY` | 4 |

链接内 JSON 上限 128 KiB，每个输入 URL 上限 2048 字节。响应逐块读取并检查大小。HTTP 响应禁用缓存、跨站引用和内容类型嗅探。服务不记录完整请求 URI、请求体或用户订阅。健康接口不会执行转换，不能代表整条链路正常。

## 开发与发布

```bash
cargo fmt --all --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --locked -p sub-forge
```

本地需要构建容器时使用 `docker compose -f compose.yml -f compose.dev.yml up -d --build`。API 和网页分别打包，API 以非 root 用户运行，镜像不含源码与私人配置；网页镜像仅包含静态资源。构建上下文采用允许列表，排除 `.env` 和本地部署资料。

发布步骤：

1. 更新 Cargo workspace 版本及 Compose、文档中的镜像标签，并更新 `Cargo.lock`。
2. 完成上述静态检查，提交后创建相同版本的 `vX.Y.Z` tag。
3. 在 GitHub 发布对应 Release，自动触发 `Publish container images` 工作流。
4. 等待 Rust / JavaScript 检查、AMD64 / ARM64 构建和多架构清单发布全部成功，再部署版本镜像。

工作流使用 GitHub 提供的 `GITHUB_TOKEN` 推送 GHCR，无需配置个人访问令牌。首次发布后，将两个容器包的可见性设为 Public，使服务器无需登录就能拉取；后续版本沿用包的可见性。也可在 Actions 中选择现有版本 tag 手动重跑。公开镜像附带源码地址、版本、提交 SHA、构建来源和 SBOM。

## 许可证

项目代码采用 [MIT License](LICENSE)。ACL4SSR 模板与用户选择的转换器属于独立上游项目，使用时遵守各自许可证。
