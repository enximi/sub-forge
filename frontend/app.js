import { createSubscription, downloadConfiguration } from "./api.js";
import { initRulesPicker } from "./rules-picker.js";

const form = document.querySelector("#subscription-form");
const status = document.querySelector("#status");
const result = document.querySelector("#result");
const resultUrl = document.querySelector("#result-url");
const generateButton = document.querySelector("#generate-button");
const downloadButton = document.querySelector("#download-button");
const helpDialog = document.querySelector("#help-dialog");
let revision = 0;
let generatedSubscription;
let generationController;
let downloadController;

function showStatus(message, error = false) {
  status.textContent = message;
  status.classList.toggle("error", error);
  status.hidden = false;
}

function stopGeneration() {
  generationController?.abort();
  generationController = undefined;
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
  generatedSubscription = undefined;
  status.hidden = true;
  event.target.setCustomValidity?.("");
  event.target.removeAttribute("aria-invalid");
}

form.addEventListener("input", invalidate);
form.addEventListener("change", invalidate);
form.addEventListener("invalid", (event) => event.target.setAttribute("aria-invalid", "true"), true);

const rulesPicker = initRulesPicker(form, invalidate);

function scrollTo(element) {
  element.scrollIntoView({
    behavior: matchMedia("(prefers-reduced-motion: reduce)").matches ? "instant" : "smooth",
    block: "center",
  });
}

function readForm() {
  const data = new FormData(form);
  return {
    accessToken: data.get("access_token"),
    config: {
      source_url: data.get("source"),
      converter_url: data.get("converter"),
      policy: {
        rules: { source_url: rulesPicker.getUrl() },
        converter: {
          ...Object.fromEntries(["emoji", "tfo", "scv", "fdn", "expand", "sort", "new_name"].map(name => [name, data.has(name)])),
          interval_secs: Number(data.get("interval")),
        },
      },
      scripts: Object.fromEntries(["subscription", "preprocess_rules", "postprocess_config"].map(name => [name, data.get(name)])),
    },
  };
}

form.addEventListener("submit", async (event) => {
  event.preventDefault();
  stopGeneration();
  downloadController?.abort();
  downloadButton.disabled = false;
  downloadButton.textContent = "下载配置";
  const current = ++revision;
  result.hidden = true;
  resultUrl.value = "";
  generatedSubscription = undefined;
  const input = readForm();
  generateButton.disabled = true;
  generateButton.textContent = "正在生成…";
  form.setAttribute("aria-busy", "true");
  showStatus("正在保存参数并生成订阅地址…");

  function fail(message, field) {
    if (current !== revision) return;
    stopGeneration();
    showStatus(message, true);
    const element = field && document.getElementById(field);
    if (element) {
      element.setCustomValidity(message);
      element.setAttribute("aria-invalid", "true");
      element.focus();
      element.reportValidity();
    } else {
      scrollTo(status);
    }
  }

  const controller = new AbortController();
  generationController = controller;
  try {
    const output = await createSubscription(input.accessToken, input.config, controller.signal);
    if (current !== revision) return;
    stopGeneration();
    generatedSubscription = output;
    resultUrl.value = output.url;
    result.hidden = false;
    showStatus(`订阅地址已生成（${output.url.length} 字符），可以复制到客户端使用。`);
    scrollTo(result);
  } catch (error) {
    if (current !== revision) return;
    fail(error.message, error.field);
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
  if (!generatedSubscription || downloadButton.disabled) return;
  const current = revision;
  const controller = new AbortController();
  downloadController = controller;
  downloadButton.disabled = true;
  downloadButton.textContent = "正在转换…";
  showStatus("正在从后端获取最新订阅并转换配置…");
  try {
    const blob = await downloadConfiguration(generatedSubscription.id, controller.signal);
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
    showStatus(error.message, true);
  } finally {
    if (current === revision) {
      downloadController = undefined;
      downloadButton.disabled = false;
      downloadButton.textContent = "下载配置";
    }
  }
});

document.querySelector("#help-button").addEventListener("click", () => helpDialog.showModal());
