import { createSubscription, loadSubscription, updateSubscription, downloadConfiguration } from "./api.js";
import { initRulesPicker } from "./rules-picker.js";

const form = document.querySelector("#subscription-form");
const status = document.querySelector("#status");
const result = document.querySelector("#result");
const resultUrl = document.querySelector("#result-url");
const generateButton = document.querySelector("#generate-button");
const downloadButton = document.querySelector("#download-button");
const helpDialog = document.querySelector("#help-dialog");
const existingUrl = document.querySelector("#existing-url");
const loadButton = document.querySelector("#load-button");
const updatePanel = document.querySelector("#update-panel");
const editingNote = document.querySelector("#editing-note");
const accessTokenField = document.querySelector("#access-token-field");
const accessTokenInput = form.elements.access_token;
const accessTokenRequired = form.dataset.accessTokenRequired === "true";

if (accessTokenRequired) {
  accessTokenInput.required = true;
} else {
  // 公开部署不需要 token：隐藏并禁用字段，避免被自动填充或误提交。
  accessTokenField.hidden = true;
  accessTokenInput.disabled = true;
}

let editingSubscription;
let revision = 0;
let generatedSubscription;
let generationController;
let downloadController;

function showStatus(message, error = false) {
  status.textContent = message;
  status.classList.toggle("error", error);
  status.hidden = false;
}

function isUpdateMode() {
  return form.elements.subscription_mode.value === "update";
}

function stopGeneration() {
  generationController?.abort();
  generationController = undefined;
  const updating = isUpdateMode();
  updatePanel.hidden = !updating;
  existingUrl.disabled = !updating;
  generateButton.disabled = updating && !editingSubscription;
  generateButton.textContent = updating ? "保存到原链接" : "生成订阅地址";
  loadButton.disabled = false;
  loadButton.textContent = "载入配置";
  editingNote.hidden = !editingSubscription;
  form.removeAttribute("aria-busy");
}

function invalidate(event) {
  if (event.target === resultUrl) return;
  if (event.target === existingUrl || event.target.name === "subscription_mode") editingSubscription = undefined;
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

loadButton.addEventListener("click", async () => {
  if (!isUpdateMode()) return;
  if (!existingUrl.value.trim() || !existingUrl.reportValidity()) {
    showStatus("请先填写已有的 SubForge 订阅链接。", true);
    existingUrl.focus();
    return;
  }
  if (accessTokenRequired && !accessTokenInput.value.trim()) {
    showStatus("请填写管理 token。", true);
    accessTokenInput.focus();
    return;
  }
  invalidate({ target: existingUrl });
  const current = revision;
  const controller = new AbortController();
  generationController = controller;
  loadButton.disabled = true;
  loadButton.textContent = "正在载入…";
  generateButton.disabled = true;
  form.setAttribute("aria-busy", "true");
  showStatus(accessTokenRequired ? "正在验证 token 并载入配置…" : "正在载入配置…");
  try {
    const output = await loadSubscription(accessTokenInput.value, existingUrl.value, controller.signal);
    if (current !== revision) return;
    const config = output.config;
    form.elements.source.value = config.source_url;
    form.elements.converter.value = config.converter_url;
    form.elements.filename.value = config.filename || "";
    rulesPicker.setUrl(config.policy.rules.source_url);
    for (const name of ["emoji", "tfo", "scv", "fdn", "expand", "sort", "new_name"]) {
      form.elements[name].checked = config.policy.converter[name];
    }
    form.elements.interval.value = config.policy.converter.interval_secs;
    for (const name of ["subscription", "preprocess_rules", "postprocess_config"]) {
      form.elements[name].value = config.scripts[name];
    }
    for (const element of form.elements) {
      element.setCustomValidity?.("");
      element.removeAttribute("aria-invalid");
    }
    editingSubscription = { id: output.id, url: output.url };
    existingUrl.value = output.url;
    showStatus("配置已载入。修改后保存到原链接，客户端无需更换订阅地址。");
    scrollTo(document.querySelector("#subscription-heading"));
  } catch (error) {
    if (current === revision) {
      showStatus(error.message, true);
      scrollTo(status);
    }
  } finally {
    if (current === revision) stopGeneration();
  }
});

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
      filename: data.get("filename"),
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
  if (isUpdateMode() && !editingSubscription) {
    showStatus("请先载入需要更新的订阅配置。", true);
    existingUrl.focus();
    return;
  }
  stopGeneration();
  downloadController?.abort();
  downloadButton.disabled = false;
  downloadButton.textContent = "下载配置";
  const current = ++revision;
  result.hidden = true;
  resultUrl.value = "";
  generatedSubscription = undefined;
  const input = readForm();
  const target = isUpdateMode() ? editingSubscription : undefined;
  generateButton.disabled = true;
  loadButton.disabled = true;
  generateButton.textContent = target ? "正在保存…" : "正在生成…";
  form.setAttribute("aria-busy", "true");
  showStatus(target ? "正在更新原链接的配置…" : "正在保存参数并生成订阅地址…");

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
    const output = target
      ? await updateSubscription(target.id, input.accessToken, input.config, controller.signal)
      : await createSubscription(input.accessToken, input.config, controller.signal);
    if (current !== revision) return;
    editingSubscription = target ? output : undefined;
    existingUrl.value = output.url;
    stopGeneration();
    generatedSubscription = output;
    resultUrl.value = output.url;
    result.hidden = false;
    showStatus(target ? "配置已保存，订阅地址保持不变。请在客户端更新整份订阅以应用修改。" : `订阅地址已生成（${output.url.length} 字符），可以复制到客户端使用。`);
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
    const download = await downloadConfiguration(generatedSubscription.id, controller.signal);
    if (current !== revision) return;
    const url = URL.createObjectURL(download.blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = download.filename || "sub-forge.yaml";
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
