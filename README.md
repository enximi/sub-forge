# sub-forge

Mihomo 订阅转换 HTTP 服务。用户在内置网页填写订阅来源、规则模板、转换器地址、转换参数与可选 JavaScript，由后端校验并保存到 SQLite，返回固定长度随机 ID 的订阅链接。客户端访问时，核心从数据库读取配置并执行转换；不缓存上游订阅内容。

## Workspace

```text
crates/
└── sub-forge/                 Rust 单体程序：网页与 HTTP API
    └── src/                   网络校验、SQLite 存储、脚本执行与转换编排
frontend/                      原生 HTML / CSS / JavaScript 单页
├── index.html                 表单与结果区域
├── styles.css                 桌面与移动端样式
├── app.js                     页面状态、表单交互、复制与下载
├── api.js                     HTTP 请求、错误与超时处理
└── rules-picker.js            规则选择器与键盘交互
```

Cargo workspace 包含一个 Rust 可执行程序。前端 HTML、CSS、JavaScript 在编译时嵌入二进制，启动后同一端口提供网页、API 和健康检查。只需部署一个程序或一个容器，不需要 Nginx、Node.js 或独立静态目录。程序说明见 [sub-forge](crates/sub-forge/README.md)，页面源码说明见 [前端](frontend/README.md)。

## 架构

```text
内置网页 / HTTP 路由 / 鉴权 / 响应
          ↓
SubscriptionService：创建、转换、订阅与规则取源
          ├── Links → Store → SQLite
          ├── SubscriptionSource / RuleSource → PublicNetwork
          └── ConfigPipeline → Converter / ScriptEngine
```

- `model.rs` 定义完整订阅、转换参数、用户脚本及四种链接记录；必需字段由类型保证。
- `http/` 只处理路由、请求提取、鉴权、限流及响应映射，不直接操作数据库或执行脚本。
- `service.rs` 负责业务流程、并发和整体转换超时；`links.rs` 统一管理链接用途和地址。
- `store.rs` 负责 SQLite 读写及去重，`store/migrations.rs` 负责事务迁移和数据库版本检查。
- 取源、规则保护、转换器调用和脚本执行分别封装，原有公网网络边界保持一致。
- 前端使用原生 ES Modules：页面状态、API 请求、规则选择器独立，不引入框架或构建工具。

## 调用方式

打开服务首页，填写订阅来源、规则模板和转换器地址，按需调整选项与脚本，再点击“生成订阅地址”。前端通过 `POST /api/v1/subscriptions` 提交参数，核心验证 URL 格式、转换选项、脚本大小和 QuickJS 语法，保存配置后返回订阅 URL。

生成阶段不抓取上游，也不执行用户脚本；实际公网访问检查和转换发生在访问订阅时。生成成功不代表上游可用或脚本运行成功。转换器必须是公网可访问的 SubConverter 兼容接口。

```text
浏览器 → POST /api/v1/subscriptions → SQLite → /api/v1/clash/<随机 ID>
客户端 → GET /api/v1/clash/<ID> → 核心 → SubConverter
                                  ├─→ /api/v1/subscription/<专用 ID> → 用户订阅 → 订阅 JS
                                  └─→ /api/v1/rules/<专用 ID> → 规则模板 → 预处理 JS
                        ← 最终 YAML ← 后处理 JS ← 基础 YAML
客户端刷新 provider → /api/v1/subscription/<专用 ID> → 用户订阅 → 订阅 JS
```

转换器通过独立 ID从公开 API 地址获取订阅和模板，必须能访问该地址。客户端刷新 provider 也使用公开 API 地址。所有回调由核心处理，静态前端不参与客户端订阅刷新。

服务检查模板确认标记，防止转换器取模板失败后静默退回默认配置；该临时标记不会进入最终 YAML。原始订阅、规则模板和参数始终由调用方提供。

## API

业务接口统一使用 `/api/v1` 前缀，`/health` 独立于业务版本。部署基础地址不包含 `/api/v1`，由前端和核心在生成请求、订阅及回调地址时添加。所有链接 ID 都是 32 个十六进制字符，不携带用户参数。

| 接口 | 用途 |
| --- | --- |
| `POST /api/v1/subscriptions` | 保存参数并返回 `{id, url}`，HTTP 201 |
| `POST /api/v1/subscriptions/load` | 以本站订阅链接载入完整配置，需要与生成相同的 token |
| `PUT /api/v1/subscriptions/{id}` | 校验并替换配置，保留原 ID 和订阅链接 |
| `GET /api/v1/clash/{id}` | 读取 SQLite 中的编排设置和代码生成最终 YAML |
| `GET /api/v1/subscription/{id}` | 获取原始内容，执行该用户的订阅处理脚本 |
| `GET /api/v1/rules/{id}` | 获取模板，执行该用户的规则预处理脚本 |
| `GET /api/v1/rule-resource/{id}` | 受限转发模板引用的远程规则列表 |
| `GET /health` | 204，仅检查进程存活 |

生成接口的 JSON 字段和调用示例见[核心 README](crates/sub-forge/README.md#生成订阅链接)。

各接口从 SQLite 查找记录并校验用途；未知 ID、旧编码令牌或用途不匹配返回 401。输入错误返回 400，超过频率返回 429，达到并发上限返回 503，上游或执行问题返回 502。脚本错误只反馈阶段和截断后的异常信息，不记录脚本内容或敏感 URL。

## 公开或私有部署

`SUB_FORGE_ACCESS_TOKEN` 用于控制订阅管理权限。设置后，生成、载入和更新接口都需要 `Authorization: Bearer <管理 token>`；缺失、错误或重复 Authorization 请求头返回 401，不接受 URL 查询参数传递凭据。前端仅在设置 token 时显示“管理 token”输入框，填写同一值即可。配置留空时允许公开生成、载入和更新，页面不显示该输入框。

token 须为 32 至 256 个可打印 ASCII 字符，不含空白，可用 `openssl rand -hex 32` 生成。不要将真实 token 提交到仓库或写入静态前端。Docker 在服务器私有 Compose 的 `environment` 中设置，直接运行在环境变量或 `.env` 中设置。

订阅下载、订阅源、规则模板、规则资源和 provider 回调均无需管理 token；生成的 URL 只包含随机 ID。网页、健康检查和 CORS 预检也无需 token。所有业务请求仍受 IP 限流约束。

短链接本身是访问凭据，持有链接即可读取对应内容，请妥善保管。管理 token 不会写入链接、数据库记录或脚本上下文。更换 token 并重启只影响后续生成、载入和更新，已有链接继续有效；此机制不提供用户账户、过期时间或单条订阅撤销。

## 编辑已有订阅

切换到“更新订阅”，填写管理 token 和本站生成的订阅链接，点击“载入配置”，编辑后选择“保存到原链接”。也可切换到“新建订阅”，将当前配置另存为独立链接，保留原订阅不变。新建始终创建独立链接；更新模式必须先载入配置，才能保存到原链接。

更新前先校验完整参数及脚本语法；校验失败时原配置不变。成功保存后，客户端下次更新整份订阅即可读取新配置。旧配置内的 provider 回调保持原有取源参数，单独刷新节点不会载入新的规则或后处理设置。多个编辑同时保存时，最后成功保存的配置生效。

SQLite 自动升级时保留已有 ID 和创建时间。旧版曾合并为同一 ID 的订阅仍是同一条记录，所有持有该链接的客户端都会受更新影响；需要独立修改时先另存为新链接。

## 编排与用户脚本

用户在表单中填写编排参数和三个阶段的同步 JavaScript，后端以类型化的 `Policy` 和 `UserScripts` 保存。下表为生成接口 `policy` 内的字段。核心固定阶段顺序与网络边界。

| 编排字段 | 类型与用途 | 默认值 |
| --- | --- | --- |
| `rules.source_url` | 规则模板 URL，必填；核心不预设规则源 | 无 |
| `converter.provider` | 使用 Provider 模式布尔值 | 关闭 |
| `converter.emoji` | 节点名称添加 Emoji 布尔值 | 开启 |
| `converter.tfo` | TCP Fast Open 布尔值 | 关闭 |
| `converter.scv` | 跳过证书验证布尔值 | 关闭 |
| `converter.fdn` | 过滤目标格式不支持的节点布尔值 | 关闭 |
| `converter.expand` | 展开规则集布尔值 | 开启 |
| `converter.sort` | 按节点名称排序布尔值 | 关闭 |
| `converter.new_name` | 使用新版 Clash 字段名布尔值 | 开启 |
| `converter.interval_secs` | 刷新间隔整数 | 86400 秒，范围 60–604800 |

Provider 模式默认关闭，旧订阅缺少该字段时也按关闭处理。关闭时通过转换器的 `list=true` 获取节点，将本次订阅的 Provider 展开为内嵌节点，并保留节点组筛选和 Provider 覆盖项；节点随整份订阅刷新。该开关控制节点 provider，与“展开规则集”独立。

刷新间隔传递给转换器，并应用到本次订阅对应的 HTTP provider；用户后处理脚本仍可覆盖它。本站不会保存任务或定时刷新。

顶层 `filename` 可选，用于客户端下载名和转换器 `filename` 参数。只填写不含扩展名的单段名称，默认 `sub-forge`；下载配置为 `<filename>.yaml`，原始订阅为 `<filename>.txt`。名称最长 64 个字符，不能包含路径分隔符、控制字符或 `: * ? " < > |`。

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

前端提供三个空白脚本编辑器，后端原样保存代码，包括注释与排版。三个脚本合计最多 64 KiB，请求 JSON 最多 512 KiB；链接长度不随脚本增长。无需脚本压缩、紧凑字段、zlib 或 Base64 编码。

新建订阅始终生成独立链接。载入已有订阅后，修改参数或脚本可保存到原链接；另存为新链接不会改变原订阅。订阅源、规则模板与远程规则列表的回调也使用数据库中的独立随机 ID，重复刷新复用记录。

### 执行环境

QuickJS 只提供 JavaScript 标准能力，没有文件系统、环境变量、网络、Node.js、npm、模块导入或定时器，不接受 Promise 或 `async transform`。每次执行限制时间、内存、栈和并发。核心生成地址时校验参数和脚本语法，在实际取源和转换时检查脚本入口、同步返回值与执行结果。依赖实际节点、模板或 YAML 字段的错误在使用链接时反馈。

`context.version` 为 1；`context.stage` 为对应的阶段名称；`context.urls` 提供当前阶段相关的回调地址；`context.secrets` 始终为空对象。不同脚本之间不共享上下文修改。回调地址不包含管理 token，但可读取对应内容，请使用可信脚本及转换器。

YAML 经对象解析后重新序列化，注释、锚点写法和排版不会保留。程序检查最终顶层对象及大小，不替代 Mihomo 内核验证所有配置字段。转换器的管理员插入节点功能固定关闭。

## 发布资产与平台

每次 GitHub Release 同时发布容器镜像和独立可执行文件。在 [Releases](https://github.com/enximi/sub-forge/releases/latest) 下载对应平台的压缩包，包内包含程序、`README.md` 和 `LICENSE`：

| 系统 | 架构 | Release 文件 |
| --- | --- | --- |
| Linux | x86_64 | `sub-forge-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz` |
| Linux | aarch64 | `sub-forge-vX.Y.Z-aarch64-unknown-linux-gnu.tar.gz` |
| macOS | Intel | `sub-forge-vX.Y.Z-x86_64-apple-darwin.tar.gz` |
| macOS | Apple Silicon | `sub-forge-vX.Y.Z-aarch64-apple-darwin.tar.gz` |
| Windows | x86_64 | `sub-forge-vX.Y.Z-x86_64-pc-windows-msvc.zip` |

Linux 可执行文件在 Debian 11 环境构建，要求 glibc 2.31 或更新版本。macOS 包内的二进制已做 ad-hoc 签名；如果被 Gatekeeper 隔离，可执行 `xattr -d com.apple.quarantine sub-forge` 后运行。每个 Release 提供 `SHA256SUMS` 供下载后校验。

容器镜像与 Linux 可执行文件覆盖完全相同的架构：`linux/amd64`、`linux/arm64`。macOS 和 Windows 无法以容器形式发布，因此只提供独立可执行文件。

## 单容器部署

程序和网页打包在同一个 `ghcr.io/enximi/sub-forge` 镜像中，支持 `linux/amd64`、`linux/arm64`。当前版本为 `0.6.1`。需要从源码构建时使用：

```bash
docker compose -f compose.yml -f compose.dev.yml up -d --build --wait
```

先在 `compose.yml` 设置实际 `SUB_FORGE_PUBLIC_BASE_URL`；私有部署填写 `SUB_FORGE_ACCESS_TOKEN`。启动后打开 `http://127.0.0.1:8002/`，网页和 `/api/v1/*` 都由该端口提供。SQLite 数据保存在 `sub-forge-data` 命名卷中。

使用发布镜像部署或升级：

```bash
docker compose pull
docker compose up -d --no-build --wait
```

HTTPS、域名和证书由外部反向代理负责，将整个站点转发到 `127.0.0.1:8002` 即可，无需按网页和 API 分流。首页返回 200，`/health` 返回 204，未知路径返回 404。仅将真实代理网络填入 `TRUSTED_PROXY_CIDRS`，避免代理记录完整订阅路径、查询参数或请求体。

从原先的两个容器部署升级时，还需停止和移除旧的独立网页容器；保留 SQLite 数据卷。生产升级使用明确版本或摘要。调用方自行选择可公网访问的 SubConverter 兼容服务；Sub-Store 是可选上游，配置见 `compose.sub-store.yml`，不属于程序运行依赖。

## 页面与 API 地址

页面自动使用同源 API，不需要填写后端地址或配置 CORS。生成结果使用服务端 `SUB_FORGE_PUBLIC_BASE_URL`，以便外部转换器和客户端访问；页面“下载配置”通过当前站点的 API 读取同一记录，无需携带管理 token。

部署在路径前缀下时，例如 `https://example.com/sub-forge/`，反向代理须把 `/sub-forge` 重定向到 `/sub-forge/`，并去除该前缀后转发给程序。`SUB_FORGE_PUBLIC_BASE_URL` 设置为 `https://example.com/sub-forge`。页面资源与 API 请求使用相对路径。

`SUB_FORGE_ALLOWED_ORIGINS` 仅用于额外的第三方浏览器调用方；内置页面不需要设置。只接受明确来源，不使用会话 Cookie。网页与 API 共用一个程序版本，修改 `frontend/` 文件后需要重新编译。

## 本机运行

核心从环境变量读取部署配置，直接运行时会加载根目录 `.env`。首次使用复制示例：

```bash
cp -n .env.example .env
```

在 `.env` 中填写实际公开地址和程序监听地址，然后运行：

```bash
cargo run --locked -p sub-forge
```

默认打开 `http://127.0.0.1:8080/` 即可使用网页。订阅链接必须包含可公网访问的转换器地址。本机运行也会校验用户输入 URL 的公网访问边界；执行完整转换时，将 `SUB_FORGE_PUBLIC_BASE_URL` 设置为转换器和客户端均可访问的公开 API 地址。

## SQLite 持久化与迁移

`SUB_FORGE_DATABASE_PATH` 默认是 `data/sub-forge.sqlite3`，父目录自动创建。Compose 使用命名卷 `sub-forge-data` 挂载 `/app/data`，API 以非 root 用户运行，镜像预先设置目录权限。SQLite 使用 WAL，启动时按 `user_version` 事务迁移，保留已有记录的 ID 和创建时间；重启和容器重建保留数据，删除数据卷会使全部已有链接失效。

数据库保存原始订阅地址、参数与脚本，请保护目录权限。在线备份使用 SQLite 的 `.backup`，或停止服务后备份整个数据目录；运行时不要只复制主数据库而漏掉 WAL 文件。多个独立数据库的副本不能处理彼此生成的链接，当前以单实例和本地持久卷部署为主。

旧的 `v1.` 编码链接不再支持，需要在新前端重新填写参数生成。数据库记录不自动过期，暂不提供账户、删除或单条撤销接口。输入仍仅保存在浏览器页面内存，刷新页面不会恢复表单，但已生成的链接可继续使用。

`v0.1.0` 起使用 SQLite 和单体部署；Compose 默认使用当前发布版本镜像。

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

生成接口的 JSON 上限 512 KiB，每个输入 URL 上限 2048 字节。响应逐块读取并检查大小。HTTP 响应禁用缓存、跨站引用和内容类型嗅探。服务不记录完整请求 URI、请求体或用户订阅。健康接口不会执行转换，不能代表整条链路正常。

## 开发与发布

```bash
cargo fmt --all --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --locked -p sub-forge
```

本地需要构建容器时使用 `docker compose -f compose.yml -f compose.dev.yml up -d --build`。网页资源嵌入 Rust 二进制，单个镜像以非 root 用户运行，不包含 Rust 源码与私人配置。构建上下文采用允许列表，排除 `.env` 和本地部署资料。

项目仍处于开发阶段，发布版本统一使用 `0.y.z`，Git tag 使用 `v0.y.z`。不兼容的架构或接口变更递增 `y`，兼容修复递增 `z`；达到稳定标准后再单独决定是否进入 `1.0.0`。发布工作流会拒绝非 `v0.y.z` 标签，并校验标签与 Cargo 版本一致。

发布步骤：

1. 更新 Cargo workspace 版本及 Compose、文档中的镜像标签，并更新 `Cargo.lock`。
2. 完成上述静态检查，提交后创建相同版本的 `v0.y.z` tag。
3. 在 GitHub 发布对应 Release，自动触发 `Publish container image` 和 `Publish release binaries` 两个工作流。
4. 等待 Rust / JavaScript 检查、Linux / macOS / Windows 可执行文件与 `SHA256SUMS` 上传、多架构镜像清单发布全部成功，再部署版本镜像或分发可执行文件。

工作流使用 GitHub 提供的 `GITHUB_TOKEN` 推送 GHCR，无需配置个人访问令牌。首次发布后，将容器包的可见性设为 Public，使服务器无需登录就能拉取；后续版本沿用包的可见性。也可在 Actions 中选择现有版本 tag 手动重跑。公开镜像附带源码地址、版本、提交 SHA、构建来源和 SBOM。

## 许可证

项目代码采用 [MIT License](LICENSE)。ACL4SSR 模板与用户选择的转换器属于独立上游项目，使用时遵守各自许可证。
