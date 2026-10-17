/* global Terser, fflate */
"use strict";

const encoder = new TextEncoder();
const MAX_SCRIPTS_BYTES = 64 * 1024;
const MAX_TICKET_BYTES = 128 * 1024;
const MAX_TOKEN_CHARS = 8192;

function fail(message, field) {
  throw Object.assign(new Error(message), { field });
}

function parseUrl(value, field, base = false) {
  const raw = value.trim();
  if (encoder.encode(raw).length > 2048) fail("链接不能超过 2048 字节。", field);
  let url;
  try {
    url = new URL(raw);
  } catch {
    fail("请输入完整的 HTTP(S) 链接。", field);
  }
  if (!["http:", "https:"].includes(url.protocol) || !url.hostname || url.username || url.password) {
    fail("请输入不含用户名和密码的 HTTP(S) 链接。", field);
  }
  if (base && (url.search || url.hash || /\/api\/v1\/?$/.test(url.pathname))) {
    fail("后端地址不应包含 /api/v1、查询参数或片段。", field);
  }
  url.hash = "";
  if (encoder.encode(url.href).length > 2048) fail("链接编码后超过 2048 字节，请缩短后重试。", field);
  return url;
}

function loadLibrary(name, url) {
  if (!self[name]) {
    self.postMessage({ phase: "loading" });
    try {
      importScripts(url);
    } catch {
      fail("无法从 CDN 加载压缩库，请检查网络后重试。");
    }
  }
  self.postMessage({ phase: "processing" });
}

async function generate(input) {
  const base = parseUrl(input.backend, "backend-url", true);
  const accessToken = input.access_token ?? "";
  if (accessToken && !/^[\x21-\x7e]{32,256}$/.test(accessToken)) {
    fail("访问 token 须为 32 至 256 个可打印 ASCII 字符，不能包含空格。", "access-token");
  }
  const ticket = {
    k: "f",
    s: parseUrl(input.source, "source-url").href,
    c: parseUrl(input.converter, "converter-url").href,
    r: parseUrl(input.rules, "rules-url").href,
  };
  const interval = Number(input.interval);
  if (!Number.isInteger(interval) || interval < 60 || interval > 604800) {
    fail("刷新间隔须为 60 至 604800 之间的整数。", "interval");
  }

  // 缺省值属于 v1 编码协议，与 Rust CompactOptions 保持一致。
  const options = {};
  for (const [name, key, defaultValue] of [
    ["emoji", "e", true], ["tfo", "t", false], ["scv", "s", false],
    ["fdn", "f", false], ["expand", "x", true], ["sort", "o", false],
    ["new_name", "n", true],
  ]) {
    if (input.options[name] !== defaultValue) options[key] = input.options[name];
  }
  if (interval !== 86400) options.i = interval;
  if (Object.keys(options).length) ticket.o = options;

  const stages = [
    ["subscription", "s", "subscription-script", "原始订阅"],
    ["preprocess_rules", "r", "rules-script", "规则预处理"],
    ["postprocess_config", "c", "config-script", "后处理"],
  ];
  const sourceBytes = stages.reduce((total, [name]) => total + encoder.encode(input.scripts[name]).length, 0);
  if (sourceBytes > MAX_SCRIPTS_BYTES) fail("三个脚本合计不能超过 64 KiB，请缩短后重试。");
  const scripts = {};
  for (const [name, key, field, label] of stages) {
    const source = input.scripts[name];
    if (!source.trim()) continue;
    // 只在需要时加载压缩器；始终把用户代码当作文本，不在浏览器中执行。
    loadLibrary("Terser", "https://cdn.jsdelivr.net/npm/terser@5.51.2/dist/bundle.min.js");
    try {
      const result = await Terser.minify(source, {
        parse: { bare_returns: true },
        compress: false,
        mangle: false,
        module: false,
        toplevel: false,
        format: { comments: false, beautify: false },
        sourceMap: false,
      });
      if (result.code) scripts[key] = result.code;
    } catch (error) {
      const location = error.line ? `（第 ${error.line} 行，第 ${(error.col ?? 0) + 1} 列）` : "";
      fail(`${label}脚本语法错误${location}：${error.message}`, field);
    }
  }
  if (Object.values(scripts).reduce((total, code) => total + encoder.encode(code).length, 0) > MAX_SCRIPTS_BYTES) {
    fail("压缩后的脚本合计超过 64 KiB，请缩短后重试。");
  }
  if (Object.keys(scripts).length) ticket.j = scripts;
  const json = encoder.encode(JSON.stringify(ticket));
  if (json.length > MAX_TICKET_BYTES) fail("脚本与链接内容超过 128 KiB，请缩短后重试。");
  loadLibrary("fflate", "https://cdn.jsdelivr.net/npm/fflate@0.8.3/umd/index.js");
  const compressed = fflate.zlibSync(json, { level: 9 });
  let binary = "";
  for (const byte of compressed) binary += String.fromCharCode(byte);
  const token = "v1." + btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
  if (token.length > MAX_TOKEN_CHARS) {
    fail("脚本与链接压缩后仍超过地址长度限制（令牌最多 8192 字符），请缩短后重试。");
  }
  base.pathname = `${base.pathname.replace(/\/+$/, "")}/api/v1/clash/${token}`;
  if (accessToken) base.searchParams.set("access_token", accessToken);
  return { url: base.href };
}

self.onmessage = async ({ data }) => {
  try {
    self.postMessage(await generate(data));
  } catch (error) {
    self.postMessage({ error: error.message || "无法生成链接，请检查输入后重试。", field: error.field });
  }
};
