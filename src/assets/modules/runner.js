import { state } from './state.js';
import { els } from './dom.js';
import { fetchRunner, fetchRunnerModels, saveRunner } from './api.js';
import { clear, node, text, showToast } from './formatters.js';

var activeModelsRequest = null;
var activeModelsCommand = '';
var modelsRequestSeq = 0;

var modelDropdownOpen = false;
var modelFilterActive = false;
var modelFilterQuery = '';
var activeOptionIndex = -1;
var renderedOptions = [];

export function runnerModelLabel(value) {
  if (!value) return '';
  var models = state.runner.models || [];
  for (var i = 0; i < models.length; i += 1) {
    if (models[i].value === value) return models[i].name || value;
  }
  return value;
}

export function runnerSummaryText() {
  var runner = state.runner || {};
  var actor = runner.actor || 'agent:omp';
  var model = runner.model
    ? runnerModelLabel(runner.model)
    : (runner.modelCurrent
      ? 'default: ' + runnerModelLabel(runner.modelCurrent)
      : 'agent default');
  return actor + ' · ' + model;
}

export function runnerFullTitle() {
  var runner = state.runner || {};
  var title = 'Runner: ' + (runner.command || 'unknown command') +
    ' · ' + (runner.actor || 'agent:omp');
  if (runner.model) title += ' · ' + runner.model;
  return title;
}

export function harnessForCommand(command) {
  var value = String(command == null ? '' : command).trim();
  if (value === 'omp acp') return 'omp';
  if (value === 'opencode acp') return 'opencode';
  return 'custom';
}

function setHarnessPressed(el, pressed) {
  if (el && el.setAttribute) el.setAttribute('aria-pressed', pressed ? 'true' : 'false');
}

export function renderRunnerHarness() {
  var command = els.runnerCommandInput ? els.runnerCommandInput.value : state.runner.command;
  var harness = harnessForCommand(command);
  setHarnessPressed(els.runnerHarnessOmp, harness === 'omp');
  setHarnessPressed(els.runnerHarnessOpenCode, harness === 'opencode');
}

export function setRunnerHarness(kind) {
  if (!els.runnerCommandInput) return;
  if (kind === 'omp') els.runnerCommandInput.value = 'omp acp';
  else if (kind === 'opencode') els.runnerCommandInput.value = 'opencode acp';
  else return;
  onRunnerCommandInput();
}

function dialogOpen() {
  return !!(els.runnerDialog && (els.runnerDialog.open ||
    (els.runnerDialog.hasAttribute && els.runnerDialog.hasAttribute('open'))));
}

function selectedModelValue() {
  if (els.runnerModelSelect) return String(els.runnerModelSelect.value || '');
  return state.runner.model ? String(state.runner.model) : '';
}

function defaultModelLabel() {
  var runner = state.runner;
  var label = 'Agent default';
  if (runner.modelCurrent) {
    label += ' (' + runnerModelLabel(runner.modelCurrent) + ')';
  }
  return label;
}

function selectedModelDisplayText() {
  var selected = selectedModelValue();
  if (!selected) return defaultModelLabel();
  return runnerModelLabel(selected);
}

function activeSearchQuery() {
  if (!modelFilterActive) return '';
  return String(modelFilterQuery || '').trim();
}

function allSelectableModels() {
  var runner = state.runner;
  var models = (runner.models || []).slice();
  var selected = selectedModelValue();
  if (selected && !models.some(function (m) { return m.value === selected; })) {
    models.unshift({
      value: selected,
      name: selected,
      description: 'Configured outside the advertised model list',
      _custom: true
    });
  }
  return models;
}

function tokenizeQuery(query) {
  var q = String(query || '').toLowerCase().trim();
  if (!q) return [];
  return q.split(/\s+/).filter(Boolean);
}

function isWordBoundary(textStr, idx) {
  if (idx <= 0) return true;
  var ch = textStr.charAt(idx - 1);
  return ch === ' ' || ch === '/' || ch === '-' || ch === '_' || ch === '.';
}

function scoreFieldMatch(field, term) {
  if (!field) return 0;
  var idx = field.indexOf(term);
  if (idx === -1) return 0;
  if (field === term) return 120;
  if (idx === 0) return 60;
  var slashIdx = field.indexOf('/' + term);
  if (slashIdx !== -1) return 60;
  if (isWordBoundary(field, idx)) return 35;
  return 20;
}

function scoreModelMatch(model, terms, fullQuery) {
  var name = String(model.name || '').toLowerCase();
  var value = String(model.value || '').toLowerCase();
  var desc = String(model.description || '').toLowerCase();
  var combined = name + ' ' + value + ' ' + desc;
  var score = 0;

  for (var i = 0; i < terms.length; i += 1) {
    var term = terms[i];
    if (combined.indexOf(term) === -1) return -1;
    var best = Math.max(scoreFieldMatch(name, term), scoreFieldMatch(value, term));
    if (best > 0) score += best;
    else if (desc.indexOf(term) !== -1) score += 8;
  }

  if (fullQuery) {
    if (name.indexOf(fullQuery) !== -1) score += 40;
    else if (value.indexOf(fullQuery) !== -1) score += 30;
  }

  return score;
}

function filterModels(query) {
  var terms = tokenizeQuery(query);
  var models = allSelectableModels();
  if (!terms.length) return models;
  var fullQuery = terms.join(' ');
  var scored = [];
  for (var i = 0; i < models.length; i += 1) {
    var score = scoreModelMatch(models[i], terms, fullQuery);
    if (score >= 0) {
      scored.push({ model: models[i], score: score, index: i });
    }
  }
  scored.sort(function (a, b) {
    if (b.score !== a.score) return b.score - a.score;
    return a.index - b.index;
  });
  return scored.map(function (entry) { return entry.model; });
}

function defaultMatchesQuery(query) {
  var terms = tokenizeQuery(query);
  if (!terms.length) return true;
  var runner = state.runner;
  var haystack = ('agent default ' + (runner.modelCurrent || '') + ' ' +
    (runner.modelCurrent ? runnerModelLabel(runner.modelCurrent) : '')).toLowerCase();
  for (var i = 0; i < terms.length; i += 1) {
    if (haystack.indexOf(terms[i]) === -1) return false;
  }
  return true;
}

function appendHighlightedText(parent, rawText, terms) {
  var str = String(rawText == null ? '' : rawText);
  if (!terms || !terms.length || !str) {
    parent.appendChild(document.createTextNode(str));
    return;
  }
  var lower = str.toLowerCase();
  var ranges = [];
  for (var i = 0; i < terms.length; i += 1) {
    var term = terms[i];
    if (!term) continue;
    var pos = lower.indexOf(term);
    while (pos !== -1) {
      ranges.push([pos, pos + term.length]);
      pos = lower.indexOf(term, pos + term.length);
    }
  }
  if (!ranges.length) {
    parent.appendChild(document.createTextNode(str));
    return;
  }
  ranges.sort(function (a, b) { return a[0] - b[0] || b[1] - a[1]; });
  var merged = [ranges[0]];
  for (var j = 1; j < ranges.length; j += 1) {
    var prev = merged[merged.length - 1];
    if (ranges[j][0] <= prev[1]) {
      if (ranges[j][1] > prev[1]) prev[1] = ranges[j][1];
    } else {
      merged.push(ranges[j]);
    }
  }
  var cursor = 0;
  for (var k = 0; k < merged.length; k += 1) {
    var start = merged[k][0];
    var end = merged[k][1];
    if (start > cursor) {
      parent.appendChild(document.createTextNode(str.slice(cursor, start)));
    }
    parent.appendChild(node('mark', 'runner-model-highlight', str.slice(start, end)));
    cursor = end;
  }
  if (cursor < str.length) {
    parent.appendChild(document.createTextNode(str.slice(cursor)));
  }
}

function updateClearButton() {
  var clearBtn = els.runnerModelClear;
  if (!clearBtn) return;
  var searching = modelFilterActive && String(modelFilterQuery || '').length > 0;
  var hasSelection = selectedModelValue() !== '';
  clearBtn.hidden = !(searching || hasSelection);
  if (clearBtn.setAttribute) {
    clearBtn.setAttribute('aria-label', searching ? 'Clear model search' : 'Reset to agent default');
    clearBtn.setAttribute('title', searching ? 'Clear model search' : 'Reset to agent default');
  }
}

function syncComboboxExpanded() {
  var expanded = modelDropdownOpen ? 'true' : 'false';
  if (els.runnerModelFilter && els.runnerModelFilter.setAttribute) {
    els.runnerModelFilter.setAttribute('aria-expanded', expanded);
  }
  if (els.runnerModelToggle && els.runnerModelToggle.setAttribute) {
    els.runnerModelToggle.setAttribute('aria-expanded', expanded);
  }
  if (els.runnerModelListbox) {
    els.runnerModelListbox.hidden = !modelDropdownOpen;
  }
  if (els.runnerModelCombobox && els.runnerModelCombobox.classList) {
    if (modelDropdownOpen) els.runnerModelCombobox.classList.add('is-open');
    else els.runnerModelCombobox.classList.remove('is-open');
  }
}

function setActiveOption(index, scrollIntoView) {
  if (!renderedOptions.length) {
    activeOptionIndex = -1;
    if (els.runnerModelFilter && els.runnerModelFilter.removeAttribute) {
      els.runnerModelFilter.removeAttribute('aria-activedescendant');
    }
    return;
  }
  var clamped = ((index % renderedOptions.length) + renderedOptions.length) % renderedOptions.length;
  activeOptionIndex = clamped;
  for (var i = 0; i < renderedOptions.length; i += 1) {
    var entry = renderedOptions[i];
    var isActive = i === clamped;
    if (entry.el && entry.el.classList) {
      if (isActive) entry.el.classList.add('is-active');
      else entry.el.classList.remove('is-active');
    }
  }
  var activeEntry = renderedOptions[clamped];
  if (els.runnerModelFilter && els.runnerModelFilter.setAttribute) {
    els.runnerModelFilter.setAttribute('aria-activedescendant', activeEntry.id);
  }
  if (scrollIntoView && activeEntry.el && activeEntry.el.scrollIntoView) {
    activeEntry.el.scrollIntoView({ block: 'nearest' });
  }
}

export function isRunnerModelListboxOpen() {
  return modelDropdownOpen;
}

export function openRunnerModelListbox() {
  modelDropdownOpen = true;
  renderRunnerModelList();
  syncComboboxExpanded();
  var selected = selectedModelValue();
  var idx = 0;
  for (var i = 0; i < renderedOptions.length; i += 1) {
    if (renderedOptions[i].value === selected) {
      idx = i;
      break;
    }
  }
  setActiveOption(renderedOptions.length ? idx : -1, true);
  if (els.runnerModelListbox && els.runnerModelListbox.scrollIntoView) {
    els.runnerModelListbox.scrollIntoView({ block: 'nearest' });
  }
}

export function closeRunnerModelListbox(restoreFocus) {
  if (!modelDropdownOpen && !modelFilterActive) return;
  modelDropdownOpen = false;
  modelFilterActive = false;
  modelFilterQuery = '';
  activeOptionIndex = -1;
  if (els.runnerModelFilter) {
    els.runnerModelFilter.value = selectedModelDisplayText();
    if (els.runnerModelFilter.removeAttribute) {
      els.runnerModelFilter.removeAttribute('aria-activedescendant');
    }
    if (restoreFocus && els.runnerModelFilter.focus) {
      els.runnerModelFilter.focus();
    }
  }
  syncComboboxExpanded();
  updateClearButton();
  renderRunnerModelStatus();
}

export function selectRunnerModel(value) {
  var nextValue = value ? String(value) : '';
  if (els.runnerModelSelect) els.runnerModelSelect.value = nextValue;
  modelDropdownOpen = false;
  modelFilterActive = false;
  modelFilterQuery = '';
  activeOptionIndex = -1;
  if (els.runnerModelFilter) {
    els.runnerModelFilter.value = selectedModelDisplayText();
    if (els.runnerModelFilter.removeAttribute) {
      els.runnerModelFilter.removeAttribute('aria-activedescendant');
    }
  }
  syncComboboxExpanded();
  updateClearButton();
  renderRunnerModelList();
}

export function renderRunnerModelStatus() {
  var statusEl = els.runnerModelStatus;
  if (!statusEl) return;

  var runner = state.runner;
  var total = (runner.models || []).length;
  var query = activeSearchQuery();
  var matched = filterModels(query).length;
  var includeDefault = defaultMatchesQuery(query);
  var message;

  if (runner.modelsError) {
    message = runner.modelsError;
  } else if (runner.loadingModels) {
    message = 'Loading models from ' + (runner.command || 'agent') + '…';
  } else if (!runner.modelsLoaded) {
    message = 'Open the model list to load options from ' + (runner.command || 'the agent') + '.';
  } else if (!total) {
    message = 'No models advertised by ' + (runner.modelsCommand || runner.command || 'this agent') + '. Runs use the agent default.';
  } else if (query) {
    if (matched === 0 && !includeDefault) {
      message = 'No models match "' + query + '".';
    } else if (matched === 0) {
      message = 'Agent default matches "' + query + '".';
    } else {
      message = matched + ' of ' + total + ' models match "' + query + '".';
    }
  } else {
    message = total + ' models loaded from ' + runner.modelsCommand;
    if (runner.modelCurrent) {
      message += ' · agent default: ' + runnerModelLabel(runner.modelCurrent);
    }
    message += '.';
  }

  text(statusEl, message);
  if (statusEl.classList) {
    if (runner.modelsError) statusEl.classList.add('error');
    else statusEl.classList.remove('error');
  }
}

export function renderRunner() {
  if (els.runnerSummary) text(els.runnerSummary, runnerSummaryText());
  if (els.btnRunner) els.btnRunner.title = runnerFullTitle();
  renderRunnerModelStatus();
}

function buildOptionElement(item, index, selectedValue, currentDefault, terms) {
  var optionId = 'runner-model-opt-' + index;
  var isSelected = item.value === selectedValue;
  var el = node('div', 'runner-model-option' + (isSelected ? ' is-selected' : ''));
  el.id = optionId;
  el.setAttribute('role', 'option');
  el.setAttribute('aria-selected', isSelected ? 'true' : 'false');
  el.dataset.value = item.value;

  var mainRow = node('div', 'runner-model-option-main');
  var nameWrap = node('div', 'runner-model-option-title');
  var nameSpan = node('span', 'runner-model-option-name');
  appendHighlightedText(nameSpan, item.name || item.value, terms);
  nameWrap.appendChild(nameSpan);

  if (item.isDefaultOption) {
    nameWrap.appendChild(node('span', 'runner-model-badge', 'Default'));
  } else if (item._custom) {
    nameWrap.appendChild(node('span', 'runner-model-badge', 'Current'));
  } else if (currentDefault && item.value === currentDefault) {
    nameWrap.appendChild(node('span', 'runner-model-badge', 'Agent default'));
  }

  mainRow.appendChild(nameWrap);
  if (isSelected) {
    var check = node('span', 'runner-model-check', '✓');
    check.setAttribute('aria-hidden', 'true');
    mainRow.appendChild(check);
  }
  el.appendChild(mainRow);

  if (item.meta) {
    var metaDiv = node('div', 'runner-model-option-id');
    appendHighlightedText(metaDiv, item.meta, terms);
    el.appendChild(metaDiv);
  }
  if (item.description) {
    var descDiv = node('div', 'runner-model-option-desc');
    appendHighlightedText(descDiv, item.description, terms);
    el.appendChild(descDiv);
  }

  return { id: optionId, value: item.value, el: el };
}

export function renderRunnerModelList() {
  var listbox = els.runnerModelListbox;
  var runner = state.runner;
  var selected = selectedModelValue();
  var query = activeSearchQuery();

  if (!modelFilterActive && els.runnerModelFilter) {
    els.runnerModelFilter.value = selectedModelDisplayText();
  }
  updateClearButton();

  if (!listbox) {
    renderRunnerModelStatus();
    return;
  }

  clear(listbox);
  renderedOptions = [];

  if (runner.loadingModels && !(runner.models || []).length) {
    listbox.appendChild(node('div', 'runner-model-empty', 'Loading models from ' + (runner.command || 'agent') + '…'));
    setActiveOption(-1, false);
    renderRunnerModelStatus();
    return;
  }

  var terms = tokenizeQuery(query);
  var models = filterModels(query);
  var includeDefault = defaultMatchesQuery(query);
  var optIndex = 0;

  if (includeDefault) {
    var defaultItem = {
      value: '',
      name: defaultModelLabel(),
      meta: runner.modelCurrent ? runner.modelCurrent : 'Uses the agent configured default',
      description: '',
      isDefaultOption: true
    };
    var defEntry = buildOptionElement(defaultItem, optIndex, selected, runner.modelCurrent, terms);
    listbox.appendChild(defEntry.el);
    renderedOptions.push(defEntry);
    optIndex += 1;
  }

  models.forEach(function (model) {
    var entry = buildOptionElement({
      value: model.value,
      name: model.name || model.value,
      meta: model.value,
      description: model.description || '',
      _custom: !!model._custom
    }, optIndex, selected, runner.modelCurrent, terms);
    listbox.appendChild(entry.el);
    renderedOptions.push(entry);
    optIndex += 1;
  });

  if (!renderedOptions.length) {
    var emptyWrap = node('div', 'runner-model-empty');
    emptyWrap.appendChild(node('span', '', 'No models match "' + query + '".'));
    var clearBtn = node('button', 'control text-button runner-model-empty-clear', 'Clear search');
    clearBtn.type = 'button';
    clearBtn.addEventListener('click', function () {
      modelFilterActive = true;
      modelFilterQuery = '';
      if (els.runnerModelFilter) {
        els.runnerModelFilter.value = '';
        if (els.runnerModelFilter.focus) els.runnerModelFilter.focus();
      }
      renderRunnerModelList();
      setActiveOption(0, false);
    });
    emptyWrap.appendChild(clearBtn);
    listbox.appendChild(emptyWrap);
    setActiveOption(-1, false);
  } else if (modelDropdownOpen) {
    var nextActive = activeOptionIndex;
    if (nextActive < 0 || nextActive >= renderedOptions.length) nextActive = 0;
    setActiveOption(nextActive, false);
  }

  renderRunnerModelStatus();
}

export function applyRunnerSettings(settings) {
  var runner = state.runner;
  var previousCommand = runner.command;
  var next = settings || {};

  runner.command = next.command || '';
  runner.actor = next.actor || 'agent:omp';
  runner.model = next.model || null;
  runner.loaded = true;
  runner.error = '';

  if (runner.command !== previousCommand && runner.modelsCommand !== runner.command) {
    runner.models = [];
    runner.modelCurrent = null;
    runner.modelsLoaded = false;
    runner.loadingModels = false;
    runner.modelsCommand = '';
    runner.modelsError = '';
  }

  if (els.runnerModelSelect && (!dialogOpen() || !modelDropdownOpen)) {
    els.runnerModelSelect.value = runner.model || '';
  }

  if (dialogOpen()) {
    var active = typeof document !== 'undefined' ? document.activeElement : null;
    if (els.runnerCommandInput && active !== els.runnerCommandInput) {
      els.runnerCommandInput.value = runner.command;
    }
    if (els.runnerActorInput && active !== els.runnerActorInput) {
      els.runnerActorInput.value = runner.actor;
    }
    renderRunnerHarness();
    renderRunnerModelList();
    if (runner.command && !runner.modelsLoaded && !runner.loadingModels) {
      loadRunnerModels(false);
    }
  }

  renderRunner();
}

export function loadRunnerSettings() {
  return fetchRunner()
    .then(function (res) {
      if (!res.ok) throw new Error('Runner settings unavailable (' + res.status + ')');
      return res.json();
    })
    .then(function (settings) {
      applyRunnerSettings(settings);
      return settings;
    })
    .catch(function (err) {
      state.runner.error = err.message || 'Runner settings unavailable';
      showToast(state.runner.error, true);
      renderRunner();
      return null;
    });
}

export function loadRunnerModels(refresh) {
  var runner = state.runner;
  if (!runner.command) return Promise.resolve(null);

  // A POST and its SSE broadcast can ask for the same command at nearly the
  // same time. Share the in-flight request instead of spawning two agents.
  if (!refresh && activeModelsRequest && activeModelsCommand === runner.command) {
    return activeModelsRequest;
  }

  var requestedCommand = runner.command;
  var requestSeq = modelsRequestSeq + 1;
  modelsRequestSeq = requestSeq;

  runner.loadingModels = true;
  runner.modelsError = '';
  renderRunnerModelList();

  var promise = fetchRunnerModels(!!refresh)
    .then(function (res) {
      if (!res.ok) {
        return res.json()
          .catch(function () { return {}; })
          .then(function (body) {
            throw new Error((body && body.error) || ('Could not load models (' + res.status + ')'));
          });
      }
      return res.json();
    })
    .then(function (data) {
      if (requestSeq !== modelsRequestSeq || state.runner.command !== requestedCommand) {
        return null;
      }
      runner.loadingModels = false;
      runner.models = Array.isArray(data.options) ? data.options : [];
      runner.modelCurrent = data.current || null;
      runner.modelsCommand = data.command || requestedCommand;
      runner.modelsLoaded = true;
      runner.modelsError = '';
      renderRunnerModelList();
      renderRunner();
      return data;
    })
    .catch(function (err) {
      if (requestSeq !== modelsRequestSeq || state.runner.command !== requestedCommand) {
        return null;
      }
      runner.loadingModels = false;
      runner.modelsLoaded = false;
      runner.modelsError = err.message || 'Could not load models';
      renderRunnerModelList();
      return null;
    })
    .then(function (result) {
      if (activeModelsRequest === promise) {
        activeModelsRequest = null;
        activeModelsCommand = '';
      }
      return result;
    });

  activeModelsRequest = promise;
  activeModelsCommand = requestedCommand;
  return promise;
}

export function openRunnerDialog() {
  var runner = state.runner;

  showRunnerError('');
  modelDropdownOpen = false;
  modelFilterActive = false;
  modelFilterQuery = '';
  activeOptionIndex = -1;

  if (els.runnerCommandInput) els.runnerCommandInput.value = runner.command || '';
  if (els.runnerActorInput) els.runnerActorInput.value = runner.actor || '';
  if (els.runnerModelSelect) els.runnerModelSelect.value = runner.model || '';
  if (els.runnerModelFilter) els.runnerModelFilter.value = selectedModelDisplayText();
  syncComboboxExpanded();
  renderRunnerHarness();
  renderRunnerModelList();

  if (els.runnerDialog) {
    if (els.runnerDialog.showModal) {
      if (!els.runnerDialog.open) els.runnerDialog.showModal();
    } else if (els.runnerDialog.setAttribute) {
      els.runnerDialog.setAttribute('open', '');
    }
  }

  if (!runner.modelsLoaded && !runner.loadingModels) {
    loadRunnerModels(false);
  }
}

export function closeRunnerDialog() {
  modelDropdownOpen = false;
  modelFilterActive = false;
  modelFilterQuery = '';
  activeOptionIndex = -1;
  syncComboboxExpanded();
  if (!els.runnerDialog) return;
  if (els.runnerDialog.close) els.runnerDialog.close();
  else if (els.runnerDialog.removeAttribute) els.runnerDialog.removeAttribute('open');
  showRunnerError('');
}

function setRunnerFieldInvalid(el, invalid) {
  if (!el) return;
  if (invalid && el.setAttribute) {
    el.setAttribute('aria-invalid', 'true');
    el.setAttribute('aria-describedby', 'runner-dialog-error');
  } else if (el.removeAttribute) {
    el.removeAttribute('aria-invalid');
    el.removeAttribute('aria-describedby');
  }
}

function showRunnerError(message) {
  if (!els.runnerDialogError) return;
  if (message) {
    text(els.runnerDialogError, message);
    els.runnerDialogError.hidden = false;
  } else {
    text(els.runnerDialogError, '');
    els.runnerDialogError.hidden = true;
    setRunnerFieldInvalid(els.runnerCommandInput, false);
    setRunnerFieldInvalid(els.runnerActorInput, false);
  }
}

export function onRunnerCommandInput() {
  renderRunnerHarness();
  var input = els.runnerCommandInput;
  var typed = input ? input.value.trim() : '';
  if (typed && typed !== state.runner.command) {
    if (els.runnerModelStatus) {
      text(els.runnerModelStatus, 'Save the new agent command to load its models.');
      if (els.runnerModelStatus.classList) els.runnerModelStatus.classList.remove('error');
    }
    return;
  }
  renderRunnerModelStatus();
}

export function refreshRunnerModels() {
  var input = els.runnerCommandInput;
  var typed = input ? input.value.trim() : '';
  if (typed && typed !== state.runner.command) {
    if (els.runnerModelStatus) {
      text(els.runnerModelStatus, 'Save the new agent command to load its models.');
      if (els.runnerModelStatus.classList) els.runnerModelStatus.classList.remove('error');
    }
    return Promise.resolve(null);
  }
  return loadRunnerModels(true);
}

export function saveRunnerSettingsFromDialog() {
  var command = els.runnerCommandInput ? els.runnerCommandInput.value.trim() : '';
  var actor = els.runnerActorInput ? els.runnerActorInput.value.trim() : '';

  if (!command) {
    showRunnerError('Enter an agent command before saving.');
    setRunnerFieldInvalid(els.runnerCommandInput, true);
    if (els.runnerCommandInput && els.runnerCommandInput.focus) els.runnerCommandInput.focus();
    return Promise.resolve(null);
  }
  if (!actor) {
    showRunnerError('Enter a vault actor before saving.');
    setRunnerFieldInvalid(els.runnerActorInput, true);
    if (els.runnerActorInput && els.runnerActorInput.focus) els.runnerActorInput.focus();
    return Promise.resolve(null);
  }

  var commandChanged = command !== state.runner.command;
  var selectedModel = els.runnerModelSelect ? (els.runnerModelSelect.value || null) : null;

  // If model discovery has not finished, preserve a CLI/env-seeded model
  // instead of silently clearing it while the list is still loading.
  if (!state.runner.modelsLoaded && state.runner.model && !selectedModel) {
    selectedModel = state.runner.model;
  }

  var payload = {
    command: command,
    actor: actor,
    model: commandChanged ? null : selectedModel
  };

  if (els.btnRunnerSave) els.btnRunnerSave.disabled = true;
  showRunnerError('');

  return saveRunner(payload)
    .then(function (res) {
      return res.json()
        .catch(function () { return {}; })
        .then(function (body) {
          if (!res.ok) throw new Error((body && body.error) || 'Could not save runner settings');
          return body;
        });
    })
    .then(function (settings) {
      applyRunnerSettings(settings);

      if (commandChanged) {
        showToast('Agent command updated');
        modelFilterActive = false;
        modelFilterQuery = '';
        if (els.runnerModelSelect) els.runnerModelSelect.value = '';
        state.runner.modelsLoaded = false;
        renderRunnerModelList();
        return loadRunnerModels(false).then(function (data) {
          if (!data) return settings;
          if ((data.options || []).length) {
            openRunnerModelListbox();
            if (els.runnerModelStatus) {
              text(els.runnerModelStatus, 'Agent switched. Choose a model, then save again.');
            }
          } else {
            // The new agent has no model choices, so there is nothing else
            // to pick. The command and actor were already saved.
            closeRunnerDialog();
          }
          return settings;
        });
      }

      showToast('Runner updated');
      closeRunnerDialog();
      return settings;
    })
    .catch(function (err) {
      showRunnerError(err.message || 'Could not save runner settings');
      return null;
    })
    .then(function (result) {
      if (els.btnRunnerSave) els.btnRunnerSave.disabled = false;
      return result;
    });
}

export function handleRunnerSettingsEvent(settings) {
  if (!settings) return;
  var previousCommand = state.runner.command;
  applyRunnerSettings(settings);
  if (state.runner.command !== previousCommand) {
    loadRunnerModels(false);
  }
}

function onModelFilterInput() {
  modelFilterActive = true;
  modelFilterQuery = els.runnerModelFilter ? els.runnerModelFilter.value : '';
  modelDropdownOpen = true;
  renderRunnerModelList();
  syncComboboxExpanded();
  setActiveOption(renderedOptions.length ? 0 : -1, false);
}

function onModelFilterKeydown(event) {
  var key = event.key;
  if (key === 'ArrowDown') {
    if (event.preventDefault) event.preventDefault();
    if (!modelDropdownOpen) {
      openRunnerModelListbox();
    } else {
      setActiveOption(activeOptionIndex + 1, true);
    }
    return;
  }
  if (key === 'ArrowUp') {
    if (event.preventDefault) event.preventDefault();
    if (!modelDropdownOpen) {
      openRunnerModelListbox();
      if (renderedOptions.length) setActiveOption(renderedOptions.length - 1, true);
    } else {
      setActiveOption(activeOptionIndex - 1, true);
    }
    return;
  }
  if (key === 'Home' && modelDropdownOpen && renderedOptions.length) {
    if (event.preventDefault) event.preventDefault();
    setActiveOption(0, true);
    return;
  }
  if (key === 'End' && modelDropdownOpen && renderedOptions.length) {
    if (event.preventDefault) event.preventDefault();
    setActiveOption(renderedOptions.length - 1, true);
    return;
  }
  if (key === 'Enter' && !event.metaKey && !event.ctrlKey) {
    if (event.preventDefault) event.preventDefault();
    if (modelDropdownOpen && activeOptionIndex >= 0 && renderedOptions[activeOptionIndex]) {
      selectRunnerModel(renderedOptions[activeOptionIndex].value);
    } else if (!modelDropdownOpen) {
      openRunnerModelListbox();
    }
    return;
  }
  if (key === 'Escape' && modelDropdownOpen) {
    if (event.preventDefault) event.preventDefault();
    if (event.stopPropagation) event.stopPropagation();
    closeRunnerModelListbox(true);
    return;
  }
  if (key === 'Tab' && modelDropdownOpen) {
    closeRunnerModelListbox(false);
  }
}

function onModelClearClick() {
  var searching = modelFilterActive && String(modelFilterQuery || '').length > 0;
  if (searching) {
    modelFilterActive = true;
    modelFilterQuery = '';
    if (els.runnerModelFilter) {
      els.runnerModelFilter.value = '';
      if (els.runnerModelFilter.focus) els.runnerModelFilter.focus();
    }
    if (!modelDropdownOpen) openRunnerModelListbox();
    else {
      renderRunnerModelList();
      setActiveOption(renderedOptions.length ? 0 : -1, false);
    }
    return;
  }
  selectRunnerModel('');
  if (els.runnerModelFilter && els.runnerModelFilter.focus) els.runnerModelFilter.focus();
}

function onModelToggleClick() {
  if (modelDropdownOpen) {
    closeRunnerModelListbox(true);
  } else {
    openRunnerModelListbox();
    if (els.runnerModelFilter && els.runnerModelFilter.focus) {
      els.runnerModelFilter.focus();
      if (!modelFilterActive && els.runnerModelFilter.select) els.runnerModelFilter.select();
    }
  }
}

var runnerUiBound = false;
export function bindRunnerUi() {
  if (runnerUiBound) return;
  runnerUiBound = true;
  if (els.btnRunner) els.btnRunner.addEventListener('click', openRunnerDialog);
  if (els.btnRunnerClose) els.btnRunnerClose.addEventListener('click', closeRunnerDialog);
  if (els.btnRunnerCancel) els.btnRunnerCancel.addEventListener('click', closeRunnerDialog);
  if (els.btnRunnerSave) els.btnRunnerSave.addEventListener('click', saveRunnerSettingsFromDialog);
  if (els.btnRunnerRefreshModels) els.btnRunnerRefreshModels.addEventListener('click', refreshRunnerModels);
  if (els.runnerModelFilter) {
    els.runnerModelFilter.addEventListener('input', onModelFilterInput);
    els.runnerModelFilter.addEventListener('keydown', onModelFilterKeydown);
    els.runnerModelFilter.addEventListener('focus', function () {
      if (!modelFilterActive && els.runnerModelFilter.select) {
        els.runnerModelFilter.select();
      }
    });
    els.runnerModelFilter.addEventListener('click', function () {
      if (!modelDropdownOpen) {
        openRunnerModelListbox();
        if (!modelFilterActive && els.runnerModelFilter.select) {
          els.runnerModelFilter.select();
        }
      }
    });
  }
  if (els.runnerModelClear) {
    els.runnerModelClear.addEventListener('mousedown', function (event) {
      if (event && event.preventDefault) event.preventDefault();
    });
    els.runnerModelClear.addEventListener('click', onModelClearClick);
  }
  if (els.runnerModelToggle) {
    els.runnerModelToggle.addEventListener('mousedown', function (event) {
      if (event && event.preventDefault) event.preventDefault();
    });
    els.runnerModelToggle.addEventListener('click', onModelToggleClick);
  }
  if (els.runnerModelListbox) {
    els.runnerModelListbox.addEventListener('mousedown', function (event) {
      // Keep focus in the combobox input while clicking an option.
      if (event && event.preventDefault) event.preventDefault();
    });
    els.runnerModelListbox.addEventListener('click', function (event) {
      var target = event && event.target && event.target.closest
        ? event.target.closest('[role="option"]')
        : null;
      if (!target) return;
      var value = target.dataset ? target.dataset.value : '';
      selectRunnerModel(value);
      if (els.runnerModelFilter && els.runnerModelFilter.focus) els.runnerModelFilter.focus();
    });
  }
  if (els.runnerCommandInput) els.runnerCommandInput.addEventListener('input', onRunnerCommandInput);
  if (els.runnerHarnessOmp) {
    els.runnerHarnessOmp.addEventListener('click', function () { setRunnerHarness('omp'); });
  }
  if (els.runnerHarnessOpenCode) {
    els.runnerHarnessOpenCode.addEventListener('click', function () { setRunnerHarness('opencode'); });
  }
  if (els.runnerDialog) {
    els.runnerDialog.addEventListener('click', function (event) {
      if (!modelDropdownOpen || !els.runnerModelCombobox) return;
      var target = event && event.target;
      if (target && els.runnerModelCombobox.contains && !els.runnerModelCombobox.contains(target)) {
        closeRunnerModelListbox(false);
      }
    });
    els.runnerDialog.addEventListener('cancel', function (event) {
      if (event && event.preventDefault) event.preventDefault();
      if (modelDropdownOpen) {
        closeRunnerModelListbox(true);
      } else {
        closeRunnerDialog();
      }
    });
  }
}
