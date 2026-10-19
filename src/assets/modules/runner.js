import { state } from './state.js';
import { els } from './dom.js';
import { fetchRunner, fetchRunnerModels, saveRunner } from './api.js';
import { clear, node, text, showToast } from './formatters.js';

var activeModelsRequest = null;
var activeModelsCommand = '';
var modelsRequestSeq = 0;

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

function visibleModels() {
  var runner = state.runner;
  var filterEl = els.runnerModelFilter;
  var filter = filterEl ? String(filterEl.value || '').toLowerCase().trim() : '';
  var selected = els.runnerModelSelect ? els.runnerModelSelect.value : '';
  var keep = selected || runner.model || '';
  var out = [];

  (runner.models || []).forEach(function (model) {
    var haystack = (model.value + ' ' + (model.name || '') + ' ' + (model.description || '')).toLowerCase();
    if (!filter || haystack.indexOf(filter) !== -1 || model.value === keep) {
      out.push(model);
    }
  });

  return out;
}

export function renderRunnerModelStatus() {
  var statusEl = els.runnerModelStatus;
  if (!statusEl) return;

  var runner = state.runner;
  var total = (runner.models || []).length;
  var visible = visibleModels().length;
  var message;

  if (runner.modelsError) {
    message = runner.modelsError;
  } else if (runner.loadingModels) {
    message = 'Loading models from ' + (runner.command || 'agent') + '…';
  } else if (!runner.modelsLoaded) {
    message = 'Open the model list to load options from ' + (runner.command || 'the agent') + '.';
  } else if (!total) {
    message = 'No models advertised by ' + (runner.modelsCommand || runner.command || 'this agent') + '. Runs use the agent default.';
  } else if (visible !== total) {
    message = visible + ' of ' + total + ' models loaded from ' + runner.modelsCommand + '.';
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

export function renderRunnerModelList() {
  var select = els.runnerModelSelect;
  if (!select) return;

  var runner = state.runner;
  var previous = select.value || runner.model || '';
  var models = visibleModels();

  clear(select);

  var defaultLabel = 'Agent default';
  if (runner.modelCurrent) {
    defaultLabel += ' (' + runnerModelLabel(runner.modelCurrent) + ')';
  }
  var defaultOption = node('option', '', defaultLabel);
  defaultOption.value = '';
  select.appendChild(defaultOption);

  models.forEach(function (model) {
    var option = node('option', '', (model.name || model.value) + '  ·  ' + model.value);
    option.value = model.value;
    if (model.description) option.title = model.description;
    select.appendChild(option);
  });

  if (previous && models.some(function (model) { return model.value === previous; })) {
    select.value = previous;
  } else if (runner.model && models.some(function (model) { return model.value === runner.model; })) {
    select.value = runner.model;
  } else {
    select.value = '';
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

  if (dialogOpen()) {
    if (els.runnerCommandInput) els.runnerCommandInput.value = runner.command;
    if (els.runnerActorInput) els.runnerActorInput.value = runner.actor;
    renderRunnerHarness();
    renderRunnerModelList();
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
      showToast(state.runner.error);
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
  renderRunnerModelStatus();

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
      renderRunnerModelStatus();
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
  if (els.runnerCommandInput) els.runnerCommandInput.value = runner.command || '';
  if (els.runnerActorInput) els.runnerActorInput.value = runner.actor || '';
  if (els.runnerModelFilter) els.runnerModelFilter.value = '';
  renderRunnerHarness();
  renderRunnerModelList();

  if (els.runnerDialog) {
    if (els.runnerDialog.showModal) els.runnerDialog.showModal();
    else if (els.runnerDialog.setAttribute) els.runnerDialog.setAttribute('open', '');
  }

  if (!runner.modelsLoaded && !runner.loadingModels) {
    loadRunnerModels(false);
  }
}

export function closeRunnerDialog() {
  if (!els.runnerDialog) return;
  if (els.runnerDialog.close) els.runnerDialog.close();
  else if (els.runnerDialog.removeAttribute) els.runnerDialog.removeAttribute('open');
  showRunnerError('');
}

function showRunnerError(message) {
  if (!els.runnerDialogError) return;
  if (message) {
    text(els.runnerDialogError, message);
    els.runnerDialogError.hidden = false;
  } else {
    text(els.runnerDialogError, '');
    els.runnerDialogError.hidden = true;
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
    showRunnerError('Agent command cannot be empty.');
    return Promise.resolve(null);
  }
  if (!actor) {
    showRunnerError('Vault actor cannot be empty.');
    return Promise.resolve(null);
  }

  var commandChanged = command !== state.runner.command;
  var selectedModel = els.runnerModelSelect ? (els.runnerModelSelect.value || null) : null;

  // If model discovery has not finished, the select only contains the
  // "Agent default" option. Preserve a CLI/env-seeded model instead of
  // silently clearing it just because the list is still loading.
  if (!state.runner.modelsLoaded && state.runner.model) {
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
        if (els.runnerModelFilter) els.runnerModelFilter.value = '';
        state.runner.modelsLoaded = false;
        renderRunnerModelList();
        return loadRunnerModels(false).then(function (data) {
          if (!data) return settings;
          if ((data.options || []).length) {
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

export function bindRunnerUi() {
  if (els.btnRunner) els.btnRunner.addEventListener('click', openRunnerDialog);
  if (els.btnRunnerClose) els.btnRunnerClose.addEventListener('click', closeRunnerDialog);
  if (els.btnRunnerCancel) els.btnRunnerCancel.addEventListener('click', closeRunnerDialog);
  if (els.btnRunnerSave) els.btnRunnerSave.addEventListener('click', saveRunnerSettingsFromDialog);
  if (els.btnRunnerRefreshModels) els.btnRunnerRefreshModels.addEventListener('click', refreshRunnerModels);
  if (els.runnerModelFilter) els.runnerModelFilter.addEventListener('input', renderRunnerModelList);
  if (els.runnerCommandInput) els.runnerCommandInput.addEventListener('input', onRunnerCommandInput);
  if (els.runnerHarnessOmp) {
    els.runnerHarnessOmp.addEventListener('click', function () { setRunnerHarness('omp'); });
  }
  if (els.runnerHarnessOpenCode) {
    els.runnerHarnessOpenCode.addEventListener('click', function () { setRunnerHarness('opencode'); });
  }
  if (els.runnerDialog) {
    els.runnerDialog.addEventListener('cancel', function (event) {
      if (event && event.preventDefault) event.preventDefault();
      closeRunnerDialog();
    });
  }
}
