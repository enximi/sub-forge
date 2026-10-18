const FAVORITES_KEY = "sub-forge.rules-favorites";

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
  const rulesSearch = document.querySelector("#rules-search");
  const rulesEmpty = document.querySelector("#rules-empty");
  const rulesFavoritesGroup = document.querySelector("#rules-favorites-group");
  const rulesFavoritesList = document.querySelector("#rules-favorites-list");
  const rulesFavorite = document.querySelector("#rules-favorite");
  const rulesChoices = Array.from(rulesPresetList.querySelectorAll('[role="option"]'));
  let selectedRulesPreset = rulesChoices.find(option => option.getAttribute("aria-selected") === "true");
  let favorites = readFavorites();

  const choiceUrl = (choice) => choice.querySelector(".rules-option-url")?.textContent || "";
  const allOptions = () => Array.from(rulesPresetList.querySelectorAll('[role="option"]'));
  const visibleOptions = () => allOptions().filter(option => !option.hidden && !option.closest('[role="group"]')?.hidden);

  function readFavorites() {
    try {
      const value = JSON.parse(localStorage.getItem(FAVORITES_KEY) || "[]");
      return Array.isArray(value) ? value.filter(url => typeof url === "string") : [];
    } catch {
      return [];
    }
  }

  function saveFavorites() {
    try {
      localStorage.setItem(FAVORITES_KEY, JSON.stringify(favorites));
    } catch {
      // 隐私模式或存储被禁用时，收藏在当前页面会话内仍然有效。
    }
  }

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

  function updateFavoriteButton() {
    const favorite = favorites.includes(choiceUrl(selectedRulesPreset));
    rulesFavorite.setAttribute("aria-pressed", String(favorite));
    rulesFavorite.textContent = favorite ? "★ 已收藏" : "☆ 收藏";
    rulesFavorite.title = favorite ? "取消收藏当前模板" : "收藏当前模板";
  }

  function updateRulesDescription() {
    rulesPresetValue.textContent = selectedRulesPreset.querySelector(".rules-option-name").textContent;
    rulesDescription.textContent = selectedRulesPreset.dataset.description;
    rulesTemplateLink.href = choiceUrl(selectedRulesPreset);
    updateFavoriteButton();
  }

  function markSelected(url) {
    for (const option of allOptions()) {
      option.setAttribute("aria-selected", String(choiceUrl(option) === url));
    }
  }

  function closeRulesPicker(restoreFocus = false) {
    rulesPresetList.hidden = true;
    rulesPreset.setAttribute("aria-expanded", "false");
    if (restoreFocus) rulesPreset.focus();
  }

  function focusChoice(index) {
    const choices = visibleOptions();
    if (!choices.length) {
      rulesSearch.focus();
      return;
    }
    const choice = choices[Math.max(0, Math.min(index, choices.length - 1))];
    choice.focus();
    choice.scrollIntoView({ block: "nearest" });
  }

  function applySearch() {
    const query = rulesSearch.value.trim().toLowerCase();
    for (const option of allOptions()) {
      option.hidden = Boolean(query) && !option.textContent.toLowerCase().includes(query);
    }
    for (const group of rulesPresetList.querySelectorAll('[role="group"]')) {
      group.hidden = !Array.from(group.querySelectorAll('[role="option"]')).some(option => !option.hidden);
    }
    // 当前键盘焦点所在的选项被过滤掉时，把焦点留在搜索框，避免列表意外关闭。
    if (document.activeElement?.matches?.('[role="option"]') && !visibleOptions().includes(document.activeElement)) {
      rulesSearch.focus();
    }
    rulesEmpty.hidden = visibleOptions().length > 0;
  }

  function renderFavorites() {
    rulesFavoritesList.replaceChildren();
    for (const url of favorites) {
      const source = rulesChoices.find(option => choiceUrl(option) === url);
      if (!source) continue;
      const clone = source.cloneNode(true);
      clone.dataset.favorite = "true";
      clone.setAttribute("aria-selected", String(choiceUrl(selectedRulesPreset) === url));
      rulesFavoritesList.append(clone);
    }
    rulesFavoritesGroup.hidden = rulesFavoritesList.children.length === 0;
  }

  function toggleFavorite(url) {
    const index = favorites.indexOf(url);
    if (index >= 0) favorites.splice(index, 1);
    else favorites.unshift(url);
    saveFavorites();
    renderFavorites();
    applySearch();
    updateFavoriteButton();
  }

  function selectOption(option) {
    const url = choiceUrl(option);
    const source = rulesChoices.find(choice => choiceUrl(choice) === url);
    if (!source) return;
    selectedRulesPreset = source;
    markSelected(url);
    updateRulesDescription();
    onChange({ target: rulesPreset });
    closeRulesPicker(true);
  }

  function openRulesPicker() {
    rulesPresetList.hidden = false;
    rulesPreset.setAttribute("aria-expanded", "true");
    rulesSearch.value = "";
    applySearch();
    const choices = visibleOptions();
    if (choices.includes(selectedRulesPreset)) selectedRulesPreset.focus();
    else if (choices.length) choices[0].focus();
    else rulesSearch.focus();
    (choices.includes(selectedRulesPreset) ? selectedRulesPreset : choices[0] || rulesSearch).scrollIntoView({ block: "nearest" });
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
  rulesPresetList.addEventListener("click", (event) => {
    const option = event.target.closest('[role="option"]');
    if (option) selectOption(option);
  });
  rulesSearch.addEventListener("input", applySearch);
  rulesSearch.addEventListener("keydown", (event) => {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      focusChoice(0);
    } else if (event.key === "Enter") {
      // 搜索框在表单内，默认 Enter 会提交整个订阅表单。
      event.preventDefault();
      const first = visibleOptions()[0];
      if (first) selectOption(first);
    } else if (event.key === "Escape") {
      event.preventDefault();
      closeRulesPicker(true);
    }
  });
  rulesPresetList.addEventListener("keydown", (event) => {
    if (event.target === rulesSearch) return;
    const choices = visibleOptions();
    const current = choices.indexOf(document.activeElement);
    let next;
    if (event.key === "ArrowDown") next = Math.min(current + 1, choices.length - 1);
    if (event.key === "ArrowUp") next = Math.max(current - 1, 0);
    if (event.key === "Home") next = 0;
    if (event.key === "End") next = choices.length - 1;
    if (next !== undefined) {
      event.preventDefault();
      focusChoice(next);
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
  rulesFavorite.addEventListener("click", () => toggleFavorite(choiceUrl(selectedRulesPreset)));

  for (const mode of rulesModes) mode.addEventListener("change", updateRulesMode);
  renderFavorites();
  applySearch();
  updateRulesDescription();
  updateRulesMode();

  return {
    getUrl: () => form.elements.rules_mode.value === "preset" ? rulesTemplateLink.href : rulesUrl.value,
    setUrl(url) {
      const choice = rulesChoices.find(option => choiceUrl(option) === url);
      form.elements.rules_mode.value = choice ? "preset" : "custom";
      if (choice) {
        selectedRulesPreset = choice;
        markSelected(url);
        updateRulesDescription();
      }
      rulesUrl.value = url;
      updateRulesMode();
    },
  };
}
