"use strict";

const form = document.querySelector("#subscription-form");
const status = document.querySelector("#status");
const result = document.querySelector("#result");
const resultUrl = document.querySelector("#result-url");
const generateButton = document.querySelector("#generate-button");
const downloadButton = document.querySelector("#download-button");
const helpDialog = document.querySelector("#help-dialog");
const rulesPreset = document.querySelector("#rules-preset");
const rulesUrl = document.querySelector("#rules-url");
const rulesModes = Array.from(form.querySelectorAll('[name="rules_mode"]'));
const rulesPresetPanel = document.querySelector("#rules-preset-panel");
const rulesCustomPanel = document.querySelector("#rules-custom-panel");
const rulesDescription = document.querySelector("#rules-preset-description");
const rulesTemplateLink = document.querySelector("#rules-template-link");
const rulesPicker = document.querySelector(".rules-picker");
const rulesPresetList = document.querySelector("#rules-preset-list");
const rulesPresetValue = document.querySelector("#rules-preset-value");
const rulesChoices = Array.from(rulesPresetList.querySelectorAll('[role="option"]'));
let selectedRulesPreset = rulesChoices.find(option => option.getAttribute("aria-selected") === "true");
let revision = 0;
let generationWorker;
let generationTimer;
let downloadController;

form.elements.backend.value = document.querySelector('meta[name="sub-forge-api-base-url"]').content;

function showStatus(message, error = false) {
  status.textContent = message;
  status.classList.toggle("error", error);
  status.hidden = false;
}

function stopGeneration() {
  generationWorker?.terminate();
  generationWorker = undefined;
  clearTimeout(generationTimer);
  generateButton.disabled = false;
  generateButton.textContent = "生成订阅地址";
  form.removeAttribute("aria-busy");
}

function invalidate(event) {
  if (event.target === resultUrl) return;
  // 输入变化即作废旧结果并终止异步任务，避免迟到的结果覆盖新配置。
  revision++;
  stopGeneration();
  downloadController?.abort();
  downloadController = undefined;
  downloadButton.disabled = false;
  downloadButton.textContent = "下载配置";
  result.hidden = true;
  resultUrl.value = "";
  status.hidden = true;
  event.target.setCustomValidity?.("");
  event.target.removeAttribute("aria-invalid");
}

form.addEventListener("input", invalidate);
form.addEventListener("change", invalidate);
form.addEventListener("invalid", (event) => event.target.setAttribute("aria-invalid", "true"), true);

function updateRulesMode() {
  closeRulesPicker();
  const usePreset = form.elements.rules_mode.value === "preset";
  rulesPresetPanel.hidden = !usePreset;
  rulesCustomPanel.hidden = usePreset;
  // 仅校验当前模式，另一种模式的输入保留在原控件中。
  rulesPreset.disabled = !usePreset;
  rulesUrl.disabled = usePreset;
  for (const control of [rulesPreset, rulesUrl]) {
    control.setCustomValidity("");
    control.removeAttribute("aria-invalid");
  }
}

function updateRulesDescription() {
  rulesPresetValue.textContent = selectedRulesPreset.querySelector(".rules-option-name").textContent;
  rulesDescription.textContent = selectedRulesPreset.dataset.description;
  rulesTemplateLink.href = selectedRulesPreset.querySelector(".rules-option-url").textContent;
}

function closeRulesPicker(restoreFocus = false) {
  rulesPresetList.hidden = true;
  rulesPreset.setAttribute("aria-expanded", "false");
  if (restoreFocus) rulesPreset.focus();
}

function openRulesPicker() {
  rulesPresetList.hidden = false;
  rulesPreset.setAttribute("aria-expanded", "true");
  selectedRulesPreset.focus();
  selectedRulesPreset.scrollIntoView({ block: "nearest" });
}

rulesPreset.addEventListener("click", () => {
  if (rulesPresetList.hidden) openRulesPicker();
  else closeRulesPicker();
});
rulesPreset.addEventListener("keydown", (event) => {
  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    event.preventDefault();
    openRulesPicker();
  }
});
for (const option of rulesChoices) {
  option.addEventListener("click", () => {
    selectedRulesPreset.setAttribute("aria-selected", "false");
    selectedRulesPreset = option;
    option.setAttribute("aria-selected", "true");
    updateRulesDescription();
    invalidate({ target: rulesPreset });
    closeRulesPicker(true);
  });
}
rulesPresetList.addEventListener("keydown", (event) => {
  const current = rulesChoices.indexOf(document.activeElement);
  let next;
  if (event.key === "ArrowDown") next = Math.min(current + 1, rulesChoices.length - 1);
  if (event.key === "ArrowUp") next = Math.max(current - 1, 0);
  if (event.key === "Home") next = 0;
  if (event.key === "End") next = rulesChoices.length - 1;
  if (next !== undefined) {
    event.preventDefault();
    rulesChoices[next].focus();
    rulesChoices[next].scrollIntoView({ block: "nearest" });
  } else if (event.key === "Escape") {
    event.preventDefault();
    event.stopPropagation();
    closeRulesPicker(true);
  } else if (event.key === "Tab") {
    // 先回到触发按钮，让浏览器按正常顺序移到列表之外。
    closeRulesPicker(true);
  }
});
document.addEventListener("pointerdown", (event) => {
  if (!rulesPicker.contains(event.target)) closeRulesPicker();
});
rulesPicker.addEventListener("focusout", (event) => {
  if (!rulesPicker.contains(event.relatedTarget)) closeRulesPicker();
});

for (const mode of rulesModes) mode.addEventListener("change", updateRulesMode);
updateRulesMode();
updateRulesDescription();

function scrollTo(element) {
  element.scrollIntoView({
    behavior: matchMedia("(prefers-reduced-motion: reduce)").matches ? "instant" : "smooth",
    block: "center",
  });
}

form.addEventListener("submit", (event) => {
  event.preventDefault();
  stopGeneration();
  downloadController?.abort();
  downloadButton.disabled = false;
  downloadButton.textContent = "下载配置";
  const current = ++revision;
  result.hidden = true;
  resultUrl.value = "";
  const data = new FormData(form);
  const input = Object.fromEntries(["backend", "access_token", "source", "rules", "converter", "interval"].map(name => [name, data.get(name)]));
  if (data.get("rules_mode") === "preset") input.rules = rulesTemplateLink.href;
  input.options = Object.fromEntries(["emoji", "tfo", "scv", "fdn", "expand", "sort", "new_name"].map(name => [name, data.has(name)]));
  input.scripts = Object.fromEntries(["subscription", "preprocess_rules", "postprocess_config"].map(name => [name, data.get(name)]));
  generateButton.disabled = true;
  generateButton.textContent = "正在生成…";
  form.setAttribute("aria-busy", "true");
  showStatus("正在本地压缩脚本并生成链接…");

  function fail(message, field) {
    if (current !== revision) return;
    stopGeneration();
    showStatus(message, true);
    const element = field === "rules-url" && rulesUrl.disabled
      ? rulesPreset
      : field && document.getElementById(field);
    if (element) {
      element.setCustomValidity(message);
      element.setAttribute("aria-invalid", "true");
      element.focus();
      element.reportValidity();
    } else {
      scrollTo(status);
    }
  }

  try {
    generationWorker = new Worker("./generator-worker.js");
    generationTimer = setTimeout(() => fail("本地处理超时，请缩短脚本后重试。"), 15000);
    generationWorker.onerror = (error) => {
      error.preventDefault();
      fail("无法加载链接生成器或 CDN 压缩库，请检查网络并通过 HTTP(S) 地址打开前端。");
    };
    generationWorker.onmessage = ({ data: output }) => {
      if (current !== revision) return;
      if (output.phase) {
        clearTimeout(generationTimer);
        const loading = output.phase === "loading";
        showStatus(loading ? "正在从 CDN 加载压缩库…" : "正在本地压缩脚本并生成链接…");
        generationTimer = setTimeout(() => fail(loading
          ? "CDN 压缩库加载超时，请检查网络后重试。"
          : "本地处理超时，请缩短脚本后重试。"), loading ? 60000 : 15000);
        return;
      }
      if (output.error) return fail(output.error, output.field);
      stopGeneration();
      resultUrl.value = output.url;
      result.hidden = false;
      showStatus(`订阅地址已生成（${output.url.length} 字符），可以复制到客户端使用。`);
      scrollTo(result);
    };
    generationWorker.postMessage(input);
  } catch {
    fail("浏览器无法启动链接生成器，请使用支持 Web Worker 的浏览器通过 HTTP(S) 打开页面。");
  }
});

document.querySelector("#copy-button").addEventListener("click", async () => {
  const current = revision;
  const url = resultUrl.value;
  if (!url) return;
  try {
    await navigator.clipboard.writeText(url);
    if (current === revision) showStatus("订阅地址已复制，粘贴到客户端的订阅设置中即可。");
  } catch {
    if (current !== revision) return;
    resultUrl.focus();
    resultUrl.select();
    showStatus("浏览器未允许自动复制，已选中链接，请手动复制。", true);
  }
});

downloadButton.addEventListener("click", async () => {
  if (!resultUrl.value || downloadButton.disabled) return;
  const current = revision;
  const controller = new AbortController();
  downloadController = controller;
  const timeout = setTimeout(() => controller.abort(), 70000);
  downloadButton.disabled = true;
  downloadButton.textContent = "正在转换…";
  showStatus("正在从后端获取最新订阅并转换配置…");
  try {
    const response = await fetch(resultUrl.value, {
      signal: controller.signal,
      credentials: "omit",
      cache: "no-store",
      referrerPolicy: "no-referrer",
    });
    if (!response.ok) {
      let message = `获取配置失败（HTTP ${response.status}）。`;
      try {
        const body = await response.json();
        if (typeof body.error === "string") message = body.error;
      } catch {
        // 网关可能返回非 JSON 错误，保留 HTTP 状态提示。
      }
      throw new Error(message);
    }
    const blob = await response.blob();
    if (current !== revision) return;
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = "sub-forge.yaml";
    link.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
    showStatus("配置已下载，可导入 Clash / Mihomo 客户端使用。");
  } catch (error) {
    if (current !== revision) return;
    showStatus(controller.signal.aborted
      ? "获取配置超时，请稍后重试。"
      : error instanceof TypeError
        ? "无法连接后端，请检查后端地址、HTTPS 和跨域设置。"
        : error.message, true);
  } finally {
    clearTimeout(timeout);
    if (current === revision) {
      downloadController = undefined;
      downloadButton.disabled = false;
      downloadButton.textContent = "下载配置";
    }
  }
});

document.querySelector("#help-button").addEventListener("click", () => helpDialog.showModal());
