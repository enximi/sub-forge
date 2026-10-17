# SubForge 静态前端

原生 HTML、CSS 和 JavaScript 单页，没有前端框架、构建步骤或 npm 运行依赖。订阅配置、转换选项、三个可选脚本和生成结果在同一页面纵向排列。浅色模式使用白色背景和中性灰配色，暗色模式跟随系统偏好自动切换。

浏览器使用本地 Web Worker 压缩脚本、编码配置并生成订阅链接，不调用后端创建接口。用户代码始终按文本解析，前端不会执行它。复制链接也不需要后端在线；仅下载配置时通过 GET 请求后端，客户端订阅刷新同样直接访问后端。

规则模板区域提供“ACL4SSR 预设”和“自定义链接”两种模式。默认使用推荐的标准版；18 个官方在线模板按标准版、完整版和精简版分组，展开列表时，每个选项同时展示名称和可换行的完整 URL；支持方向键、Home / End、Enter 和 Esc 操作。选择后显示用途说明，可通过“查看模板”打开原始文件。自定义模式只显示 INI 链接输入框；来回切换会分别保留当前模板和自定义地址，仅当前模式的配置参与校验和生成。预设列表内置于页面，浏览时不请求 GitHub API；实际使用订阅时由后端获取规则模板。

## 本机运行

在仓库根目录运行：

```bash
python3 -m http.server 8081 --bind 127.0.0.1 --directory frontend
```

打开 `http://127.0.0.1:8081/`。不支持直接双击 HTML 使用 `file://` 打开，因为生成器使用 Web Worker。后端地址默认留空，需在表单中填写；生成链接时后端可以不在线。

下载配置时需要运行核心，并设置允许的前端来源：

```dotenv
SUB_FORGE_ALLOWED_ORIGINS=http://127.0.0.1:8081
```

执行完整转换时，订阅、规则模板和转换器需要公网可达，核心的 `SUB_FORGE_PUBLIC_BASE_URL` 还必须能被转换器和客户端访问。本地生成成功不保证上游可访问或脚本实际运行成功。

## 部署

可直接使用 `ghcr.io/enximi/sub-forge-web:v1.0.0`，容器内监听 8080，默认 Compose 映射到本机 8081。镜像由 GitHub Actions 构建，服务器只拉取镜像，无需保存源码。网页与 API 独立运行，代理可在同一域名下按 `/api/*`、`/health` 分流。后端地址和 token 仍由使用者在页面填写，不写进公开镜像。


将 `index.html`、`styles.css`、`app.js`、`generator-worker.js` 部署到静态托管服务，保留相对路径即可，也支持部署在子目录下。不需要构建；Worker 从 jsDelivr CDN 加载固定版本的压缩库，使用时需能访问该 CDN。订阅信息和用户脚本不会上传到 CDN。

`index.html` 的 `sub-forge-api-base-url` 元标签默认留空。如需部署时预填后端地址，可修改该元标签，用户也可以在页面中覆盖。基础地址不包含 `/api/v1`、查询参数或片段；允许路径前缀，例如 `https://example.com/sub-forge`。前端自动拼接 `/api/v1/clash/<token>`。

“访问 token（可选）”默认留空，公开服务无需填写。私有部署填写服务端 `SUB_FORGE_ACCESS_TOKEN` 的值，须为 32 至 256 个无空白的可打印 ASCII 字符。它通过 `access_token` 查询参数加入生成链接，不参与配置 JSON 压缩；下载、复制和客户端刷新都使用这个完整链接。输入按密码显示，仅保存在当前页面内存，不写入静态文件或浏览器本地存储。后端 token 缺失或错误时，“下载配置”显示鉴权错误。

访问 token 具有整个服务的权限；后端自动在转换器和 provider 回调中携带它，因此转换器及生成的配置也会接触到它。更换服务端 token 后，应重新生成订阅并更新客户端配置；请勿分享完整链接或配置。

建议通过 HTTPS 部署，以使用浏览器剪贴板；无法自动复制时会选中链接，允许手动复制。HTTPS 页面下载配置也需要 HTTPS 后端。后端的 `SUB_FORGE_ALLOWED_ORIGINS` 需包含实际前端来源，生成和复制链接不依赖该配置。

如果托管服务设置 CSP，需要允许本站脚本与 Worker、允许 `script-src` 加载 `https://cdn.jsdelivr.net`，并允许 `connect-src` 连接所用后端。

## 编码与脚本

完整协议与短字段映射见[核心 README](../crates/sub-forge/README.md#订阅链接格式)。前端仅生成用途为 `f` 的订阅令牌；核心生成订阅源、规则模板和规则资源的专用回调令牌。

```text
参数与脚本 → Terser 压缩 → 紧凑 JSON → UTF-8 → zlib → Base64URL（无填充）→ v1. 令牌
```

- `generator-worker.js` 中的默认值与 Rust `CompactOptions` 的 v1 默认值一致。省略默认选项、空脚本和空对象。
- 使用 Terser 5.51.2，禁用逻辑优化、名称改写和 Source Map，只去掉注释与多余空白；非空脚本才加载压缩器。解析允许函数体中的 `return`；脚本执行约定不支持 ES 模块导入导出。不会在浏览器中编译执行或验证 `transform` 的运行结果。
- 使用 fflate 0.8.3 的 `zlibSync`，压缩级别 9。后端支持标准 zlib，无需与 Rust 的压缩结果逐字节相同。
- 输入 URL 最多 2048 UTF-8 字节；原始与压缩后的三个脚本合计均最多 64 KiB；JSON 最多 128 KiB；含 `v1.` 的令牌最多 8192 字符。完整 URL 还包括后端地址和路径，客户端或代理可能有更低限制。
- 处理在 Web Worker 中进行，依赖加载超过 60 秒、单阶段本地处理超过 15 秒时终止并提示；修改输入会取消当前生成或下载、清除旧结果，过期异步结果不能恢复旧链接。
- 链接不提供加密、签名、自动过期或单条撤销，请勿公开分享。QuickJS 兼容性、同步 `transform` 入口、实际输出和公网访问边界由核心在使用链接时检查。

输入只保存在页面内存，不使用 localStorage、账户、数据库或持久化缓存。刷新页面后清空。下载超时为 70 秒，错误会显示在生成区域。

## 第三方依赖与验证

生成器固定使用下列官方 npm 包的 CDN 发行文件：

- [Terser 5.51.2](https://cdn.jsdelivr.net/npm/terser@5.51.2/dist/bundle.min.js)，BSD-2-Clause。只在非空脚本需要压缩时加载，不生成 Source Map，也无需加载独立的 Source Map 库。
- [fflate 0.8.3](https://cdn.jsdelivr.net/npm/fflate@0.8.3/umd/index.js)，MIT。生成时在 Worker 内加载。

升级时修改 `generator-worker.js` 中固定的版本 URL，并确认链接与后端解码兼容。CDN 不可用时会显示加载失败或超时提示，不会生成不完整链接。

```bash
node --check frontend/app.js
node --check frontend/generator-worker.js
```

CI 检查 JavaScript 语法，功能验证使用实际浏览器交互和隔离的 HTTP 调用。
