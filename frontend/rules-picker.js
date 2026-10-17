export function initRulesPicker(form, onChange) {
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
      onChange({ target: rulesPreset });
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

  return { getUrl: () => form.elements.rules_mode.value === "preset" ? rulesTemplateLink.href : rulesUrl.value };
}
