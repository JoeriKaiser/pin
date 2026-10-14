import { state, find } from './state.js';
import { els, $ } from './dom.js';
import { sendCancel, sendRun, sendCommitPr, sendMessage, sendFinish, sendUpdate, sendSync } from './api.js';
import { text, showToast } from './formatters.js';
import { setRoute, applyRoute, setOnRouteChanged } from './router.js';
import { insertSnippetIntoTextarea, handleScreenshotPaste } from './media.js';
import { handleCardDrop, setViewMode, setBoardCallbacks } from './board.js';
import { renderList, renderDetail, setReaderTab, setDetailCallbacks } from './detail.js';
import {
  SPEC_SNIPPETS, collectTicketFields, expandQuickAdd, collapseQuickAdd, submitQuickAdd,
  openSpecModal, closeSpecModal, setSpecModalTab, submitSpecModal,
  closeActionModal, triggerModalSubmit, openCompleteModal, openBlockedModal, openCloseModal,
  convertIdeaToSpec, setFormCallbacks, openWorktreeModal, closeWorktreeModal
} from './forms.js';
import {
  openAgentDrawer, closeAgentDrawer, updateActiveRuns, toggleAllToolCards,
  openToolInspector, closeToolInspector, copyToolOutput, filterTrajectoryRows,
  appendTrajectoryUserRow, appendTrajectoryContextRow, setAgentCallbacks
} from './agent.js';

var bootCallbacks = {
  refreshData: function () {},
  ingest: function () {}
};

export function setBindCallbacks(cbs) {
  if (cbs) Object.assign(bootCallbacks, cbs);
}

export function sendRunItem(id, useWorktree, errorPrefix, optMode) {
  var mode = (optMode === 'idea' || (typeof errorPrefix === 'string' && errorPrefix.indexOf('Idea') !== -1)) ? 'idea' : undefined;
  showToast(mode === 'idea' ? 'Starting Idea agent...' : 'Starting agent...', false);
  var payload = useWorktree ? { use_worktree: true } : {};
  if (mode) payload.mode = mode;
  return sendRun(id, payload)
    .then(function (res) {
      if (!res.ok) {
        return res.json().then(function (err) {
          if (res.status === 409 && err.status === 'primary_busy') {
            var it = find(id);
            if (it) openWorktreeModal(it, err.active_id || 'another task');
            return null;
          }
          throw new Error(err.error || 'Failed to start agent');
        }).catch(function (pErr) {
          if (pErr) throw pErr;
        });
      }
      return res.json();
    })
    .then(function (data) {
      if (!data) return;
      showToast(mode === 'idea' ? 'Idea agent started' : 'Agent started', false);
      updateActiveRuns();
      openAgentDrawer(id);
      setReaderTab('trajectory');
    })
    .catch(function (err) {
      if (err) showToast(errorPrefix ? errorPrefix + (err.message || 'Failed to start agent') : (err.message || 'Failed to start agent'), true);
    });
}

export function sendCommitPrItem(id) {
  showToast('Starting Commit & PR agent...', false);
  return sendCommitPr(id)
    .then(function (res) {
      if (!res.ok) {
        return res.json().then(function (e) { throw new Error(e.error || 'Failed to start Commit & PR agent'); });
      }
      return res.json();
    })
    .then(function (data) {
      if (!data) return;
      showToast('Commit & PR agent started', false);
      updateActiveRuns();
      openAgentDrawer(id);
      setReaderTab('trajectory');
    })
    .catch(function (err) {
      if (err) showToast(err.message || 'Failed to start Commit & PR agent', true);
    });
}

export function sendAgentChatMessage(itemId) {
  if (!els.agentChatInput) return;
  var message = els.agentChatInput.value.trim();
  if (!message) return;
  if (els.btnAgentChatSend) els.btnAgentChatSend.disabled = true;
  if (els.agentChatInput) els.agentChatInput.disabled = true;

  appendTrajectoryUserRow(message);
  sendMessage(itemId, message)
    .then(function (res) {
      if (!res.ok) throw new Error('Failed to send message (' + res.status + ')');
      return res.json();
    })
    .then(function () {
      if (els.agentChatInput) {
        els.agentChatInput.value = '';
        els.agentChatInput.disabled = false;
        els.agentChatInput.focus();
      }
      if (els.btnAgentChatSend) els.btnAgentChatSend.disabled = false;
    })
    .catch(function (err) {
      showToast(err.message || 'Failed to send message', true);
      if (els.agentChatInput) els.agentChatInput.disabled = false;
      if (els.btnAgentChatSend) els.btnAgentChatSend.disabled = false;
    });
}

export function finishIdeaDiscussion(itemId) {
  if (els.btnAgentFinishIdea) els.btnAgentFinishIdea.disabled = true;
  showToast('Finalizing discussion…', false);
  sendFinish(itemId)
    .then(function (res) {
      if (!res.ok) throw new Error('Failed to finish (' + res.status + ')');
      return res.json();
    })
    .then(function () {
      showToast('Discussion finished. Generating spec…', false);
      appendTrajectoryContextRow('Finalizing idea discussion and updating proposal…');
      updateActiveRuns();
    })
    .catch(function (err) {
      showToast(err.message || 'Failed to finish discussion', true);
      if (els.btnAgentFinishIdea) els.btnAgentFinishIdea.disabled = false;
    });
}

export function handleUpdateInstall() {
  if (!els.btnUpdate) return;
  els.btnUpdate.disabled = true;
  els.btnUpdate.classList.add('updating');
  text(els.btnUpdate, 'Updating…');
  showToast('Downloading and installing pin update…', false);

  return sendUpdate()
    .then(function (res) {
      return res.json().then(function (data) {
        if (!res.ok) {
          throw new Error(data && data.error ? data.error : 'Update failed');
        }
        return data;
      });
    })
    .then(function (data) {
      els.btnUpdate.classList.remove('updating');
      els.btnUpdate.classList.add('updated');
      text(els.btnUpdate, 'Updated to v' + (data.updated_version || 'latest'));
      showToast('pin updated successfully! Restart viewer to apply.', false);
      if (els.updateFooter) {
        text(els.updateFooter, 'Updated to v' + (data.updated_version || 'latest') + '. Restart viewer.');
      }
      return data;
    })
    .catch(function (err) {
      els.btnUpdate.disabled = false;
      els.btnUpdate.classList.remove('updating');
      text(els.btnUpdate, 'Retry Update');
      showToast('Update failed: ' + (err.message || err), true);
      throw err;
    });
}

export function handleGitSync() {
  if (!els.btnSyncGit) return;
  els.btnSyncGit.disabled = true;
  els.btnSyncGit.classList.add('syncing');
  text(els.btnSyncGit, 'Syncing…');
  showToast('Syncing Git merge status…', false);

  return sendSync()
    .then(function (res) {
      if (!res.ok) throw new Error('Sync failed with status ' + res.status);
      return res.json();
    })
    .then(function (data) {
      els.btnSyncGit.disabled = false;
      els.btnSyncGit.classList.remove('syncing');
      text(els.btnSyncGit, 'Sync Git');
      var count = data.synced ? data.synced.length : 0;
      if (count > 0) {
        showToast('Synced ' + count + ' merged item' + (count === 1 ? '' : 's'), false);
      } else {
        showToast('Git sync complete: no merged items', false);
      }
      bootCallbacks.refreshData();
    })
    .catch(function (err) {
      els.btnSyncGit.disabled = false;
      els.btnSyncGit.classList.remove('syncing');
      text(els.btnSyncGit, 'Sync Git');
      showToast('Sync failed: ' + (err.message || err), true);
    });
}

export function bindNavigation() {
  if (els.viewBoardBtn) els.viewBoardBtn.addEventListener('click', function () { setViewMode('board'); });
  if (els.viewDetailBtn) els.viewDetailBtn.addEventListener('click', function () { setViewMode('detail'); });
  if (els.back) els.back.addEventListener('click', function () { setRoute(null); });
  if (els.btnNewTicket) els.btnNewTicket.addEventListener('click', function () { openSpecModal(); });
  if (els.btnUpdate) els.btnUpdate.addEventListener('click', handleUpdateInstall);
  if (els.btnSyncGit) els.btnSyncGit.addEventListener('click', handleGitSync);
  window.addEventListener('hashchange', applyRoute);
}

export function bindFilters() {
  var searchDebounceTimer = null;
  if (els.search) {
    els.search.addEventListener('input', function () {
      state.filters.text = this.value;
      clearTimeout(searchDebounceTimer);
      searchDebounceTimer = setTimeout(renderList, 40);
    });
  }
  if (els.status) els.status.addEventListener('change', function () { state.filters.status = this.value; renderList(); });
  if (els.type) els.type.addEventListener('change', function () { state.filters.type = this.value; renderList(); });
  if (els.kind) els.kind.addEventListener('change', function () { state.filters.kind = this.value; renderList(); });
  if (els.priority) els.priority.addEventListener('change', function () { state.filters.priority = this.value; renderList(); });
  if (els.project) els.project.addEventListener('input', function () { state.filters.project = this.value; renderList(); });
  if (els.filterToggle) {
    els.filterToggle.addEventListener('click', function () {
      var open = els.filters.hidden;
      els.filters.hidden = !open;
      this.setAttribute('aria-expanded', String(open));
    });
  }
  if (els.clear) {
    els.clear.addEventListener('click', function () {
      state.filters = { text: '', status: '', type: '', kind: '', priority: '', project: '' };
      if (els.search) els.search.value = '';
      if (els.status) els.status.value = '';
      if (els.type) els.type.value = '';
      if (els.kind) els.kind.value = '';
      if (els.priority) els.priority.value = '';
      if (els.project) els.project.value = '';
      renderList();
    });
  }
}

export function bindBoard() {
  document.querySelectorAll('.board-col-cards').forEach(function (container) {
    container.addEventListener('dragover', function (e) {
      e.preventDefault();
      if (e.dataTransfer) e.dataTransfer.dropEffect = 'move';
      container.classList.add('drag-over');
    });
    container.addEventListener('dragleave', function () {
      container.classList.remove('drag-over');
    });
    container.addEventListener('drop', function (e) {
      e.preventDefault();
      container.classList.remove('drag-over');
      var id = e.dataTransfer ? e.dataTransfer.getData('text/plain') : '';
      var status = container.dataset.dropStatus;
      if (id && status) handleCardDrop(id, status);
    });
  });
}

export function bindDetail() {
  if (els.tabTaskSpec) els.tabTaskSpec.addEventListener('click', function () { setReaderTab('spec'); });
  if (els.tabAgentTrajectory) els.tabAgentTrajectory.addEventListener('click', function () { setReaderTab('trajectory'); });
  if (els.dialogSubmitBtn) els.dialogSubmitBtn.addEventListener('click', triggerModalSubmit);
  if (els.dialogCloseBtn) els.dialogCloseBtn.addEventListener('click', closeActionModal);
  if (els.dialogCancelBtn) els.dialogCancelBtn.addEventListener('click', closeActionModal);
  if (els.dialogBody) {
    els.dialogBody.addEventListener('keydown', function (e) {
      if (e.key === 'Enter') {
        if (e.target.tagName === 'INPUT' || (e.target.tagName === 'TEXTAREA' && (e.ctrlKey || e.metaKey))) {
          e.preventDefault();
          triggerModalSubmit();
        }
      }
    });
  }
}

export function bindForms() {
  if (els.quickAddTrigger) els.quickAddTrigger.addEventListener('click', function () { expandQuickAdd('title'); });
  if (els.quickIdeaTrigger) els.quickIdeaTrigger.addEventListener('click', function () { expandQuickAdd('title', 'idea'); });
  if (els.quickAddCloseBtn) els.quickAddCloseBtn.addEventListener('click', collapseQuickAdd);
  if (els.quickAddCancel) els.quickAddCancel.addEventListener('click', collapseQuickAdd);
  if (els.quickAddExpandBtn) {
    els.quickAddExpandBtn.addEventListener('click', function () {
      var initial = collectTicketFields('quick-add');
      collapseQuickAdd();
      openSpecModal(initial);
    });
  }
  if (els.quickAddInput) {
    els.quickAddInput.addEventListener('keydown', function (e) {
      if (e.key === 'Enter' && !e.metaKey && !e.ctrlKey) {
        e.preventDefault();
        if (els.quickAddBody) els.quickAddBody.focus();
      }
    });
  }
  if (els.quickAddForm) {
    els.quickAddForm.addEventListener('submit', function (e) { e.preventDefault(); submitQuickAdd(); });
    els.quickAddForm.addEventListener('keydown', function (e) {
      if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) { e.preventDefault(); submitQuickAdd(); }
      else if (e.key === 'Escape') { e.preventDefault(); collapseQuickAdd(); }
    });
  }
  if (els.specModalCloseBtn) els.specModalCloseBtn.addEventListener('click', closeSpecModal);
  if (els.specModalCancelBtn) els.specModalCancelBtn.addEventListener('click', closeSpecModal);
  if (els.specTabWrite) els.specTabWrite.addEventListener('click', function () { setSpecModalTab('write'); });
  if (els.specTabPreview) els.specTabPreview.addEventListener('click', function () { setSpecModalTab('preview'); });
  if (els.specModalForm) {
    els.specModalForm.addEventListener('submit', function (e) { e.preventDefault(); submitSpecModal(); });
    els.specModalForm.addEventListener('keydown', function (e) {
      if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) { e.preventDefault(); submitSpecModal(); }
      else if (e.key === 'Escape') { e.preventDefault(); closeSpecModal(); }
    });
  }
  document.querySelectorAll('.snippet-btn').forEach(function (btn) {
    btn.addEventListener('click', function () {
      var snippet = SPEC_SNIPPETS[btn.dataset.insert];
      if (!snippet) return;
      var modalWrap = btn.closest('.spec-modal-body') || btn.closest('.spec-modal-content');
      if (modalWrap && els.specModalBody) {
        setSpecModalTab('write');
        insertSnippetIntoTextarea(els.specModalBody, snippet);
      } else if (els.quickAddBody) {
        insertSnippetIntoTextarea(els.quickAddBody, snippet);
      }
    });
  });
  if (els.quickAddBody) els.quickAddBody.addEventListener('paste', function (e) { handleScreenshotPaste(e, els.quickAddBody); });
  if (els.specModalBody) els.specModalBody.addEventListener('paste', function (e) { handleScreenshotPaste(e, els.specModalBody); });
  if (els.quickAddForm) {
    els.quickAddForm.addEventListener('paste', function (e) {
      if (e.target === els.quickAddBody || (e.target && /^(INPUT|SELECT)$/.test(e.target.tagName))) return;
      handleScreenshotPaste(e, els.quickAddBody);
    });
  }
  if (els.specModal) {
    els.specModal.addEventListener('paste', function (e) {
      if (e.target === els.specModalBody || (e.target && /^(INPUT|SELECT)$/.test(e.target.tagName))) return;
      setSpecModalTab('write');
      handleScreenshotPaste(e, els.specModalBody);
    });
  }
  if (els.btnWorktreeClose) els.btnWorktreeClose.addEventListener('click', closeWorktreeModal);
  if (els.btnWorktreeCancel) els.btnWorktreeCancel.addEventListener('click', closeWorktreeModal);
  if (els.btnWorktreePrimary) {
    els.btnWorktreePrimary.addEventListener('click', function () {
      var item = state.pendingWorktreeItem;
      closeWorktreeModal();
      if (item) sendRunItem(item.id, false);
    });
  }
  if (els.btnWorktreeConfirm) {
    els.btnWorktreeConfirm.addEventListener('click', function () {
      var item = state.pendingWorktreeItem;
      closeWorktreeModal();
      if (item) sendRunItem(item.id, true, 'Worktree start failed: ');
    });
  }
}

export function bindAgent() {
  if (els.btnCloseAgentDrawer) els.btnCloseAgentDrawer.addEventListener('click', closeAgentDrawer);
  if (els.btnToggleTools) els.btnToggleTools.addEventListener('click', toggleAllToolCards);
  if (els.btnCancelAgent) {
    els.btnCancelAgent.addEventListener('click', function () {
      var id = state.currentStreamingId || (els.agentItemId && (els.agentItemId.dataset.fullId || els.agentItemId.textContent.trim()));
      if (id) {
        sendCancel(id).then(function () {
          showToast('Agent cancelled', false);
          updateActiveRuns();
        }).catch(function (err) {
          showToast(err.message || 'Failed to cancel', true);
        });
      }
    });
  }
  if (els.btnAgentChatSend) {
    els.btnAgentChatSend.addEventListener('click', function () {
      if (state.currentStreamingId) sendAgentChatMessage(state.currentStreamingId);
    });
  }
  if (els.btnAgentFinishIdea) {
    els.btnAgentFinishIdea.addEventListener('click', function () {
      if (state.currentStreamingId) finishIdeaDiscussion(state.currentStreamingId);
    });
  }
  if (els.agentChatInput) {
    els.agentChatInput.addEventListener('keydown', function (e) {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        if (state.currentStreamingId) sendAgentChatMessage(state.currentStreamingId);
      }
    });
  }
  if (els.trajectorySearch) {
    els.trajectorySearch.addEventListener('input', function () {
      filterTrajectoryRows(this.value);
    });
  }
  if (els.trajectoryStream) {
    els.trajectoryStream.addEventListener('click', function (e) {
      var r = e.target && e.target.closest && e.target.closest('.trajectory-row-tool');
      if (r && r.dataset && r.dataset.callId) openToolInspector(r.dataset.callId);
    });
  }
  if (els.btnCloseInspector) els.btnCloseInspector.addEventListener('click', closeToolInspector);
  if (els.btnCopyToolOutput) els.btnCopyToolOutput.addEventListener('click', copyToolOutput);
}

export function bindGlobalShortcuts() {
  document.addEventListener('keydown', function (event) {
    if (els.specModal && (els.specModal.open || els.specModal.hasAttribute('open'))) {
      if (event.key === 'Escape') { event.preventDefault(); closeSpecModal(); }
      else if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) { event.preventDefault(); submitSpecModal(); }
      return;
    }
    if (els.actionDialog && (els.actionDialog.open || els.actionDialog.hasAttribute('open'))) {
      if (event.key === 'Escape') { event.preventDefault(); closeActionModal(); }
      else if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) { event.preventDefault(); triggerModalSubmit(); }
      return;
    }
    if (event.key === 'Escape') {
      if (els.quickAddForm && !els.quickAddForm.hidden) { event.preventDefault(); collapseQuickAdd(); return; }
      if (els.worktreeModal && (els.worktreeModal.open || els.worktreeModal.hasAttribute('open'))) { event.preventDefault(); closeWorktreeModal(); return; }
      if (els.toolInspector && !els.toolInspector.hidden) { event.preventDefault(); closeToolInspector(); return; }
      if (els.filters && !els.filters.hidden) {
        event.preventDefault(); els.filters.hidden = true;
        if (els.filterToggle) els.filterToggle.setAttribute('aria-expanded', 'false');
        return;
      }
      if (state.viewMode === 'detail') { event.preventDefault(); setViewMode('board'); return; }
      if (/^(INPUT|SELECT|TEXTAREA)$/.test(event.target.tagName)) { event.target.blur(); return; }
      if (state.selected) { setRoute(null); return; }
      return;
    }
    var input = /^(INPUT|SELECT|TEXTAREA)$/.test(event.target.tagName);
    if (event.key === '/' && !input) {
      event.preventDefault();
      if (els.search) els.search.focus();
      return;
    }
    var card = event.target && event.target.closest && event.target.closest('.board-card');
    if (card && (event.key === 'Enter' || /^Arrow/.test(event.key))) {
      if (event.key === 'Enter') {
        if (event.target === card || !/^(BUTTON|A|INPUT)$/.test(event.target.tagName)) { event.preventDefault(); card.click(); return; }
      }
      var colCards = card.closest('.board-col-cards') ? Array.from(card.closest('.board-col-cards').querySelectorAll('.board-card')) : [];
      var cIdx = colCards.indexOf(card);
      if (event.key === 'ArrowDown' && cIdx >= 0 && cIdx < colCards.length - 1) { event.preventDefault(); colCards[cIdx + 1].focus(); return; }
      if (event.key === 'ArrowUp' && cIdx > 0) { event.preventDefault(); colCards[cIdx - 1].focus(); return; }
    }
    if (input || event.metaKey || event.ctrlKey || event.altKey) return;
    if (event.key === 'n' || event.key === 'c') { event.preventDefault(); openSpecModal(); return; }
    if (event.key !== 'j' && event.key !== 'k' && event.key !== 'Enter') return;
    var idx = state.shown.findIndex(function (item) { return item.id === state.selected; });
    if (event.key === 'j' || event.key === 'k') {
      event.preventDefault();
      idx = Math.max(0, Math.min(state.shown.length - 1, (idx < 0 ? 0 : idx) + (event.key === 'j' ? 1 : -1)));
      if (state.shown[idx]) setRoute(state.shown[idx].id);
    } else if (event.key === 'Enter' && state.shown[idx]) {
      event.preventDefault();
      setRoute(state.shown[idx].id);
    }
  });
}

export function bind(options) {
  if (options) setBindCallbacks(options);

  // Wire callbacks into earlier modules
  setBoardCallbacks({
    openCompleteModal: openCompleteModal,
    openBlockedModal: openBlockedModal,
    openWorktreeModal: openWorktreeModal,
    openCloseModal: openCloseModal,
    openAgentDrawer: openAgentDrawer,
    sendCommitPrItem: sendCommitPrItem,
    refreshData: function () { bootCallbacks.refreshData(); }
  });

  setDetailCallbacks({
    sendRunItem: sendRunItem,
    convertIdeaToSpec: convertIdeaToSpec,
    openWorktreeModal: openWorktreeModal,
    openBlockedModal: openBlockedModal,
    openAgentDrawer: openAgentDrawer,
    openCompleteModal: openCompleteModal,
    sendCommitPrItem: sendCommitPrItem,
    openCloseModal: openCloseModal,
    refreshData: function () { bootCallbacks.refreshData(); }
  });

  setFormCallbacks({
    openWorktreeModal: openWorktreeModal,
    refreshData: function () { bootCallbacks.refreshData(); },
    ingest: function (data) { bootCallbacks.ingest(data); }
  });

  setAgentCallbacks({
    refreshData: function () { bootCallbacks.refreshData(); },
    renderBoard: renderList,
    renderDetail: renderDetail
  });

  setOnRouteChanged(function (selected) {
    if (selected && state.viewMode !== 'detail') {
      setViewMode('detail');
    }
    renderList();
    renderDetail();
    if (selected && typeof window !== 'undefined' && window.matchMedia && window.matchMedia('(max-width: 799.98px)').matches) {
      if (els.reader) {
        try { els.reader.focus({ preventScroll: true }); } catch (_) { els.reader.focus(); }
      }
    }
  });

  bindNavigation();
  bindFilters();
  bindBoard();
  bindDetail();
  bindForms();
  bindAgent();
  bindGlobalShortcuts();
}
