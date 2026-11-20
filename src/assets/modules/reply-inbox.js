import { state, find, getRunInfo } from './state.js';
import { els } from './dom.js';
import { sendMessage, sendFinish } from './api.js';
import { text, clear, node, showToast } from './formatters.js';
import { renderMarkdownSnippet } from './markdown.js';
import { handleScreenshotPaste } from './media.js';
import { getWaitingRuns, syncWaitingAlerts } from './alerts.js';
import { formatDuration } from './agents-view.js';

var activeReplyItemId = null;
var replyTicker = null;
var callbacks = {
  openAgentDrawer: function () {},
  updateActiveRuns: function () {},
  renderBoard: function () {},
  renderDetail: function () {},
  renderAgentsTable: function () {}
};

export function setReplyInboxCallbacks(cbs) {
  if (cbs) Object.assign(callbacks, cbs);
}

/**
 * Extracts top-level numbered items (1. ... 2. ...) from an assistant message
 * so the user can scaffold a matching numbered reply in one click.
 */
export function extractNumberedQuestions(markdownText) {
  if (!markdownText || typeof markdownText !== 'string') return [];
  var lines = markdownText.split(/\r?\n/);
  var found = [];
  var seenNums = Object.create(null);
  for (var i = 0; i < lines.length; i++) {
    var m = lines[i].match(/^\s*(\d{1,2})[.)]\s+(.+?)\s*$/);
    if (!m) continue;
    var num = parseInt(m[1], 10);
    if (num >= 1 && num <= 20 && !seenNums[num]) {
      seenNums[num] = true;
      found.push({ num: num, text: m[2] });
    }
  }
  if (found.length < 2) return [];
  // Require sequential numbering starting at 1 so incidental numbers don't trigger.
  if (found[0].num !== 1) return [];
  return found;
}

export function buildNumberedScaffold(questions) {
  if (!Array.isArray(questions) || !questions.length) return '';
  return questions.map(function (q) { return q.num + '. '; }).join('\n');
}

function saveCurrentDraft() {
  if (!activeReplyItemId || !els.replyModalInput) return;
  state.chatDrafts = state.chatDrafts || {};
  state.chatDrafts[activeReplyItemId] = els.replyModalInput.value;
}

function setReplyError(msg) {
  if (!els.replyModalError) return;
  if (msg) {
    text(els.replyModalError, msg);
    els.replyModalError.hidden = false;
    if (els.replyModalInput && els.replyModalInput.setAttribute) {
      els.replyModalInput.setAttribute('aria-invalid', 'true');
      els.replyModalInput.setAttribute('aria-describedby', 'reply-modal-error');
    }
  } else {
    text(els.replyModalError, '');
    els.replyModalError.hidden = true;
    if (els.replyModalInput && els.replyModalInput.removeAttribute) {
      els.replyModalInput.removeAttribute('aria-invalid');
      els.replyModalInput.removeAttribute('aria-describedby');
    }
  }
}

function stopReplyTicker() {
  if (replyTicker) {
    clearInterval(replyTicker);
    replyTicker = null;
  }
}

function updateWaitingDurationLabel() {
  if (!activeReplyItemId || !els.replyModalWaiting) return;
  var run = getRunInfo(activeReplyItemId);
  var since = (run && (run.waiting_since || run.started_at)) || 0;
  text(els.replyModalWaiting, since ? ('Waiting ' + formatDuration(since)) : 'Waiting for reply');
}

function setReplySubmitting(submitting) {
  if (els.replyModalInput) els.replyModalInput.disabled = submitting;
  if (els.btnReplySend) {
    els.btnReplySend.disabled = submitting;
    text(els.btnReplySend, submitting ? 'Sending…' : 'Send reply');
  }
  if (els.btnReplyFinish) els.btnReplyFinish.disabled = submitting;
}

function renderReplyQueue(waitingRuns) {
  if (!els.replyModalQueue) return;
  clear(els.replyModalQueue);
  if (waitingRuns.length <= 1) {
    els.replyModalQueue.hidden = true;
    return;
  }
  els.replyModalQueue.hidden = false;
  waitingRuns.forEach(function (run, idx) {
    var itm = find(run.item_id);
    var label = (itm && itm.title) ? itm.title : run.item_id.slice(0, 8);
    var isActive = run.item_id === activeReplyItemId;
    var btn = node('button', 'reply-queue-pill' + (isActive ? ' active' : ''));
    btn.type = 'button';
    btn.setAttribute('role', 'tab');
    btn.setAttribute('aria-selected', String(isActive));
    btn.setAttribute('tabindex', isActive ? '0' : '-1');
    btn.dataset.itemId = run.item_id;
    btn.appendChild(node('span', 'reply-queue-num', String(idx + 1)));
    btn.appendChild(node('span', 'reply-queue-label', label));
    btn.addEventListener('click', function () {
      saveCurrentDraft();
      renderReplyModalForItem(run.item_id);
    });
    els.replyModalQueue.appendChild(btn);
  });
}

export function renderQuickHelpersInto(container, inputEl, messageText, onDraftChange) {
  if (!container) return;
  clear(container);
  var questions = extractNumberedQuestions(messageText);

  if (questions.length >= 2) {
    var scaffoldBtn = node('button', 'snippet-btn reply-helper-chip', 'Scaffold ' + questions.length + ' answers');
    scaffoldBtn.type = 'button';
    scaffoldBtn.addEventListener('click', function () {
      if (!inputEl || inputEl.disabled) return;
      var scaffold = buildNumberedScaffold(questions);
      var current = inputEl.value.trim();
      inputEl.value = current ? (current + '\n\n' + scaffold) : scaffold;
      if (onDraftChange) onDraftChange();
      if (typeof inputEl.focus === 'function') inputEl.focus();
      if (typeof inputEl.setSelectionRange === 'function') {
        var pos = current ? (current.length + 2 + 3) : 3;
        inputEl.setSelectionRange(pos, pos);
      }
    });
    container.appendChild(scaffoldBtn);
  }

  var quickChoices = [
    { label: 'Agree with recommendations', text: 'I agree with your recommendations. Please proceed.' },
    { label: 'Proceed', text: 'Proceed.' },
    { label: 'Looks good', text: 'Looks good to me.' }
  ];
  quickChoices.forEach(function (choice) {
    var chip = node('button', 'snippet-btn reply-helper-chip', choice.label);
    chip.type = 'button';
    chip.addEventListener('click', function () {
      if (!inputEl || inputEl.disabled) return;
      var cur = inputEl.value.trim();
      inputEl.value = cur ? (cur + '\n' + choice.text) : choice.text;
      if (onDraftChange) onDraftChange();
      if (typeof inputEl.focus === 'function') inputEl.focus();
    });
    container.appendChild(chip);
  });
}

function renderQuickHelpers(messageText) {
  renderQuickHelpersInto(els.replyModalHelpers, els.replyModalInput, messageText, saveCurrentDraft);
}

export function renderReplyModalForItem(itemId) {
  var waitingRuns = getWaitingRuns();
  var run = null;
  if (itemId) {
    for (var i = 0; i < waitingRuns.length; i++) {
      if (waitingRuns[i].item_id === itemId) {
        run = waitingRuns[i];
        break;
      }
    }
    if (!run) run = getRunInfo(itemId);
  }
  if (!run && waitingRuns.length > 0) {
    run = waitingRuns[0];
  }
  if (!run) {
    closeReplyModal();
    return false;
  }

  activeReplyItemId = run.item_id;
  var itm = find(run.item_id);
  var title = (itm && itm.title) || run.item_id;

  setReplyError('');
  setReplySubmitting(false);
  renderReplyQueue(waitingRuns);

  if (els.replyModalTitle) text(els.replyModalTitle, title);
  if (els.replyModalId) text(els.replyModalId, run.item_id.slice(0, 8));
  if (els.replyModalMode) {
    var isIdea = run.mode === 'idea';
    text(els.replyModalMode, isIdea ? 'Idea' : 'Implement');
    els.replyModalMode.className = 'badge ' + (isIdea ? 'badge-mode-idea' : 'badge-mode-implement');
  }
  updateWaitingDurationLabel();

  var messageText = run.last_message || 'The agent paused and is waiting for your input. Open the full trajectory to inspect recent activity.';
  if (els.replyModalMessage) {
    clear(els.replyModalMessage);
    renderMarkdownSnippet(messageText, els.replyModalMessage);
  }

  renderQuickHelpers(run.last_message || '');

  if (els.btnReplyFinish) {
    text(els.btnReplyFinish, run.mode === 'implement' ? 'Finish & Complete' : 'Finish Discussion');
  }
  if (els.replyModalInput) {
    state.chatDrafts = state.chatDrafts || {};
    els.replyModalInput.value = state.chatDrafts[run.item_id] || '';
    if (typeof els.replyModalInput.focus === 'function') els.replyModalInput.focus();
  }
  return true;
}

export function isReplyModalOpen() {
  return !!(els.replyModal && (els.replyModal.open || els.replyModal.hasAttribute('open')));
}

export function stepReplyQueue(delta) {
  if (!isReplyModalOpen()) return false;
  var waitingRuns = getWaitingRuns();
  if (waitingRuns.length <= 1) return false;
  var idx = waitingRuns.findIndex(function (r) { return r.item_id === activeReplyItemId; });
  if (idx < 0) idx = 0;
  var nextIdx = (idx + delta + waitingRuns.length) % waitingRuns.length;
  saveCurrentDraft();
  renderReplyModalForItem(waitingRuns[nextIdx].item_id);
  return true;
}

export function syncReplyModalIfOpen() {
  if (!isReplyModalOpen()) return;
  var waitingRuns = getWaitingRuns();
  if (waitingRuns.length === 0) {
    closeReplyModal();
    return;
  }
  var stillWaiting = waitingRuns.some(function (r) { return r.item_id === activeReplyItemId; });
  if (!stillWaiting) {
    renderReplyModalForItem(waitingRuns[0].item_id);
    return;
  }
  // Keep current draft and focus untouched; only refresh queue pills and duration.
  renderReplyQueue(waitingRuns);
  updateWaitingDurationLabel();
}

export function openReplyModal(itemId) {
  var waitingRuns = getWaitingRuns();
  if (!itemId && waitingRuns.length === 0) {
    showToast('No prompts waiting for a reply', false);
    return false;
  }
  var ok = renderReplyModalForItem(itemId || (waitingRuns[0] && waitingRuns[0].item_id));
  if (!ok) {
    showToast('No prompts waiting for a reply', false);
    return false;
  }
  if (els.replyModal) {
    if (els.replyModal.showModal) els.replyModal.showModal();
    else els.replyModal.setAttribute('open', '');
  }
  stopReplyTicker();
  if (typeof setInterval !== 'undefined') {
    replyTicker = setInterval(updateWaitingDurationLabel, 1000);
  }
  if (els.replyModalInput && typeof els.replyModalInput.focus === 'function') {
    els.replyModalInput.focus();
  }
  return true;
}

export function closeReplyModal() {
  saveCurrentDraft();
  stopReplyTicker();
  activeReplyItemId = null;
  setReplyError('');
  if (els.replyModal) {
    if (els.replyModal.close) els.replyModal.close();
    else els.replyModal.removeAttribute('open');
  }
}

function advanceOrCloseAfterAction(handledItemId) {
  var run = getRunInfo(handledItemId);
  if (run) {
    run.waiting_for_user = false;
    run.waiting_since = null;
    run.last_activity = 'Running';
  }
  var remaining = syncWaitingAlerts({ notify: false });
  callbacks.renderBoard();
  callbacks.renderDetail();
  if (state.viewMode === 'agents') callbacks.renderAgentsTable();
  callbacks.updateActiveRuns();

  if (remaining.length > 0) {
    renderReplyModalForItem(remaining[0].item_id);
  } else {
    closeReplyModal();
  }
}

export function submitReplyModal() {
  if (!activeReplyItemId || !els.replyModalInput) return;
  var itemId = activeReplyItemId;
  var message = els.replyModalInput.value.trim();
  if (!message) {
    setReplyError('Write a reply before sending.');
    if (typeof els.replyModalInput.focus === 'function') els.replyModalInput.focus();
    return;
  }

  setReplyError('');
  setReplySubmitting(true);

  return sendMessage(itemId, message)
    .then(function (res) {
      if (!res.ok) {
        return res.json().catch(function () { return {}; }).then(function (d) {
          throw new Error((d && d.error) || ('Failed to send reply (' + res.status + ')'));
        });
      }
      return res.json();
    })
    .then(function () {
      if (state.chatDrafts) delete state.chatDrafts[itemId];
      if (els.replyModalInput) els.replyModalInput.value = '';
      setReplySubmitting(false);
      var itm = find(itemId);
      showToast('Reply sent to ' + ((itm && itm.title) || itemId.slice(0, 8)), false);
      advanceOrCloseAfterAction(itemId);
    })
    .catch(function (err) {
      setReplySubmitting(false);
      setReplyError((err && err.message) || 'Failed to send reply');
      if (els.replyModalInput && typeof els.replyModalInput.focus === 'function') {
        els.replyModalInput.focus();
      }
    });
}

export function finishFromReplyModal() {
  if (!activeReplyItemId) return;
  var itemId = activeReplyItemId;
  setReplyError('');
  setReplySubmitting(true);

  return sendFinish(itemId)
    .then(function (res) {
      if (!res.ok) {
        return res.json().catch(function () { return {}; }).then(function (d) {
          throw new Error((d && d.error) || ('Failed to finish run (' + res.status + ')'));
        });
      }
      return res.json();
    })
    .then(function () {
      if (state.chatDrafts) delete state.chatDrafts[itemId];
      setReplySubmitting(false);
      showToast('Finalizing interactive run…', false);
      advanceOrCloseAfterAction(itemId);
    })
    .catch(function (err) {
      setReplySubmitting(false);
      setReplyError((err && err.message) || 'Failed to finish run');
    });
}

export function bindReplyInboxUi() {
  if (els.btnReplyClose) els.btnReplyClose.addEventListener('click', closeReplyModal);
  if (els.btnReplySend) els.btnReplySend.addEventListener('click', submitReplyModal);
  if (els.btnReplyFinish) els.btnReplyFinish.addEventListener('click', finishFromReplyModal);
  if (els.btnReplyOpenTrajectory) {
    els.btnReplyOpenTrajectory.addEventListener('click', function () {
      var id = activeReplyItemId;
      closeReplyModal();
      if (id) callbacks.openAgentDrawer(id);
    });
  }
  if (els.replyModalInput) {
    els.replyModalInput.addEventListener('input', saveCurrentDraft);
    els.replyModalInput.addEventListener('paste', function (e) {
      handleScreenshotPaste(e, els.replyModalInput);
    });
    els.replyModalInput.addEventListener('keydown', function (e) {
      if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        submitReplyModal();
      }
    });
  }
}

export function getActiveReplyItemId() {
  return activeReplyItemId;
}
