import { createSubscription, loadSubscription, updateSubscription, downloadConfiguration } from "./api.js";
import { initRulesPicker } from "./rules-picker.js";

const form = document.querySelector("#subscription-form");
const status = document.querySelector("#status");
const result = document.querySelector("#result");
const resultUrl = document.querySelector("#result-url");
const resultTitle = document.querySelector("#result-title");
const resultSubtitle = document.querySelector("#result-subtitle");
const resultFeedback = document.querySelector("#result-feedback");
const generateButton = document.querySelector("#generate-button");
const copyButton = document.querySelector("#copy-button");
const downloadButton = document.querySelector("#download-button");
const helpDialog = document.querySelector("#help-dialog");
const existingUrl = document.querySelector("#existing-url");
const loadButton = document.querySelector("#load-button");
const updatePanel = document.querySelector("#update-panel");
const editingNote = document.querySelector("#editing-note");
const loadError = document.querySelector("#load-error");
const accessTokenField = document.querySelector("#access-token-field");
const accessTokenInput = form.elements.access_token;
const accessTokenRequired = form.dataset.accessTokenRequired === "true";
const tokenToggle = document.querySelector("#toggle-token");
const intervalHidden = document.querySelector("#interval");
const intervalPresets = Array.from(form.querySelectorAll('[name="interval_preset"]'));
const intervalCustom = document.querySelector("#interval-custom");
const intervalCustomValue = document.querySelector("#interval-custom-value");
const saveModeText = document.querySelector("#save-mode-text");
const mobileActionBar = document.querySelector("#mobile-action-bar");
const mobileGenerateButton = document.querySelector("#mobile-generate-button");
const qrButton = document.querySelector("#qr-button");
const qrDialog = document.querySelector("#qr-dialog");
const qrCode = document.querySelector("#qr-code");

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
let copyFeedbackTimer;
let downloadFeedbackTimer;
let formDirty = false;

function showStatus(message, error = false) {
  status.textContent = message;
  status.classList.toggle("error", error);
  status.hidden = false;
}

function showResultFeedback(message, error = false) {
  resultFeedback.textContent = message;
  resultFeedback.classList.toggle("error", error);
  resultFeedback.hidden = false;
}

function clearResultFeedback() {
  resultFeedback.textContent = "";
  resultFeedback.classList.remove("error");
  resultFeedback.hidden = true;
}

function setButtonLabel(button, text) {
  const label = button.querySelector(".button-label, .download-label");
  if (label) label.textContent = text;
}

function setButtonBusy(button, busy) {
  button.classList.toggle("is-busy", busy);
  button.setAttribute("aria-busy", busy ? "true" : "false");
}

function setIntervalValue(seconds) {
  const value = String(seconds ?? "").trim();
  const preset = ["3600", "21600", "86400"].includes(value) ? value : "custom";
  intervalHidden.value = value || "86400";
  for (const radio of intervalPresets) radio.checked = radio.value === preset;
  const custom = preset === "custom";
  intervalCustom.hidden = !custom;
  intervalCustomValue.disabled = !custom;
  if (custom) intervalCustomValue.value = value;
}

function updateIntervalFromPreset() {
  const preset = intervalPresets.find((radio) => radio.checked);
  if (!preset) return;
  if (preset.value === "custom") {
    intervalCustom.hidden = false;
    intervalCustomValue.disabled = false;
    if (!intervalCustomValue.value) intervalCustomValue.value = intervalHidden.value || "86400";
    intervalHidden.value = intervalCustomValue.value;
    intervalCustomValue.focus();
  } else {
    intervalCustom.hidden = true;
    intervalCustomValue.disabled = true;
    intervalHidden.value = preset.value;
  }
}

function syncSaveMode() {
  const message = !isUpdateMode()
    ? "将创建独立链接"
    : editingSubscription
      ? "将保存到原链接"
      : "先载入要更新的订阅";
  if (saveModeText.textContent !== message) saveModeText.textContent = message;
}

function syncMobileAction() {
  // 结果未生成时使用常驻底部操作栏，并隐藏页面内的重复按钮。
  mobileActionBar.hidden = !result.hidden;
  document.body.classList.toggle("sticky-action-visible", !mobileActionBar.hidden);
  mobileGenerateButton.disabled = generateButton.disabled;
  setButtonLabel(mobileGenerateButton, generateButton.querySelector(".button-label").textContent);
  setButtonBusy(mobileGenerateButton, generateButton.classList.contains("is-busy"));
}

function showFieldError(control) {
  const field = control.closest?.(".field");
  if (!field) return;
  let message = field.querySelector(".field-error");
  if (!message) {
    message = document.createElement("p");
    message.className = "field-error";
    message.setAttribute("role", "alert");
    field.append(message);
  }
  message.textContent = control.validationMessage || "请检查此项。";
  message.hidden = false;
}

function clearFieldError(control) {
  control.closest?.(".field")?.querySelector(".field-error")?.remove();
}

function clearAllFieldErrors() {
  for (const message of form.querySelectorAll(".field-error")) message.remove();
}

function fallbackCopySelection() {
  try {
    return document.execCommand("copy");
  } catch {
    return false;
  }
}

function flashCopyButton() {
  clearResultFeedback();
  clearTimeout(copyFeedbackTimer);
  copyButton.classList.remove("copied");
  // 强制重排，让连续复制也能重新播放动画。
  void copyButton.offsetWidth;
  copyButton.classList.add("copied");
  copyButton.setAttribute("aria-label", "已复制");
  copyFeedbackTimer = setTimeout(() => {
    copyButton.classList.remove("copied");
    copyButton.removeAttribute("aria-label");
  }, 1200);
}

function flashDownloadButton() {
  clearResultFeedback();
  clearTimeout(downloadFeedbackTimer);
  downloadButton.classList.remove("downloaded");
  void downloadButton.offsetWidth;
  downloadButton.classList.add("downloaded");
  downloadButton.setAttribute("aria-label", "已下载");
  downloadFeedbackTimer = setTimeout(() => {
    downloadButton.classList.remove("downloaded");
    downloadButton.removeAttribute("aria-label");
  }, 1200);
}

function resetDownloadButton() {
  clearTimeout(downloadFeedbackTimer);
  downloadButton.classList.remove("downloaded");
  downloadButton.removeAttribute("aria-label");
  downloadButton.disabled = false;
  setButtonLabel(downloadButton, "下载配置");
  setButtonBusy(downloadButton, false);
}

async function copyResultUrl() {
  const current = revision;
  const url = resultUrl.value;
  if (!url) return;
  try {
    await navigator.clipboard.writeText(url);
    if (current === revision) flashCopyButton();
  } catch {
    if (current !== revision) return;
    resultUrl.focus();
    resultUrl.select();
    if (fallbackCopySelection()) {
      flashCopyButton();
      return;
    }
    showResultFeedback("自动复制被浏览器拦截，请点击“复制链接”或按 Ctrl/Cmd+C。", true);
  }
}

function showLoadError(message) {
  status.hidden = true;
  loadError.textContent = message;
  loadError.hidden = false;
}

function clearLoadError() {
  loadError.textContent = "";
  loadError.hidden = true;
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
  setButtonLabel(generateButton, updating ? "保存到原链接" : "生成订阅地址");
  setButtonBusy(generateButton, false);
  loadButton.disabled = false;
  setButtonLabel(loadButton, "载入配置");
  setButtonBusy(loadButton, false);
  editingNote.hidden = !editingSubscription;
  form.removeAttribute("aria-busy");
  syncSaveMode();
  syncMobileAction();
}

function invalidate(event) {
  if (event.target === resultUrl || event.target === accessTokenInput) return;
  formDirty = true;
  clearFieldError(event.target);
  if (event.target === existingUrl || event.target.name === "subscription_mode") editingSubscription = undefined;
  // 输入变化即作废旧结果并终止异步任务，避免迟到的结果覆盖新配置。
  revision++;
  stopGeneration();
  downloadController?.abort();
  downloadController = undefined;
  resetDownloadButton();
  result.hidden = true;
  resultUrl.value = "";
  clearResultFeedback();
  generatedSubscription = undefined;
  status.hidden = true;
  clearLoadError();
  event.target.setCustomValidity?.("");
  event.target.removeAttribute("aria-invalid");
  syncSaveMode();
  syncMobileAction();
}

form.addEventListener("input", invalidate);
form.addEventListener("change", invalidate);
form.addEventListener("invalid", (event) => {
  event.target.setAttribute("aria-invalid", "true");
  showFieldError(event.target);
}, true);

const rulesPicker = initRulesPicker(form, invalidate);

loadButton.addEventListener("click", async () => {
  if (!isUpdateMode()) return;
  if (!existingUrl.value.trim() || !existingUrl.reportValidity()) {
    showLoadError("请先填写已有的 SubForge 订阅链接。");
    existingUrl.focus();
    return;
  }
  if (accessTokenRequired && !accessTokenInput.value.trim()) {
    showLoadError("请填写管理 token。");
    accessTokenInput.focus();
    return;
  }
  invalidate({ target: existingUrl });
  const current = revision;
  const controller = new AbortController();
  generationController = controller;
  loadButton.disabled = true;
  setButtonLabel(loadButton, "正在载入…");
  setButtonBusy(loadButton, true);
  generateButton.disabled = true;
  form.setAttribute("aria-busy", "true");
  try {
    const output = await loadSubscription(accessTokenInput.value, existingUrl.value, controller.signal);
    if (current !== revision) return;
    const config = output.config;
    form.elements.source.value = config.source_url;
    form.elements.converter.value = config.converter_url;
    form.elements.filename.value = config.filename || "";
    rulesPicker.setUrl(config.policy.rules.source_url);
    for (const name of ["provider", "emoji", "tfo", "scv", "fdn", "expand", "sort", "new_name"]) {
      form.elements[name].checked = config.policy.converter[name];
    }
    setIntervalValue(config.policy.converter.interval_secs);
    for (const name of ["subscription", "preprocess_rules", "postprocess_config"]) {
      form.elements[name].value = config.scripts[name];
    }
    for (const element of form.elements) {
      element.setCustomValidity?.("");
      element.removeAttribute("aria-invalid");
    }
    editingSubscription = { id: output.id, url: output.url };
    existingUrl.value = output.url;
    formDirty = false;
    clearLoadError();
    clearAllFieldErrors();
    scrollTo(document.querySelector("#subscription-heading"));
  } catch (error) {
    if (current === revision) showLoadError(error.message);
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
          ...Object.fromEntries(["provider", "emoji", "tfo", "scv", "fdn", "expand", "sort", "new_name"].map(name => [name, data.has(name)])),
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
  resetDownloadButton();
  const current = ++revision;
  result.hidden = true;
  resultUrl.value = "";
  generatedSubscription = undefined;
  const input = readForm();
  const target = isUpdateMode() ? editingSubscription : undefined;
  generateButton.disabled = true;
  loadButton.disabled = true;
  setButtonLabel(generateButton, target ? "正在保存…" : "正在生成…");
  setButtonBusy(generateButton, true);
  form.setAttribute("aria-busy", "true");
  status.hidden = true;
  syncMobileAction();

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
      showFieldError(element);
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
    formDirty = false;
    clearAllFieldErrors();
    stopGeneration();
    generatedSubscription = output;
    resultTitle.textContent = target ? "配置已保存" : "订阅地址已生成";
    resultSubtitle.textContent = target
      ? "订阅地址保持不变，客户端下次更新整份订阅后生效。"
      : "复制到 Clash / Mihomo 客户端使用，或下载当前配置。";
    clearResultFeedback();
    status.hidden = true;
    resultUrl.value = output.url;
    result.hidden = false;
    syncMobileAction();
    scrollTo(result);
    if (!target) copyResultUrl();
  } catch (error) {
    if (current !== revision) return;
    fail(error.message, error.field);
  }
});

document.querySelector("#copy-button").addEventListener("click", copyResultUrl);

downloadButton.addEventListener("click", async () => {
  if (!generatedSubscription || downloadButton.disabled) return;
  const current = revision;
  const controller = new AbortController();
  downloadController = controller;
  resetDownloadButton();
  downloadButton.disabled = true;
  setButtonLabel(downloadButton, "正在转换…");
  setButtonBusy(downloadButton, true);
  clearResultFeedback();
  try {
    const download = await downloadConfiguration(generatedSubscription.id, controller.signal);
    if (current !== revision) return;
    const url = URL.createObjectURL(download.blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = download.filename || "sub-forge.yaml";
    link.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
    flashDownloadButton();
  } catch (error) {
    if (current !== revision) return;
    showResultFeedback(error.message, true);
  } finally {
    if (current === revision) {
      downloadController = undefined;
      downloadButton.disabled = false;
      setButtonLabel(downloadButton, "下载配置");
      setButtonBusy(downloadButton, false);
    }
  }
});

tokenToggle.addEventListener("click", () => {
  const visible = accessTokenInput.type === "text";
  accessTokenInput.type = visible ? "password" : "text";
  tokenToggle.setAttribute("aria-pressed", String(!visible));
  tokenToggle.setAttribute("aria-label", visible ? "显示管理 token" : "隐藏管理 token");
});

setIntervalValue(Number(intervalHidden.value) || 86400);
for (const radio of intervalPresets) radio.addEventListener("change", updateIntervalFromPreset);
intervalCustomValue.addEventListener("input", () => {
  intervalHidden.value = intervalCustomValue.value;
});
syncSaveMode();
syncMobileAction();

qrButton.addEventListener("click", () => {
  const url = resultUrl.value;
  if (!url) return;
  const qr = globalThis.QrCreator;
  if (!qr) {
    showResultFeedback("二维码组件未加载，请刷新页面重试。", true);
    return;
  }
  qrCode.replaceChildren();
  qr.render({
    text: url,
    radius: 0.35,
    ecLevel: "M",
    fill: "#18181b",
    background: "#ffffff",
    size: 220,
  }, qrCode);
  qrDialog.showModal();
});

window.addEventListener("beforeunload", (event) => {
  if (!formDirty) return;
  event.preventDefault();
  event.returnValue = "";
});

document.querySelector("#help-button").addEventListener("click", () => helpDialog.showModal());
