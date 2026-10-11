import { state, find } from './state.js';
import { els, $ } from './dom.js';
import { createItem, sendAction, fetchData } from './api.js';
import { text, clear, node, showToast, showDialogError } from './formatters.js';
import { PURIFY_CONFIG, markedRenderer, sanitizeHtml } from './markdown.js';
import { setRoute } from './router.js';
import { insertSnippetIntoTextarea } from './media.js';

export var SPEC_SNIPPETS = {
  ac: '## Acceptance Criteria\n- [ ] ',
  template: '## Problem & Context\n\n## Implementation Proposal\n\n## Acceptance Criteria\n- [ ] '
};

var activeModalConfig = null;

var callbacks = {
  openWorktreeModal: function () {},
  refreshData: function () {},
  ingest: function () {}
};

export function setFormCallbacks(cbs) {
  if (cbs) Object.assign(callbacks, cbs);
}

export function collectTicketFields(source) {
  if (source === 'quick-add') {
    return {
      title: els.quickAddInput ? els.quickAddInput.value.trim() : '',
      body: els.quickAddBody ? els.quickAddBody.value.trim() : '',
      type: els.quickAddType ? els.quickAddType.value : 'task',
      kind: els.quickAddKind ? els.quickAddKind.value : 'technical',
      priority: els.quickAddPriority && els.quickAddPriority.value ? els.quickAddPriority.value : undefined,
      tags: els.quickAddTags && els.quickAddTags.value.trim() ? els.quickAddTags.value.trim() : undefined,
      project: (state.scope && state.scope !== 'all') ? state.scope : undefined
    };
  }
  var projectVal = els.specModalProject ? els.specModalProject.value.trim() : '';
  return {
    title: els.specModalTitle ? els.specModalTitle.value.trim() : '',
    body: els.specModalBody ? els.specModalBody.value.trim() : '',
    type: els.specModalType ? els.specModalType.value : 'task',
    kind: els.specModalKind ? els.specModalKind.value : 'technical',
    priority: els.specModalPriority && els.specModalPriority.value ? els.specModalPriority.value : undefined,
    tags: els.specModalTags && els.specModalTags.value.trim() ? els.specModalTags.value.trim() : undefined,
    project: projectVal || ((state.scope && state.scope !== 'all') ? state.scope : undefined)
  };
}

export function convertIdeaToSpec(item) {
  var rev = item.revision;
  showToast('Converting to spec ticket...', false);
  return sendAction(item.id, { action: 'to_spec', expect_revision: rev })
    .then(function () {
      showToast('Converted to spec ticket', false);
    })
    .catch(function (err) {
      showToast('Conversion failed: ' + err.message, true);
    });
}

export function expandQuickAdd(focusTarget, defaultType) {
  if (els.quickAddTrigger) els.quickAddTrigger.hidden = true;
  if (els.quickIdeaTrigger) els.quickIdeaTrigger.hidden = true;
  if (els.quickAddForm) els.quickAddForm.hidden = false;
  if (defaultType && els.quickAddType) els.quickAddType.value = defaultType;
  if (focusTarget === 'body' && els.quickAddBody && els.quickAddBody.focus) {
    els.quickAddBody.focus();
  } else if (els.quickAddInput && els.quickAddInput.focus) {
    els.quickAddInput.focus();
  }
}

export function collapseQuickAdd() {
  if (els.quickAddForm) els.quickAddForm.hidden = true;
  if (els.quickAddTrigger) els.quickAddTrigger.hidden = false;
  if (els.quickIdeaTrigger) els.quickIdeaTrigger.hidden = false;
}

export function resetQuickAdd() {
  if (els.quickAddInput) els.quickAddInput.value = '';
  if (els.quickAddBody) els.quickAddBody.value = '';
  if (els.quickAddType) els.quickAddType.value = 'task';
  if (els.quickAddKind) els.quickAddKind.value = 'technical';
  if (els.quickAddPriority) els.quickAddPriority.value = '';
  if (els.quickAddTags) els.quickAddTags.value = '';
  collapseQuickAdd();
}

function setQuickAddDisabled(disabled) {
  if (els.quickAddInput) els.quickAddInput.disabled = disabled;
  if (els.quickAddBody) els.quickAddBody.disabled = disabled;
  if (els.quickAddType) els.quickAddType.disabled = disabled;
  if (els.quickAddKind) els.quickAddKind.disabled = disabled;
  if (els.quickAddPriority) els.quickAddPriority.disabled = disabled;
  if (els.quickAddTags) els.quickAddTags.disabled = disabled;
  if (els.quickAddSubmit) {
    els.quickAddSubmit.disabled = disabled;
    text(els.quickAddSubmit, disabled ? 'Creating…' : 'Create Ticket');
  }
}

export function submitQuickAdd() {
  var fields = collectTicketFields('quick-add');
  if (!fields.title) {
    if (els.quickAddInput) els.quickAddInput.focus();
    showToast('Title is required', true);
    return;
  }

  setQuickAddDisabled(true);
  createItem({
    title: fields.title,
    body: fields.body || undefined,
    type: fields.type,
    kind: fields.kind,
    priority: fields.priority,
    tags: fields.tags,
    status: 'created',
    project: fields.project
  })
  .then(function (newItem) {
    resetQuickAdd();
    setQuickAddDisabled(false);
    showToast('Created ' + (newItem.type || 'task') + ': ' + (newItem.title || fields.title), false);
    callbacks.refreshData();
    if (newItem && newItem.id) {
      setRoute(newItem.id);
    }
  })
  .catch(function (err) {
    setQuickAddDisabled(false);
    showToast(err.message || 'Failed to create task', true);
  });
}

function showSpecModalError(msg) {
  if (!els.specModalError) return;
  if (msg) {
    text(els.specModalError, msg);
    els.specModalError.hidden = false;
  } else {
    text(els.specModalError, '');
    els.specModalError.hidden = true;
  }
}

export function setSpecModalTab(tab) {
  if (tab === 'preview') {
    if (els.specTabWrite) els.specTabWrite.classList.remove('active');
    if (els.specTabPreview) els.specTabPreview.classList.add('active');
    if (els.specModalBody) els.specModalBody.hidden = true;
    if (els.specModalPreview) {
      els.specModalPreview.hidden = false;
      var raw = els.specModalBody ? els.specModalBody.value.trim() : '';
      if (!raw) {
        els.specModalPreview.innerHTML = '<p style="color: var(--muted); font-style: italic;">Nothing to preview yet. Write Markdown in the editor.</p>';
      } else {
        var rendered = '';
        if (typeof marked !== 'undefined' && marked && typeof marked.parse === 'function') {
          try {
            rendered = marked.parse(raw, { renderer: markedRenderer, gfm: true, async: false });
          } catch (_) {
            rendered = raw;
          }
        } else {
          rendered = raw;
        }
        els.specModalPreview.innerHTML = sanitizeHtml(rendered);
      }
    }
  } else {
    if (els.specTabWrite) els.specTabWrite.classList.add('active');
    if (els.specTabPreview) els.specTabPreview.classList.remove('active');
    if (els.specModalBody) els.specModalBody.hidden = false;
    if (els.specModalPreview) els.specModalPreview.hidden = true;
  }
}

export function openSpecModal(initial) {
  initial = initial || {};
  var activeProject = (state.scope && state.scope !== 'all') ? state.scope : '';
  if (els.specModalTitle) els.specModalTitle.value = initial.title || '';
  if (els.specModalBody) els.specModalBody.value = initial.body || '';
  if (els.specModalType) els.specModalType.value = initial.type || 'task';
  if (els.specModalKind) els.specModalKind.value = initial.kind || 'technical';
  if (els.specModalPriority) els.specModalPriority.value = initial.priority || '';
  if (els.specModalTags) els.specModalTags.value = initial.tags || '';
  if (els.specModalProject) els.specModalProject.value = initial.project || activeProject;

  setSpecModalTab('write');
  showSpecModalError('');

  if (els.specModal) {
    if (els.specModal.showModal) {
      els.specModal.showModal();
    } else {
      els.specModal.setAttribute('open', '');
    }
  }

  if (initial.title && els.specModalBody && els.specModalBody.focus) {
    els.specModalBody.focus();
  } else if (els.specModalTitle && els.specModalTitle.focus) {
    els.specModalTitle.focus();
  }
}

export function closeSpecModal() {
  showSpecModalError('');
  if (els.specModal) {
    if (els.specModal.close) {
      els.specModal.close();
    } else {
      els.specModal.removeAttribute('open');
    }
  }
}

function setSpecModalDisabled(disabled) {
  if (els.specModalTitle) els.specModalTitle.disabled = disabled;
  if (els.specModalBody) els.specModalBody.disabled = disabled;
  if (els.specModalType) els.specModalType.disabled = disabled;
  if (els.specModalKind) els.specModalKind.disabled = disabled;
  if (els.specModalPriority) els.specModalPriority.disabled = disabled;
  if (els.specModalTags) els.specModalTags.disabled = disabled;
  if (els.specModalProject) els.specModalProject.disabled = disabled;
  if (els.specModalSubmitBtn) {
    els.specModalSubmitBtn.disabled = disabled;
    text(els.specModalSubmitBtn, disabled ? 'Creating…' : 'Create Ticket');
  }
}

export function submitSpecModal() {
  var fields = collectTicketFields('spec-modal');
  if (!fields.title) {
    showSpecModalError('Title is required');
    if (els.specModalTitle) els.specModalTitle.focus();
    return;
  }

  showSpecModalError('');
  setSpecModalDisabled(true);

  createItem({
    title: fields.title,
    body: fields.body || undefined,
    type: fields.type,
    kind: fields.kind,
    priority: fields.priority,
    tags: fields.tags,
    status: 'created',
    project: fields.project
  })
  .then(function (newItem) {
    setSpecModalDisabled(false);
    closeSpecModal();
    showToast('Created ' + (newItem.type || 'task') + ': ' + (newItem.title || fields.title), false);
    callbacks.refreshData();
    if (newItem && newItem.id) {
      setRoute(newItem.id);
    }
  })
  .catch(function (err) {
    setSpecModalDisabled(false);
    showSpecModalError(err.message || 'Failed to create task');
  });
}

export function openActionModal(config) {
  activeModalConfig = config;
  showDialogError('');
  if (els.dialogTitle) text(els.dialogTitle, config.title || 'Action');
  if (els.dialogSubmitBtn) {
    text(els.dialogSubmitBtn, config.submitText || 'Confirm');
    els.dialogSubmitBtn.disabled = false;
  }
  if (els.dialogBody) {
    clear(els.dialogBody);
    if (config.render) config.render(els.dialogBody);
  }

  if (els.actionDialog) {
    if (els.actionDialog.showModal) {
      els.actionDialog.showModal();
    } else {
      els.actionDialog.setAttribute('open', '');
    }
  }

  if (els.dialogBody) {
    var firstField = els.dialogBody.querySelector('textarea, input, select');
    if (firstField && firstField.focus) firstField.focus();
  }
}

export function closeActionModal() {
  activeModalConfig = null;
  showDialogError('');
  if (els.dialogSubmitBtn) els.dialogSubmitBtn.disabled = false;
  if (els.actionDialog) {
    if (els.actionDialog.close) {
      els.actionDialog.close();
    } else {
      els.actionDialog.removeAttribute('open');
    }
  }
}

export function triggerModalSubmit() {
  if (!activeModalConfig || !activeModalConfig.onSubmit) return;
  showDialogError('');

  var maybePromise = activeModalConfig.onSubmit();
  if (maybePromise === false) return;

  if (maybePromise && typeof maybePromise.then === 'function') {
    if (els.dialogSubmitBtn) {
      els.dialogSubmitBtn.disabled = true;
      text(els.dialogSubmitBtn, 'Submitting…');
    }
    maybePromise
      .then(function () {
        closeActionModal();
        showToast('Action applied successfully', false);
      })
      .catch(function (err) {
        if (err && err.data && err.data.status === 'primary_busy') {
          var busyItem = state.lastAction ? find(state.lastAction.id) : null;
          var activeId = err.data.active_id || 'another task';
          closeActionModal();
          if (busyItem) {
            callbacks.openWorktreeModal(busyItem, activeId);
          } else {
            showToast('Primary checkout is busy running ' + activeId, true);
          }
          return;
        }
        var isConflict = err && (err.status === 409 || /revision conflict/i.test(err.message || '') || (err.data && /revision conflict/i.test(err.data.error || '')));
        if (isConflict && els.dialogError && state.lastAction) {
          var lastAction = state.lastAction;
          clear(els.dialogError);
          var wrap = node('div', 'dialog-conflict-actions');
          var msgSpan = node('span', 'conflict-msg', err.message || 'Revision conflict');
          var retryBtn = node('button', 'action-btn-small', 'Sync & Retry');
          retryBtn.type = 'button';
          retryBtn.id = 'btn-dialog-retry';
          retryBtn.addEventListener('click', function () {
            retryBtn.disabled = true;
            text(retryBtn, 'Syncing…');
            fetchData()
              .then(function (res) {
                if (!res.ok) throw new Error('Failed to refresh data (' + res.status + ')');
                return res.json();
              })
              .then(function (data) {
                if (data) callbacks.ingest(data);
                var latestItem = find(lastAction.id);
                if (!latestItem) throw new Error('Item not found in vault after sync');
                lastAction.payload.expect_revision = latestItem.revision;
                return sendAction(lastAction.id, lastAction.payload);
              })
              .then(function () {
                closeActionModal();
                showToast('Action applied successfully', false);
              })
              .catch(function (retryErr) {
                showDialogError(retryErr.message || 'Retry failed');
              });
          });
          wrap.appendChild(msgSpan);
          wrap.appendChild(retryBtn);
          els.dialogError.appendChild(wrap);
          els.dialogError.hidden = false;
        } else {
          showDialogError(err.message || 'Action failed');
        }
        if (els.dialogSubmitBtn) {
          els.dialogSubmitBtn.disabled = false;
          text(els.dialogSubmitBtn, activeModalConfig.submitText || 'Confirm');
        }
      });
  } else {
    closeActionModal();
  }
}

export function openCompleteModal(item) {
  var rev = item.revision;
  openActionModal({
    title: 'Complete: ' + (item.title || item.id),
    submitText: 'Complete with Evidence',
    render: function (body) {
      var label = node('label');
      label.appendChild(node('span', '', 'Verification evidence (required)'));
      label.appendChild(node('span', 'help-text', 'Paste test output, CLI command output, or logs demonstrating completion.'));
      var ta = node('textarea');
      ta.id = 'action-input-evidence';
      ta.required = true;
      ta.rows = 7;
      ta.placeholder = 'e.g. cargo test output, CLI smoke test, or verification proof...';
      label.appendChild(ta);
      body.appendChild(label);
    },
    onSubmit: function () {
      var ta = $('action-input-evidence');
      var val = ta ? ta.value.trim() : '';
      if (!val) {
        if (ta && ta.focus) ta.focus();
        showDialogError('Verification evidence is required.');
        return false;
      }
      return sendAction(item.id, { action: 'complete', evidence: val, expect_revision: rev });
    }
  });
}

export function openBlockedModal(item) {
  var rev = item.revision;
  openActionModal({
    title: 'Mark Blocked: ' + (item.title || item.id),
    submitText: 'Mark Blocked',
    render: function (body) {
      var label = node('label');
      label.appendChild(node('span', '', 'Blocker reason (required)'));
      label.appendChild(node('span', 'help-text', 'Specify what is blocking this item and what is needed to unblock it.'));
      var ta = node('textarea');
      ta.id = 'action-input-blocker';
      ta.required = true;
      ta.rows = 4;
      ta.placeholder = 'e.g. Waiting on upstream PR, missing credentials, or prerequisite task...';
      label.appendChild(ta);
      body.appendChild(label);
    },
    onSubmit: function () {
      var ta = $('action-input-blocker');
      var val = ta ? ta.value.trim() : '';
      if (!val) {
        if (ta && ta.focus) ta.focus();
        showDialogError('Blocker reason is required.');
        return false;
      }
      return sendAction(item.id, { action: 'transition', to: 'blocked', note: val, expect_revision: rev });
    }
  });
}

export function openCloseModal(item) {
  var rev = item.revision;
  openActionModal({
    title: 'Close: ' + (item.title || item.id),
    submitText: 'Close Item',
    render: function (body) {
      var label = node('label');
      label.appendChild(node('span', '', 'Closing note (optional)'));
      var ta = node('textarea');
      ta.id = 'action-input-close';
      ta.rows = 3;
      ta.placeholder = 'e.g. Shipped in release v1.2, verified by test suite...';
      label.appendChild(ta);
      body.appendChild(label);
    },
    onSubmit: function () {
      var ta = $('action-input-close');
      var val = ta ? ta.value.trim() : '';
      return sendAction(item.id, { action: 'close', note: val || undefined, expect_revision: rev });
    }
  });
}

export function openWorktreeModal(item, busyId) {
  state.pendingWorktreeItem = item;
  var targetId = item ? item.id : '';
  text(els.worktreeBusyId, busyId || 'another task');
  text(els.worktreeTargetId, targetId);
  text(els.worktreePromptTargetId, targetId);

  if (busyId) {
    if (els.worktreeModalTitle) text(els.worktreeModalTitle, 'Primary Working Tree Busy');
    if (els.worktreeBusyMsg) els.worktreeBusyMsg.hidden = false;
    if (els.worktreePromptMsg) els.worktreePromptMsg.hidden = true;
    if (els.btnWorktreePrimary) els.btnWorktreePrimary.hidden = true;
  } else {
    if (els.worktreeModalTitle) text(els.worktreeModalTitle, 'Start Agent Run');
    if (els.worktreeBusyMsg) els.worktreeBusyMsg.hidden = true;
    if (els.worktreePromptMsg) els.worktreePromptMsg.hidden = false;
    if (els.btnWorktreePrimary) els.btnWorktreePrimary.hidden = false;
  }
  if (els.worktreeModal) {
    if (els.worktreeModal.showModal) els.worktreeModal.showModal();
    else els.worktreeModal.setAttribute('open', '');
  }
}

export function closeWorktreeModal() {
  state.pendingWorktreeItem = null;
  if (els.worktreeModal) {
    if (els.worktreeModal.close) els.worktreeModal.close();
    else els.worktreeModal.removeAttribute('open');
  }
}
