function endpoint(path) {
  // 相对页面目录定位 API，兼容反向代理去除部署路径前缀的场景。
  return new URL(`api/v1/${path}`, new URL("./", window.location.href)).href;
}

function authorization(accessToken) {
  return accessToken ? { Authorization: `Bearer ${accessToken}` } : {};
}

async function request(url, options, signal, timeoutMs, read) {
  const controller = new AbortController();
  const abort = () => controller.abort();
  signal.addEventListener("abort", abort, { once: true });
  if (signal.aborted) abort();
  const timer = setTimeout(abort, timeoutMs);
  try {
    const response = await fetch(url, {
      ...options, signal: controller.signal,
      credentials: "omit", cache: "no-store", referrerPolicy: "no-referrer",
    });
    if (!response.ok) {
      let message = `请求失败（HTTP ${response.status}）。`;
      try {
        const body = await response.json();
        if (typeof body.error === "string") message = body.error;
      } catch {
        // 代理可能返回 HTML 错误，保留 HTTP 状态提示。
      }
      throw new Error(message);
    }
    return await read(response);
  } catch (error) {
    if (controller.signal.aborted) throw new Error("请求已取消或超时，请稍后重试。");
    if (error instanceof TypeError) throw new Error("无法连接服务，请检查网络后重试。");
    throw error;
  } finally {
    clearTimeout(timer);
    signal.removeEventListener("abort", abort);
  }
}

async function subscriptionRequest(path, method, accessToken, body, signal) {
  const headers = { "Content-Type": "application/json", ...authorization(accessToken) };
  const result = await request(endpoint(path), {
    method, headers, body: JSON.stringify(body),
  }, signal, 30000, async response => {
    try { return await response.json(); } catch {
      throw new Error("后端返回了无效的 JSON 响应。");
    }
  });
  if (typeof result?.url !== "string" || !/^https?:\/\//.test(result.url) ||
      typeof result.id !== "string" || !/^[0-9a-f]{32}$/.test(result.id)) {
    throw new Error("后端未返回有效的订阅地址。");
  }
  return result;
}

export function createSubscription(accessToken, config, signal) {
  return subscriptionRequest("subscriptions", "POST", accessToken, config, signal);
}

export function loadSubscription(accessToken, url, signal) {
  return subscriptionRequest("subscriptions/load", "POST", accessToken, { url }, signal);
}

export function updateSubscription(id, accessToken, config, signal) {
  return subscriptionRequest(`subscriptions/${id}`, "PUT", accessToken, config, signal);
}

export function downloadConfiguration(id, signal) {
  return request(endpoint(`clash/${id}`), {}, signal, 70000, response => response.blob());
}
