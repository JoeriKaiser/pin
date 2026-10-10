(function () {
  'use strict';

  var rawBase = document.documentElement.dataset.base || '';
  var BASE = rawBase ? (rawBase.endsWith('/') ? rawBase : rawBase + '/') : '';

  var PURIFY_CONFIG = {
    ALLOWED_TAGS: ['p','br','strong','em','b','i','code','pre','blockquote','h1','h2','h3','h4','h5','h6','ul','ol','li','a','hr','table','thead','tbody','tr','th','td','del','ins','sub','sup','input'],
    ALLOWED_ATTR: ['href','title','class','type','checked','disabled','target','rel'],
    ALLOWED_URI_REGEXP: /^(?:(?:https?|mailto):|[^a-z]|[a-z+.\-]+(?:[^a-z+.\-:]|$))/i,
    FORBID_TAGS: ['script','iframe','object','embed','form','button','textarea','select','style','link','meta','base','img','svg','math'],
    FORBID_ATTR: ['style','src','srcdoc','onerror','onclick','onload','onfocus','onblur','onsubmit'],
    KEEP_CONTENT: true
  };

  var ORDER = {
    status: ['created', 'planned', 'in_progress', 'blocked', 'review', 'done', 'closed', 'cancelled'],
    type: ['task', 'bug', 'idea', 'decision'],
    kind: ['technical', 'product', 'business', 'project', 'unspecified'],
    priority: ['high', 'medium', 'low', 'unset']
  };

  var $ = function (id) { return document.getElementById(id); };
  var els = {
    workspace: $('workspace'),
    viewBoardBtn: $('view-board-btn'),
    viewDetailBtn: $('view-detail-btn'),
    board: $('board'),
    countCreated: $('count-created'),
    countPlanned: $('count-planned'),
    countInProgress: $('count-in_progress'),
    countBlocked: $('count-blocked'),
    countDoneReview: $('count-done_review'),
    countClosed: $('count-closed'),
    btnNewTicket: $('btn-new-ticket'),
    quickAddContainer: $('quick-add-container'), quickAddTrigger: $('quick-add-trigger'),
    quickAddForm: $('quick-add-form'), quickAddInput: $('quick-add-input'),
    quickAddExpandBtn: $('quick-add-expand-btn'), quickAddCloseBtn: $('quick-add-close-btn'),
    quickAddBody: $('quick-add-body'), quickAddType: $('quick-add-type'), quickAddKind: $('quick-add-kind'),
    quickAddPriority: $('quick-add-priority'), quickAddTags: $('quick-add-tags'),
    quickAddCancel: $('quick-add-cancel'), quickAddSubmit: $('quick-add-submit'),
    specModal: $('spec-modal'), specModalForm: $('spec-modal-form'), specModalCloseBtn: $('spec-modal-close-btn'),
    specModalError: $('spec-modal-error'), specModalTitle: $('spec-modal-title'),
    specModalType: $('spec-modal-type'), specModalKind: $('spec-modal-kind'), specModalPriority: $('spec-modal-priority'),
    specModalProject: $('spec-modal-project'), specModalTags: $('spec-modal-tags'),
    specTabWrite: $('spec-tab-write'), specTabPreview: $('spec-tab-preview'),
    specModalBody: $('spec-modal-body'), specModalPreview: $('spec-modal-preview'),
    specModalCancelBtn: $('spec-modal-cancel-btn'), specModalSubmitBtn: $('spec-modal-submit-btn'),
    scope: $('scope'), search: $('search'), count: $('count'), filterToggle: $('filter-toggle'), filters: $('filters'),
    status: $('status-filter'), type: $('type-filter'), kind: $('kind-filter'), priority: $('priority-filter'),
    projectWrap: $('project-wrap'), project: $('project-filter'), projects: $('projects'), clear: $('clear-filters'),
    list: $('list'), listEmpty: $('list-empty'), reader: $('reader'), detailEmpty: $('detail-empty'), proposal: $('proposal'), back: $('back'),
    context: $('proposal-context'), title: $('proposal-title'), summary: $('proposal-summary'),
    workerCard: $('worker-card'), handoffCard: $('handoff-card'), dependenciesCard: $('dependencies-card'),
    actions: $('proposal-actions'),
    more: $('proposal-more'), meta: $('proposal-meta'), resolution: $('resolution'), body: $('proposal-body'),
    activitySection: $('activity-section'), activityList: $('activity-list'),
    snapshot: $('snapshot'), liveStatus: $('live-status'),
    actionDialog: $('action-dialog'), actionForm: $('action-form'), dialogTitle: $('dialog-title'),
    dialogError: $('dialog-error'), dialogBody: $('dialog-body'),
    dialogCloseBtn: $('dialog-close-btn'), dialogCancelBtn: $('dialog-cancel-btn'),
    dialogSubmitBtn: $('dialog-submit-btn'),
    toast: $('toast'),
    readerTabs: $('reader-tabs'),
    tabTaskSpec: $('tab-task-spec'),
    tabAgentTrajectory: $('tab-agent-trajectory'),
    readerTrajectoryBadge: $('reader-trajectory-badge'),
    agentDrawer: $('agent-drawer'),
    agentTaskTitle: $('agent-task-title'),
    agentItemId: $('agent-item-id'),
    agentStatusPill: $('agent-status-pill'),
    btnCancelAgent: $('btn-cancel-agent'),
    btnCloseAgentDrawer: $('btn-close-agent-drawer'),
    agentPlanList: $('agent-plan-list'),
    agentThoughts: $('agent-thoughts'),
    agentTools: $('agent-tools'),
    btnToggleTools: $('btn-toggle-tools'),
    agentLogInfo: $('agent-log-info'),
    trajectoryStream: $('agent-trajectory-stream'),
    metricDuration: $('agent-metric-duration'),
    metricTurns: $('agent-metric-turns'),
    metricCalls: $('agent-metric-calls'),
    trajectorySearch: $('agent-trajectory-search'),
    swimlaneSvg: $('agent-swimlane-svg'),
    toolInspector: $('agent-tool-inspector'),
    inspectorTitle: $('inspector-title'),
    inspectorStatus: $('inspector-status'),
    inspectorArgs: $('inspector-args'),
    inspectorOutput: $('inspector-output'),
    btnCloseInspector: $('btn-close-inspector'),
    btnCopyToolOutput: $('btn-copy-tool-output'),
    worktreeModal: $('worktree-modal'),
    worktreeModalTitle: $('worktree-modal-title'),
    worktreeBusyMsg: $('worktree-busy-msg'),
    worktreePromptMsg: $('worktree-prompt-msg'),
    worktreeBusyId: $('worktree-busy-id'),
    worktreeTargetId: $('worktree-target-id'),
    worktreePromptTargetId: $('worktree-prompt-target-id'),
    btnWorktreeCancel: $('btn-worktree-cancel'),
    btnWorktreeClose: $('btn-worktree-close'),
    btnWorktreePrimary: $('btn-worktree-primary'),
    btnWorktreeConfirm: $('btn-worktree-confirm')
  };

  var state = {
    viewMode: (function () {
      try { return localStorage.getItem('pin_view_mode') || 'board'; } catch (_) { return 'board'; }
    })(),
    readerTab: 'spec',
    items: [],
    shown: [],
    selected: null,
    scope: '',
    archive: '',
    captured: '',
    etag: null,
    filters: { text: '', status: '', type: '', kind: '', priority: '', project: '' },
    activeRuns: new Set(),
    currentStream: null,
    currentStreamingId: null,
    pendingWorktreeItem: null,
    trajectory: {
      startTime: 0,
      turns: 0,
      calls: 0,
      timer: null,
      activeAssistantRow: null,
      activeAssistantTextEl: null,
      intervals: { input: [], model: [], tools: [] },
      currentModelInterval: null,
      toolIntervals: {},
      toolCalls: {},
      filterQuery: ''
    }
  };

  var activeModalConfig = null;
  var toastTimer = null;

  function norm(value) { return String(value == null ? '' : value).toLowerCase(); }
  function text(node, value) { if (node) node.textContent = value == null ? '' : String(value); }
  function clear(node) {
    if (!node) return;
    if (typeof node.replaceChildren === 'function') {
      node.replaceChildren();
    } else {
      node.textContent = '';
    }
  }
  function node(tag, className, value) {
    var n = document.createElement(tag);
    if (className) n.className = className;
    if (value != null) text(n, value);
    return n;
  }

  function itemStatus(item) { return item.status || 'created'; }
  function itemType(item) { return item.type || 'task'; }
  function kind(item) { return item.kind || 'unspecified'; }
  function priority(item) { return item.priority || 'unset'; }
  function tags(item) { return Array.isArray(item.tags) ? item.tags.map(String) : []; }
  function isNarrow() { return (typeof window !== 'undefined' && window && window.matchMedia) ? window.matchMedia('(max-width: 799.98px)').matches : false; }

  function date(value, long) {
    var n = Number(value); if (!isFinite(n)) return '';
    if (n < 1e12) n *= 1000;
    var opts = long
      ? { year: 'numeric', month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }
      : { month: 'short', day: 'numeric' };
    try { return new Date(n).toLocaleDateString(undefined, opts); } catch (_) { return new Date(n).toISOString(); }
  }

  function timeAgo(value) {
    var n = Number(value); if (!isFinite(n)) return '';
    if (n < 1e12) n *= 1000;
    var diffSec = Math.floor((Date.now() - n) / 1000);
    if (diffSec < 60) return 'just now';
    if (diffSec < 3600) return Math.floor(diffSec / 60) + 'm ago';
    if (diffSec < 86400) return Math.floor(diffSec / 3600) + 'h ago';
    if (diffSec < 86400 * 30) return Math.floor(diffSec / 86400) + 'd ago';
    return date(value, false);
  }

  function route() {
    var hash = '';
    try {
      if (typeof location !== 'undefined' && location && location.hash) {
        hash = location.hash;
      }
    } catch (_) {}
    if (!hash) return state ? state.selected : null;
    var match = hash.match(/^#\/(?:idea|item)\/(.+)$/);
    if (!match) return null;
    try { return decodeURIComponent(match[1]); } catch (_) { return null; }
  }

  function setRoute(id) {
    var next = id ? '#/item/' + encodeURIComponent(id) : '#/';
    try {
      if (typeof location !== 'undefined' && location) {
        if (location.hash !== next) location.hash = next; else applyRoute();
        return;
      }
    } catch (_) {}
    state.selected = id;
    applyRoute();
  }

  function find(id) {
    for (var i = 0; i < state.items.length; i++) {
      if (state.items[i].id === id) return state.items[i];
    }
    return null;
  }

  function hasUnresolvedPrereqs(item) {
    if (!item.depends_on || !item.depends_on.length) return false;
    for (var i = 0; i < item.depends_on.length; i++) {
      var depId = item.depends_on[i];
      var depItem = find(depId);
      if (!depItem) return true;
      var s = itemStatus(depItem);
      if (s !== 'done' && s !== 'closed') return true;
    }
    return false;
  }

  function unique(pick) {
    var seen = Object.create(null), out = [];
    state.items.forEach(function (item) {
      var value = pick(item);
      if (value && !seen[value]) { seen[value] = true; out.push(value); }
    });
    return out;
  }

  function fillSelect(select, values, first, order) {
    var cur = select.value;
    clear(select);
    var base = node('option', '', first);
    base.value = '';
    select.appendChild(base);
    if (order) {
      values.sort(function (a, b) {
        var ia = order.indexOf(a), ib = order.indexOf(b);
        if (ia === -1 && ib === -1) return a.localeCompare(b);
        if (ia === -1) return 1;
        if (ib === -1) return -1;
        return ia - ib;
      });
    } else {
      values.sort();
    }
    values.forEach(function (value) {
      var option = node('option', '', value);
      option.value = value;
      select.appendChild(option);
    });
    select.value = cur;
  }

  function setupFilters() {
    fillSelect(els.status, unique(itemStatus), 'Any status', ORDER.status);
    fillSelect(els.type, unique(itemType), 'Any type', ORDER.type);
    fillSelect(els.kind, unique(kind), 'Any kind', ORDER.kind);
    fillSelect(els.priority, unique(priority), 'Any priority', ORDER.priority);

    var projects = unique(function (item) { return item.project || ''; }).filter(Boolean).sort();
    clear(els.projects);
    projects.forEach(function (value) {
      var option = node('option');
      option.value = value;
      els.projects.appendChild(option);
    });
    els.projectWrap.hidden = projects.length < 2;
  }

  function matches(item) {
    var f = state.filters, q = norm(f.text).trim();
    if (f.status && itemStatus(item) !== f.status) return false;
    if (f.type && itemType(item) !== f.type) return false;
    if (f.kind && kind(item) !== f.kind) return false;
    if (f.priority && priority(item) !== f.priority) return false;
    if (f.project && norm(item.project) !== norm(f.project).trim()) return false;
    if (!q) return true;
    var searchCorpus = [
      item.title, item.project, item.id, item.type, item.status,
      item.claimed_by, tags(item).join(' '), item.content || item.body
    ].join('\n');
    return norm(searchCorpus).indexOf(q) !== -1;
  }

  function setViewMode(mode) {
    state.viewMode = mode === 'detail' ? 'detail' : 'board';
    try { localStorage.setItem('pin_view_mode', state.viewMode); } catch (_) {}
    if (els.workspace) {
      els.workspace.classList.toggle('mode-board', state.viewMode === 'board');
      els.workspace.classList.toggle('mode-detail', state.viewMode === 'detail');
    }
    if (els.viewBoardBtn) els.viewBoardBtn.setAttribute('aria-pressed', String(state.viewMode === 'board'));
    if (els.viewDetailBtn) els.viewDetailBtn.setAttribute('aria-pressed', String(state.viewMode === 'detail'));
    if (state.viewMode === 'detail' && !state.selected && state.shown.length) {
      setRoute(state.shown[0].id);
    }
  }

  function showToast(message, isError) {
    if (!els.toast) return;
    if (toastTimer) clearTimeout(toastTimer);
    text(els.toast, message);
    els.toast.className = 'toast' + (isError ? ' toast-error' : ' toast-success');
    els.toast.hidden = false;
    toastTimer = setTimeout(function () {
      els.toast.hidden = true;
    }, 3500);
  }

  function escapeHtml(str) {
    return String(str == null ? '' : str)
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&#39;');
  }

  function showDialogError(message) {
    if (!els.dialogError) return;
    if (message) {
      text(els.dialogError, message);
      els.dialogError.hidden = false;
    } else {
      text(els.dialogError, '');
      els.dialogError.hidden = true;
    }
  }

  function sendAction(id, payload) {
    state.lastAction = { id: id, payload: payload };
    var url = BASE + 'items/' + encodeURIComponent(id) + '/action';
    return fetch(url, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'X-Pin-Action': 'true'
      },
      body: JSON.stringify(payload)
    })
    .then(function (res) {
      if (!res.ok) {
        return res.json().then(function (e) {
          var err = new Error(e.error || ('HTTP ' + res.status));
          err.status = res.status;
          err.data = e;
          throw err;
        }).catch(function (parseErr) {
          if (parseErr.data) throw parseErr;
          var err = new Error(parseErr.message || ('HTTP ' + res.status));
          err.status = res.status;
          throw err;
        });
      }
      return res.json();
    })
    .then(function (data) {
      refreshData();
       return data;
     });
   }

  function sendRunItem(id, useWorktree, errorPrefix) {
    showToast('Starting agent...', false);
    var payload = useWorktree ? { use_worktree: true } : {};
    return fetch(BASE + 'items/' + encodeURIComponent(id) + '/run', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'X-Pin-Action': 'true'
      },
      body: JSON.stringify(payload)
    })
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
      showToast('Agent started', false);
      updateActiveRuns();
      openAgentDrawer(id);
      setReaderTab('trajectory');
    })
    .catch(function (err) {
      if (!err) return;
      var message = err.message || 'Failed to start agent';
      showToast(errorPrefix ? errorPrefix + message : message, true);
    });
  }

  function sendCommitPrItem(id) {
    showToast('Starting Commit & PR agent...', false);
    return fetch(BASE + 'items/' + encodeURIComponent(id) + '/commit-pr', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'X-Pin-Action': 'true'
      }
    })
    .then(function (res) {
      if (!res.ok) {
        return res.json().then(function (err) {
          if (res.status === 409 && err.status === 'primary_busy') {
            var it = find(id);
            if (it) openWorktreeModal(it, err.active_id || 'another task');
            return null;
          }
          throw new Error(err.error || 'Failed to start Commit & PR agent');
        }).catch(function (pErr) {
          if (pErr) throw pErr;
        });
      }
      return res.json();
    })
    .then(function (data) {
      if (!data) return;
      showToast('Commit & PR agent started', false);
      updateActiveRuns();
      openAgentDrawer(id);
      setReaderTab('trajectory');
      refreshData();
    })
    .catch(function (err) {
      if (err) {
        showToast('Commit & PR failed: ' + err.message, true);
      }
    });
  }

  var SPEC_SNIPPETS = {
    ac: '## Acceptance Criteria\n- [ ] ',
    template: '## Problem & Context\n\n## Implementation Proposal\n\n## Acceptance Criteria\n- [ ] '
  };

  function insertSnippetIntoTextarea(textarea, snippet) {
    if (!textarea) return;
    var start = typeof textarea.selectionStart === 'number' ? textarea.selectionStart : textarea.value.length;
    var end = typeof textarea.selectionEnd === 'number' ? textarea.selectionEnd : start;
    var val = textarea.value;
    var before = val.substring(0, start);
    var after = val.substring(end);
    var prefix = '';
    if (before && !before.endsWith('\n\n')) {
      prefix = before.endsWith('\n') ? '\n' : '\n\n';
    }
    var inserted = prefix + snippet;
    textarea.value = before + inserted + after;
    var newPos = before.length + inserted.length;
    textarea.selectionStart = newPos;
    textarea.selectionEnd = newPos;
    if (textarea.focus) textarea.focus();
  }

  function getClipboardImages(e) {
    var cd = (e && e.clipboardData) || (window && window.clipboardData);
    if (!cd) return [];
    var images = [];
    if (cd.items && cd.items.length) {
      for (var i = 0; i < cd.items.length; i++) {
        var item = cd.items[i];
        if (item && item.type && item.type.indexOf('image') !== -1) {
          var f = typeof item.getAsFile === 'function' ? item.getAsFile() : null;
          if (f) images.push(f);
        }
      }
    }
    if (images.length === 0 && cd.files && cd.files.length) {
      for (var j = 0; j < cd.files.length; j++) {
        var file = cd.files[j];
        if (file && file.type && file.type.indexOf('image') !== -1) {
          images.push(file);
        }
      }
    }
    return images;
  }

  function uploadScreenshot(file) {
    return fetch(BASE + 'screenshots', {
      method: 'POST',
      headers: {
        'Content-Type': (file && file.type) || 'image/png',
        'X-Pin-Action': 'true'
      },
      body: file
    })
    .then(function (res) {
      if (!res.ok) {
        return res.json().then(function (err) {
          throw new Error(err.error || ('Failed to upload screenshot (' + res.status + ')'));
        }).catch(function (pErr) {
          throw new Error(pErr.message || ('Failed to upload screenshot (' + res.status + ')'));
        });
      }
      return res.json();
    });
  }

  function handleScreenshotPaste(e, textareaOverride) {
    var images = getClipboardImages(e);
    if (!images.length) return false;

    if (e && typeof e.preventDefault === 'function') {
      e.preventDefault();
    }

    var targetTa = textareaOverride || (e && (e.currentTarget || e.target));
    if (!targetTa || typeof targetTa.value !== 'string') {
      var modalOpen = els.specModal && (els.specModal.open || els.specModal.hasAttribute('open'));
      targetTa = modalOpen ? els.specModalBody : els.quickAddBody;
    }
    if (!targetTa) return true;

    for (var i = 0; i < images.length; i++) {
      (function (img, idx) {
        var placeholder = '![Uploading screenshot' + (images.length > 1 ? (' ' + (idx + 1)) : '') + '...]()';
        insertSnippetIntoTextarea(targetTa, placeholder);

        uploadScreenshot(img)
          .then(function (res) {
            var md = (res && res.markdown) || ('![screenshot](' + (res && res.path ? res.path : '') + ')');
            if (targetTa.value.indexOf(placeholder) !== -1) {
              targetTa.value = targetTa.value.replace(placeholder, md);
            } else {
              insertSnippetIntoTextarea(targetTa, md);
            }
            showToast('Screenshot pasted', false);
          })
          .catch(function (err) {
            if (targetTa.value.indexOf(placeholder) !== -1) {
              targetTa.value = targetTa.value.replace(placeholder, '');
            }
            showToast(err.message || 'Failed to paste screenshot', true);
          });
      })(images[i], i);
    }
    return true;
  }

  function createItem(payload) {
    return fetch(BASE + 'items', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'X-Pin-Action': 'true'
      },
      body: JSON.stringify(payload)
    })
    .then(function (res) {
      if (!res.ok) {
        return res.json().then(function (err) { throw new Error(err.error || 'Failed to create item'); })
          .catch(function (pErr) { throw new Error(pErr.message || 'Failed to create item'); });
      }
      return res.json();
    });
  }

  function expandQuickAdd(focusTarget) {
    if (els.quickAddTrigger) els.quickAddTrigger.hidden = true;
    if (els.quickAddForm) els.quickAddForm.hidden = false;
    if (focusTarget === 'body' && els.quickAddBody && els.quickAddBody.focus) {
      els.quickAddBody.focus();
    } else if (els.quickAddInput && els.quickAddInput.focus) {
      els.quickAddInput.focus();
    }
  }

  function collapseQuickAdd() {
    if (els.quickAddForm) els.quickAddForm.hidden = true;
    if (els.quickAddTrigger) els.quickAddTrigger.hidden = false;
  }

  function resetQuickAdd() {
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

  function submitQuickAdd() {
    var title = els.quickAddInput ? els.quickAddInput.value.trim() : '';
    if (!title) {
      if (els.quickAddInput) els.quickAddInput.focus();
      showToast('Title is required', true);
      return;
    }
    var body = els.quickAddBody ? els.quickAddBody.value.trim() : '';
    var type = els.quickAddType ? els.quickAddType.value : 'task';
    var kind = els.quickAddKind ? els.quickAddKind.value : 'technical';
    var priority = els.quickAddPriority && els.quickAddPriority.value ? els.quickAddPriority.value : undefined;
    var tags = els.quickAddTags && els.quickAddTags.value.trim() ? els.quickAddTags.value.trim() : undefined;
    var activeProject = (state.scope && state.scope !== 'all') ? state.scope : undefined;

    setQuickAddDisabled(true);
    createItem({
      title: title,
      body: body || undefined,
      type: type,
      kind: kind,
      priority: priority,
      tags: tags,
      status: 'created',
      project: activeProject
    })
    .then(function (newItem) {
      resetQuickAdd();
      setQuickAddDisabled(false);
      showToast('Created ' + (newItem.type || 'task') + ': ' + (newItem.title || title), false);
      refreshData();
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

  function setSpecModalTab(tab) {
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
          try {
            rendered = marked.parse(raw, { renderer: markedRenderer, gfm: true, async: false });
          } catch (_) {
            rendered = raw;
          }
          try {
            els.specModalPreview.innerHTML = DOMPurify.sanitize(rendered, PURIFY_CONFIG);
          } catch (_) {
            text(els.specModalPreview, raw);
          }
        }
      }
    } else {
      if (els.specTabWrite) els.specTabWrite.classList.add('active');
      if (els.specTabPreview) els.specTabPreview.classList.remove('active');
      if (els.specModalBody) els.specModalBody.hidden = false;
      if (els.specModalPreview) els.specModalPreview.hidden = true;
    }
  }

  function openSpecModal(initial) {
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

  function closeSpecModal() {
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

  function submitSpecModal() {
    var title = els.specModalTitle ? els.specModalTitle.value.trim() : '';
    if (!title) {
      showSpecModalError('Title is required');
      if (els.specModalTitle) els.specModalTitle.focus();
      return;
    }
    var body = els.specModalBody ? els.specModalBody.value.trim() : '';
    var type = els.specModalType ? els.specModalType.value : 'task';
    var kind = els.specModalKind ? els.specModalKind.value : 'technical';
    var priority = els.specModalPriority && els.specModalPriority.value ? els.specModalPriority.value : undefined;
    var tags = els.specModalTags && els.specModalTags.value.trim() ? els.specModalTags.value.trim() : undefined;
    var projectVal = els.specModalProject ? els.specModalProject.value.trim() : '';
    var activeProject = projectVal || ((state.scope && state.scope !== 'all') ? state.scope : undefined);

    showSpecModalError('');
    setSpecModalDisabled(true);

    createItem({
      title: title,
      body: body || undefined,
      type: type,
      kind: kind,
      priority: priority,
      tags: tags,
      status: 'created',
      project: activeProject
    })
    .then(function (newItem) {
      setSpecModalDisabled(false);
      closeSpecModal();
      showToast('Created ' + (newItem.type || 'task') + ': ' + (newItem.title || title), false);
      refreshData();
      if (newItem && newItem.id) {
        setRoute(newItem.id);
      }
    })
    .catch(function (err) {
      setSpecModalDisabled(false);
      showSpecModalError(err.message || 'Failed to create task');
    });
  }

  function openActionModal(config) {
    activeModalConfig = config;
    showDialogError('');
    text(els.dialogTitle, config.title || 'Action');
    text(els.dialogSubmitBtn, config.submitText || 'Confirm');
    if (els.dialogSubmitBtn) els.dialogSubmitBtn.disabled = false;
    clear(els.dialogBody);
    if (config.render) config.render(els.dialogBody);

    if (els.actionDialog.showModal) {
      els.actionDialog.showModal();
    } else {
      els.actionDialog.setAttribute('open', '');
    }

    var firstField = els.dialogBody.querySelector('textarea, input, select');
    if (firstField) firstField.focus();
  }

  function closeActionModal() {
    activeModalConfig = null;
    showDialogError('');
    if (els.dialogSubmitBtn) els.dialogSubmitBtn.disabled = false;
    if (els.actionDialog.close) {
      els.actionDialog.close();
    } else {
      els.actionDialog.removeAttribute('open');
    }
  }

  function triggerModalSubmit() {
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
          // A busy primary checkout is not a revision conflict: offer the
          // worktree instead of a "Sync & Retry" loop that cannot succeed.
          if (err && err.data && err.data.status === 'primary_busy') {
            var busyItem = state.lastAction ? find(state.lastAction.id) : null;
            var activeId = err.data.active_id || 'another task';
            closeActionModal();
            if (busyItem) {
              openWorktreeModal(busyItem, activeId);
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
              fetch(BASE + 'data.json', { credentials: 'same-origin', cache: 'no-cache' })
                .then(function (res) {
                  if (!res.ok) throw new Error('Failed to refresh data (' + res.status + ')');
                  return res.json();
                })
                .then(function (data) {
                  if (data) ingest(data);
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

  function openCompleteModal(item) {
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
          if (ta) ta.focus();
          showDialogError('Verification evidence is required.');
          return false;
        }
        return sendAction(item.id, { action: 'complete', evidence: val, expect_revision: rev });
      }
    });
  }

  function openBlockedModal(item) {
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
          if (ta) ta.focus();
          showDialogError('Blocker reason is required.');
          return false;
        }
        return sendAction(item.id, { action: 'transition', to: 'blocked', note: val, expect_revision: rev });
      }
    });
  }

  function openCloseModal(item) {
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

  function handleCardDrop(droppedId, targetStatus) {
    var item = find(droppedId);
    if (!item) return;
    var cur = itemStatus(item);
    var rev = item.revision;

    if (targetStatus === 'done') {
      if (cur === 'in_progress') {
        openCompleteModal(item);
      } else {
        sendAction(item.id, { action: 'transition', to: 'done', expect_revision: rev })
          .catch(function (err) { showToast('Action failed: ' + err.message, true); });
      }
      return;
    }

    if (targetStatus === 'blocked') {
      openBlockedModal(item);
      return;
    }

    if (targetStatus === 'in_progress') {
      if (cur === 'in_progress') return;
      openWorktreeModal(item, null);
      return;
    }

    if (targetStatus === 'planned') {
      if (cur === 'planned') return;
      sendAction(item.id, { action: 'transition', to: 'planned', expect_revision: rev })
        .then(function () { showToast('Moved to Planned', false); })
        .catch(function (err) { showToast('Action failed: ' + err.message, true); });
      return;
    }

    if (targetStatus === 'created') {
      if (cur !== 'created') {
        sendAction(item.id, { action: 'transition', to: 'created', expect_revision: rev })
          .catch(function (err) { showToast('Action failed: ' + err.message, true); });
      }
      return;
    }

    if (targetStatus === 'closed') {
      openCloseModal(item);
      return;
    }
  }

  function buildBoardCard(item) {
    var card = node('button', 'board-card status-border-' + itemStatus(item));
    card.type = 'button';
    card.dataset.id = item.id;
    card.draggable = true;

    var header = node('div', 'board-card-header');
    var idSpan = node('span', 'board-card-id', item.id.slice(0, 8));
    header.appendChild(idSpan);

    if (priority(item) !== 'unset') {
      var dot = node('span', 'priority-dot priority-' + priority(item));
      dot.title = priority(item) + ' priority';
      header.appendChild(dot);
    }
    card.appendChild(header);

    var titleEl = node('h4', 'board-card-title', item.title || '(untitled)');
    card.appendChild(titleEl);

    var meta = node('div', 'board-card-meta');
    meta.appendChild(node('span', 'type-badge', itemType(item)));

    if (state.activeRuns.has(item.id)) {
      var runBadge = node('span', 'card-running-indicator');
      runBadge.appendChild(node('span', 'card-running-dot'));
      runBadge.appendChild(document.createTextNode('Running Agent'));
      meta.appendChild(runBadge);

      var viewRunBtn = node('button', 'btn-view-run', 'View Run');
      viewRunBtn.type = 'button';
      viewRunBtn.addEventListener('click', function (e) {
        e.stopPropagation();
        openAgentDrawer(item.id);
      });
      meta.appendChild(viewRunBtn);
    } else {
      var cardStatus = itemStatus(item);
      if (cardStatus === 'review' || cardStatus === 'done') {
        var commitPrBtn = node('button', 'btn-commit-pr', 'Commit + PR');
        commitPrBtn.type = 'button';
        commitPrBtn.title = 'Start agent to commit worktree and create PR';
        commitPrBtn.addEventListener('click', function (e) {
          e.stopPropagation();
          sendCommitPrItem(item.id);
        });
        meta.appendChild(commitPrBtn);
      }
    }
    if (hasUnresolvedPrereqs(item)) {
      card.classList.add('is-locked');
      var lock = node('span', 'lock-indicator', '🔒 Locked');
      lock.title = 'Has unresolved prerequisite dependencies';
      meta.appendChild(lock);
    }

    card.appendChild(meta);

    card.addEventListener('dragstart', function (e) {
      e.dataTransfer.setData('text/plain', item.id);
      e.dataTransfer.effectAllowed = 'move';
      card.classList.add('dragging');
    });

    card.addEventListener('dragend', function () {
      card.classList.remove('dragging');
    });

    card.addEventListener('click', function () {
      setRoute(item.id);
      setViewMode('detail');
    });

    return card;
  }

  function renderBoard() {
    var cols = {
      created: document.querySelector('.board-col-cards[data-drop-status="created"]'),
      planned: document.querySelector('.board-col-cards[data-drop-status="planned"]'),
      in_progress: document.querySelector('.board-col-cards[data-drop-status="in_progress"]'),
      blocked: document.querySelector('.board-col-cards[data-drop-status="blocked"]'),
      done_review: document.querySelector('.board-col-cards[data-drop-status="done"]'),
      closed: document.querySelector('.board-drawer-content')
    };

    Object.keys(cols).forEach(function (k) { if (cols[k]) clear(cols[k]); });

    var counts = { created: 0, planned: 0, in_progress: 0, blocked: 0, done_review: 0, closed: 0 };

    state.shown.forEach(function (item) {
      var st = itemStatus(item);
      var targetCol = null;
      if (st === 'created') { targetCol = cols.created; counts.created++; }
      else if (st === 'planned') { targetCol = cols.planned; counts.planned++; }
      else if (st === 'in_progress') { targetCol = cols.in_progress; counts.in_progress++; }
      else if (st === 'blocked') { targetCol = cols.blocked; counts.blocked++; }
      else if (st === 'review' || st === 'done') { targetCol = cols.done_review; counts.done_review++; }
      else if (st === 'closed' || st === 'cancelled') { targetCol = cols.closed; counts.closed++; }

      if (targetCol) {
        targetCol.appendChild(buildBoardCard(item));
      }
    });

    text(els.countCreated, counts.created);
    text(els.countPlanned, counts.planned);
    text(els.countInProgress, counts.in_progress);
    text(els.countBlocked, counts.blocked);
    text(els.countDoneReview, counts.done_review);
    text(els.countClosed, counts.closed);
  }

  function renderList() {
    state.shown = state.items.filter(matches);
    clear(els.list);
    var countText = state.shown.length === state.items.length
      ? state.items.length + (state.items.length === 1 ? ' item' : ' items')
      : state.shown.length + ' of ' + state.items.length;
    text(els.count, countText);

    els.list.hidden = !state.shown.length;
    els.listEmpty.hidden = !!state.shown.length;

    if (!state.shown.length) {
      clear(els.listEmpty);
      els.listEmpty.appendChild(node('strong', '', state.items.length ? 'No items match filters.' : 'No items in this vault.'));
      els.listEmpty.appendChild(node('p', '', state.items.length ? 'Try clearing search or filters.' : 'Add items with pin add.'));
      renderBoard();
      return;
    }

    state.shown.forEach(function (item) {
      var li = node('li');
      var button = node('button', 'proposal-item status-border-' + itemStatus(item));
      button.type = 'button';
      button.dataset.id = item.id;
      button.setAttribute('aria-current', item.id === state.selected ? 'true' : 'false');

      var titleEl = node('strong', '', item.title || '(untitled)');
      button.appendChild(titleEl);

      var meta = node('span', 'item-badges');
      meta.appendChild(node('span', 'type-badge', itemType(item)));
      meta.appendChild(node('span', 'status-badge status-' + itemStatus(item), itemStatus(item)));

      if (priority(item) !== 'unset') {
        meta.appendChild(node('span', 'priority-' + priority(item), priority(item)));
      }

      var time = node('time', '', timeAgo(item.timestamp));
      time.dateTime = String(item.timestamp || '');
      meta.appendChild(time);

      button.appendChild(meta);
      button.addEventListener('click', function () { setRoute(item.id); });
      li.appendChild(button);
      els.list.appendChild(li);
    });

    renderBoard();
  }

  function addSummary(value, className) {
    if (value) els.summary.appendChild(node('span', className || '', value));
  }

  function addMeta(label, value) {
    if (!value) return;
    els.meta.appendChild(node('dt', '', label));
    els.meta.appendChild(node('dd', '', value));
  }

  function bodyWithoutDuplicateTitle(body, title) {
    var source = String(body || '');
    var match = source.match(/^\s*#\s+(.+?)\s*(?:\n|$)/);
    return match && norm(match[1].replace(/[*_`]/g, '').trim()) === norm(title).trim()
      ? source.slice(match[0].length).replace(/^\s+/, '')
      : source;
  }

  var markedRenderer = null;
  if (typeof marked !== 'undefined' && marked && typeof marked.Renderer === 'function') {
    try {
      markedRenderer = new marked.Renderer();
      markedRenderer.listitem = function (item, task, checked) {
        var textStr = '';
        var isTask = false;
        var isChecked = false;
        if (item && typeof item === 'object') {
          isTask = !!item.task;
          isChecked = !!item.checked;
          if (item.tokens) {
            var filteredTokens = item.tokens.filter(function (t) { return t.type !== 'checkbox'; });
            textStr = this.parser ? this.parser.parse(filteredTokens) : (item.text || '');
          } else {
            textStr = item.text || '';
          }
        } else {
          textStr = String(item || '');
          isTask = !!task;
          isChecked = !!checked;
        }

        if (isTask || /^\[[ xX]\]\s*/.test(textStr)) {
          isChecked = isChecked || /^\[[xX]\]\s*/.test(textStr);
          var cleanText = textStr.replace(/^\[[ xX]\]\s*/, '');
          return '<li class="task-list-item"><input type="checkbox" disabled ' + (isChecked ? 'checked ' : '') + '/> ' + cleanText + '</li>';
        }
        return '<li>' + textStr + '</li>';
      };
      markedRenderer.image = function (token, title, text) {
        var src = (typeof token === 'object' && token) ? (token.href || '') : (token || '');
        var label = (typeof token === 'object' && token) ? (token.text || '') : (text || '');
        var filename = src.split('/').pop() || src;
        var viewUrl = BASE + 'screenshots/' + encodeURIComponent(filename);
        return '<p class="screenshot-ref">📷 <a href="' + escapeHtml(viewUrl) + '" target="_blank" rel="noopener"><code>' + escapeHtml(src || label || 'screenshot') + '</code></a></p>';
      };
      marked.use({ renderer: markedRenderer, gfm: true });
    } catch (_) {}
  }

  function renderMarkdown(item) {
    var source = bodyWithoutDuplicateTitle(item.content || item.body, item.title), dirty;
    try { dirty = marked.parse(source, { renderer: markedRenderer, gfm: true, async: false }); } catch (_) { text(els.body, source); return; }
    try { els.body.innerHTML = DOMPurify.sanitize(dirty, PURIFY_CONFIG); } catch (_) { text(els.body, source); }
  }

  function renderWorkerBox(item) {
    clear(els.workerCard);
    els.workerCard.appendChild(node('h3', '', 'Agent Run'));

    var header = node('div', 'worker-card-header');
    if (state.activeRuns.has(item.id)) {
      header.appendChild(node('span', 'worker-actor', item.claimed_by ? '@' + item.claimed_by : 'Agent running'));
    } else {
      header.appendChild(node('span', 'worker-actor', 'No active run'));
    }
    if (item.revision != null) {
      header.appendChild(node('span', 'worker-revision', 'rev ' + item.revision));
    }
    els.workerCard.appendChild(header);

    var meta = node('div', 'worker-meta');
    if (item.created_by) {
      meta.appendChild(node('span', '', 'Created by @' + item.created_by));
    }
    if (meta.children.length) {
      els.workerCard.appendChild(meta);
    }
  }

  function renderDependencies(item) {
    clear(els.dependenciesCard);
    els.dependenciesCard.appendChild(node('h3', '', 'Dependencies'));

    var list = node('ul', 'dependencies-list');
    var hasAny = false;

    if (item.depends_on && item.depends_on.length) {
      item.depends_on.forEach(function (depId) {
        hasAny = true;
        var depItem = find(depId);
        var isDone = depItem && (itemStatus(depItem) === 'done' || itemStatus(depItem) === 'closed');
        var li = node('li', 'dependency-item');
        var ind = node('span', 'dep-indicator ' + (isDone ? 'resolved' : 'unresolved'), isDone ? '✓' : '○');
        ind.title = isDone ? 'Resolved' : 'Unresolved prerequisite';
        li.appendChild(ind);

        var btn = node('button', 'dependency-link', (depItem && depItem.title ? depItem.title : depId));
        btn.type = 'button';
        btn.title = 'Prerequisite: ' + depId;
        btn.addEventListener('click', function () { setRoute(depId); });
        li.appendChild(btn);

        if (depItem) {
          li.appendChild(node('span', 'status-badge status-' + itemStatus(depItem), itemStatus(depItem)));
        }
        list.appendChild(li);
      });
    }

    var dependents = state.items.filter(function (other) {
      return other.depends_on && other.depends_on.indexOf(item.id) !== -1;
    });

    if (dependents.length) {
      dependents.forEach(function (dep) {
        hasAny = true;
        var li = node('li', 'dependency-item');
        var ind = node('span', 'dep-indicator', '↳');
        ind.title = 'Dependent item';
        li.appendChild(ind);

        var btn = node('button', 'dependency-link', dep.title || dep.id);
        btn.type = 'button';
        btn.title = 'Dependent: ' + dep.id;
        btn.addEventListener('click', function () { setRoute(dep.id); });
        li.appendChild(btn);
        li.appendChild(node('span', 'status-badge status-' + itemStatus(dep), itemStatus(dep)));
        list.appendChild(li);
      });
    }

    if (hasAny) {
      els.dependenciesCard.appendChild(list);
    } else {
      els.dependenciesCard.appendChild(node('div', 'help-text', 'No prerequisites or dependents'));
    }
  }

  function renderActions(item) {
    clear(els.actions);
    var st = itemStatus(item);
    var rev = item.revision;

    function btn(label, isPrimary, onClick) {
      var b = node('button', 'action-btn' + (isPrimary ? ' primary' : ''), label);
      b.type = 'button';
      b.addEventListener('click', onClick);
      els.actions.appendChild(b);
    }

    if (st === 'created') {
      btn('Move to Planned', true, function () {
        sendAction(item.id, { action: 'transition', to: 'planned', expect_revision: rev })
          .then(function () { showToast('Moved to Planned', false); })
          .catch(function (err) { showToast('Action failed: ' + err.message, true); });
      });
    }

    if (st === 'planned') {
      btn('Start Agent', true, function () {
        openWorktreeModal(item, null);
      });
      btn('Mark Blocked', false, function () {
        openBlockedModal(item);
      });
    }

    if (st === 'in_progress') {
      if (state.activeRuns.has(item.id)) {
        btn('View Agent Run', true, function () {
          openAgentDrawer(item.id);
        });
      } else {
        btn('Resume Agent', true, function () {
          openWorktreeModal(item, null);
        });
      }
      btn('Complete with Evidence', !state.activeRuns.has(item.id), function () {
        openCompleteModal(item);
      });
      btn('Mark Blocked', false, function () {
        openBlockedModal(item);
      });
    }

    if (st === 'blocked') {
      btn('Unblock (Move to Planned)', true, function () {
        sendAction(item.id, { action: 'transition', to: 'planned', expect_revision: rev })
          .then(function () { showToast('Moved to Planned', false); })
          .catch(function (err) { showToast('Action failed: ' + err.message, true); });
      });
    }

    if (st === 'review') {
      if (state.activeRuns.has(item.id)) {
        btn('View Agent Run', true, function () {
          openAgentDrawer(item.id);
        });
      } else {
        btn('Commit + PR', true, function () {
          sendCommitPrItem(item.id);
        });
      }
      btn('Close Item', false, function () {
        openCloseModal(item);
      });
      btn('Reopen to Planned', false, function () {
        sendAction(item.id, { action: 'transition', to: 'planned', expect_revision: rev })
          .then(function () { showToast('Reopened to Planned', false); })
          .catch(function (err) { showToast('Action failed: ' + err.message, true); });
      });
    }

    if (st === 'done') {
      if (state.activeRuns.has(item.id)) {
        btn('View Agent Run', true, function () {
          openAgentDrawer(item.id);
        });
      } else {
        btn('Commit + PR', true, function () {
          sendCommitPrItem(item.id);
        });
      }
      btn('Close Item', false, function () {
        openCloseModal(item);
      });
      btn('Reopen to Planned', false, function () {
        sendAction(item.id, { action: 'transition', to: 'planned', expect_revision: rev })
          .then(function () { showToast('Reopened to Planned', false); })
          .catch(function (err) { showToast('Action failed: ' + err.message, true); });
      });
    }

    if (st !== 'closed' && st !== 'cancelled' && st !== 'done' && st !== 'review') {
      btn('Close Item', false, function () {
        openCloseModal(item);
      });
    }

    els.actions.hidden = !els.actions.children.length;
  }

  function renderHandoff(item) {
    clear(els.handoffCard);
    var h = item.handoff;
    if (!h || (!h.progress && !h.next && !h.blocker && !h.verification)) {
      els.handoffCard.hidden = true;
      return;
    }

    els.handoffCard.hidden = false;
    els.handoffCard.appendChild(node('h3', '', 'Agent Handoff & Evidence'));

    if (h.progress) {
      var p = node('div', 'handoff-field');
      p.appendChild(node('strong', '', 'Progress:'));
      p.appendChild(node('div', 'handoff-text', h.progress));
      els.handoffCard.appendChild(p);
    }
    if (h.next) {
      var n = node('div', 'handoff-field');
      n.appendChild(node('strong', '', 'Next steps:'));
      n.appendChild(node('div', 'handoff-text', h.next));
      els.handoffCard.appendChild(n);
    }
    if (h.blocker) {
      var b = node('div', 'handoff-field');
      b.appendChild(node('strong', '', 'Blocker:'));
      b.appendChild(node('div', 'handoff-text', h.blocker));
      els.handoffCard.appendChild(b);
    }
    if (h.verification) {
      var v = node('div', 'handoff-field');
      v.appendChild(node('strong', '', 'Verification evidence:'));
      v.appendChild(node('pre', 'handoff-evidence', h.verification));
      els.handoffCard.appendChild(v);
    }
  }

  function renderActivity(item) {
    clear(els.activityList);
    var evs = Array.isArray(item.activity) ? item.activity : [];
    els.activitySection.hidden = evs.length === 0;
    if (evs.length === 0) return;

    evs.slice().reverse().forEach(function (ev) {
      var li = node('li', 'activity-item');
      var meta = node('div', 'activity-meta');
      meta.appendChild(node('strong', '', ev.actor || 'unknown'));
      meta.appendChild(document.createTextNode(' ' + (ev.action || 'event')));
      if (ev.from || ev.to) {
        meta.appendChild(document.createTextNode(' (' + (ev.from || '?') + ' → ' + (ev.to || '?') + ')'));
      }
      meta.appendChild(node('span', 'activity-time', date(ev.at, true)));
      li.appendChild(meta);

      if (ev.note) {
        li.appendChild(node('div', 'activity-note', ev.note));
      }
      els.activityList.appendChild(li);
    });
  }

  function setReaderTab(tab) {
    state.readerTab = tab;
    if (tab === 'trajectory') {
      if (els.tabAgentTrajectory) els.tabAgentTrajectory.classList.add('active');
      if (els.tabTaskSpec) els.tabTaskSpec.classList.remove('active');
      if (els.proposal) els.proposal.hidden = true;
      if (els.agentDrawer) els.agentDrawer.hidden = false;

      if (state.currentStreamingId === state.selected && els.trajectoryStream && els.trajectoryStream.children.length > 0) {
        return;
      }
      if (state.selected) {
        openAgentDrawer(state.selected);
      }
    } else {
      if (els.tabTaskSpec) els.tabTaskSpec.classList.add('active');
      if (els.tabAgentTrajectory) els.tabAgentTrajectory.classList.remove('active');
      if (els.proposal) els.proposal.hidden = false;
      if (els.agentDrawer) els.agentDrawer.hidden = true;
    }
  }

  function renderDetail() {
    var item = find(state.selected);
    document.body.classList.toggle('detail-open', !!item);
    els.detailEmpty.hidden = !!item;

    if (!item) {
      if (els.readerTabs) els.readerTabs.hidden = true;
      if (els.proposal) els.proposal.hidden = true;
      if (els.agentDrawer) els.agentDrawer.hidden = true;
      text(els.detailEmpty, state.selected ? 'This item is not in the vault.' : 'Select an item to view it.');
      return;
    }

    if (els.readerTabs) els.readerTabs.hidden = false;
    if (els.readerTrajectoryBadge) els.readerTrajectoryBadge.hidden = !state.activeRuns.has(item.id);
    setReaderTab(state.readerTab === 'trajectory' ? 'trajectory' : 'spec');
    var context = [];
    if (item.project) context.push(item.project);
    context.push(itemType(item).toUpperCase());
    context.push(kind(item));
    text(els.context, context.join(' · '));
    text(els.title, item.title || '(untitled)');

    clear(els.summary);
    addSummary(itemStatus(item), 'status-badge status-' + itemStatus(item));
    if (priority(item) !== 'unset') addSummary(priority(item) + ' priority', 'priority-' + priority(item));
    addSummary(date(item.timestamp, true));
    if (tags(item).length) addSummary(tags(item).join(' · '));

    renderWorkerBox(item);
    renderHandoff(item);
    renderDependencies(item);
    renderActions(item);

    clear(els.meta);
    addMeta('ID', item.id);
    addMeta('Type', item.type);
    addMeta('Status', item.status);
    addMeta('Revision', item.revision != null ? String(item.revision) : '');
    addMeta('Created by', item.created_by);
    if (item.parent_id) addMeta('Parent ID', item.parent_id);
    if (item.depends_on && item.depends_on.length) addMeta('Depends on', item.depends_on.join(', '));
    if (item.related && item.related.length) addMeta('Related', item.related.join(', '));
    addMeta('File', item.filename);
    if (item.archived_at) addMeta('Archived', date(item.archived_at, true));
    els.more.hidden = !els.meta.children.length;

    clear(els.resolution);
    els.resolution.hidden = !(item.resolution || item.resolution_note);
    if (!els.resolution.hidden) {
      els.resolution.appendChild(node('strong', '', item.resolution ? 'Resolution: ' + item.resolution : 'Resolution'));
      if (item.resolution_note) els.resolution.appendChild(node('div', '', item.resolution_note));
    }

    renderMarkdown(item);
    renderActivity(item);
  }

  function applyRoute() {
    state.selected = route();
    if (state.selected && state.viewMode !== 'detail') {
      setViewMode('detail');
    }
    renderList();
    renderDetail();
    if (state.selected && isNarrow()) {
      try { els.reader.focus({ preventScroll: true }); } catch (_) { els.reader.focus(); }
    }
  }

  function updateActiveRuns() {
    fetch(BASE + 'runs')
      .then(function (res) {
        if (!res.ok) return null;
        return res.json();
      })
      .then(function (data) {
        if (!data || !Array.isArray(data.running)) return;
        state.activeRunsTimings = data.timings || {};
        var newSet = new Set(data.running);
        var changed = newSet.size !== state.activeRuns.size;
        if (!changed) {
          newSet.forEach(function (id) {
            if (!state.activeRuns.has(id)) changed = true;
          });
        }
        if (changed) {
          state.activeRuns = newSet;
          if (state.viewMode === 'board') {
            renderBoard();
          }
          if (state.selected && els.readerTrajectoryBadge) {
            els.readerTrajectoryBadge.hidden = !state.activeRuns.has(state.selected);
          }
        }
      })
      .catch(function () {});
  }

  function openWorktreeModal(item, busyId) {
    state.pendingWorktreeItem = item;
    var targetId = item ? item.id : '';
    text(els.worktreeBusyId, busyId || 'another task');
    text(els.worktreeTargetId, targetId);
    if (els.worktreePromptTargetId) {
      text(els.worktreePromptTargetId, targetId);
    }
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
      if (els.worktreeModal.showModal) {
        els.worktreeModal.showModal();
      } else {
        els.worktreeModal.setAttribute('open', '');
      }
    }
  }

  function closeWorktreeModal() {
    state.pendingWorktreeItem = null;
    if (els.worktreeModal) {
      if (els.worktreeModal.close) {
        els.worktreeModal.close();
      } else {
        els.worktreeModal.removeAttribute('open');
      }
    }
  }

  var scrollStreamPending = false;
  function scrollTrajectoryStreamToBottom() {
    if (!els.trajectoryStream || scrollStreamPending) return;
    scrollStreamPending = true;
    var raf = window.requestAnimationFrame || function (cb) { setTimeout(cb, 0); };
    raf(function () {
      scrollStreamPending = false;
      if (els.trajectoryStream) {
        els.trajectoryStream.scrollTop = els.trajectoryStream.scrollHeight;
      }
    });
  }

  var scrollThoughtsPending = false;
  function scrollAgentThoughtsToBottom() {
    if (!els.agentThoughts || scrollThoughtsPending) return;
    scrollThoughtsPending = true;
    var raf = window.requestAnimationFrame || function (cb) { setTimeout(cb, 0); };
    raf(function () {
      scrollThoughtsPending = false;
      if (els.agentThoughts) {
        els.agentThoughts.scrollTop = els.agentThoughts.scrollHeight;
      }
    });
  }

  function appendTrajectoryContextRow(message) {
    if (!els.trajectoryStream) return null;
    var row = node('div', 'trajectory-row trajectory-row-context');
    var gutter = node('div', 'trajectory-gutter', '●');
    var badge = node('span', 'trajectory-badge badge-context', 'CONTEXT');
    var body = node('div', 'trajectory-body');
    var textEl = node('div', 'trajectory-context-text');
    if (typeof message === 'string') {
      textEl.textContent = message;
    } else if (message instanceof Node) {
      textEl.appendChild(message);
    } else {
      textEl.textContent = String(message || '');
    }
    body.appendChild(textEl);
    row.appendChild(gutter);
    row.appendChild(badge);
    row.appendChild(body);
    els.trajectoryStream.appendChild(row);
    scrollTrajectoryStreamToBottom();
    if (state.trajectory && state.trajectory.filterQuery) {
      var rowText = (row.textContent || '').toLowerCase();
      row.hidden = rowText.indexOf(state.trajectory.filterQuery) === -1;
    }
    return row;
  }

  function formatToolCallInline(update) {
    if (!update) return { name: '', argsStr: '', previewStr: '', status: 'running' };

    var rawIn = update.rawInput || update.input || update.arguments || update.params || update.args;
    var rawToolName = update.name || update.tool || '';
    if (update.kind && update.name) rawToolName = update.kind + ': ' + update.name;
    var rawTitle = update.title ? stripAnsi(update.title.trim()) : '';

    var cmd = '';
    if (rawIn && typeof rawIn === 'object' && rawIn.command) {
      cmd = stripAnsi(String(rawIn.command)).trim();
    } else if (rawIn && typeof rawIn === 'string' && (rawToolName === 'bash' || rawToolName === 'sh')) {
      cmd = stripAnsi(rawIn).trim();
    } else if (rawTitle && /^bash:\s*/i.test(rawTitle)) {
      cmd = rawTitle.replace(/^bash:\s*/i, '').trim();
    }

    var path = '';
    if (rawIn && typeof rawIn === 'object' && rawIn.path) {
      path = stripAnsi(String(rawIn.path)).trim();
    }

    var pattern = '';
    if (rawIn && typeof rawIn === 'object' && rawIn.pattern) {
      pattern = stripAnsi(String(rawIn.pattern)).trim();
    }

    var isShell = !!cmd || rawToolName === 'bash' || rawToolName === 'sh';
    var isFile = rawToolName === 'read' || rawToolName === 'write' || rawToolName === 'edit';
    var isGrep = rawToolName === 'grep';

    var name = '';
    var argsStr = '';

    if (isShell) {
      name = '$ ' + (cmd || rawToolName || 'sh');
      argsStr = '';
    } else if (isFile) {
      name = rawToolName + (path ? ': ' + path : '');
      argsStr = '';
    } else if (isGrep) {
      name = 'grep: ' + pattern + (path ? ' in ' + path : '');
      argsStr = '';
    }
    if (name.length > 80) name = name.slice(0, 77) + '...';
    if (!name) {
      name = rawTitle || formatToolTitle(update);
      if (rawIn != null && typeof rawIn === 'object') {
        var extraKeys = Object.keys(rawIn).filter(function (k) {
          return k !== 'path' && k !== 'command' && k !== 'i';
        });
        if (extraKeys.length > 0) {
          var filtered = {};
          extraKeys.forEach(function (k) { filtered[k] = rawIn[k]; });
          try {
            argsStr = JSON.stringify(filtered).replace(/\s+/g, ' ');
          } catch (_) {
            argsStr = String(filtered);
          }
          if (argsStr.length > 90) argsStr = argsStr.slice(0, 87) + '...';
        } else {
          if (path && (!name || name === 'tool' || name === rawToolName)) {
            name = (rawToolName || 'read') + ': ' + path;
          }
          argsStr = '';
        }
      } else if (typeof rawIn === 'string') {
        argsStr = stripAnsi(rawIn).replace(/\s+/g, ' ').trim();
        if (argsStr.length > 90) argsStr = argsStr.slice(0, 87) + '...';
      }
    }

    var rawOut = update.rawOutput || update.output || update.result || update.error || update.content;
    var previewStr = '';
    if (rawOut != null) {
      var formattedOut = formatToolPayload(rawOut);
      previewStr = stripAnsi(formattedOut).replace(/\s+/g, ' ').trim();
      if (previewStr.length > 90) previewStr = previewStr.slice(0, 87) + '...';
    }

    var status = update.status || 'running';
    if (status === 'in_progress') status = 'running';

    return {
      name: name,
      argsStr: argsStr,
      previewStr: previewStr,
      status: status
    };
  }

  function renderSwimlaneSvg() {
    if (!els.swimlaneSvg) return;
    clear(els.swimlaneSvg);

    if (els.swimlaneSvg.setAttribute) {
      els.swimlaneSvg.setAttribute('viewBox', '0 0 800 64');
    }

    var createEl = document.createElementNS
      ? function (t) { return document.createElementNS('http://www.w3.org/2000/svg', t); }
      : function (t) { return document.createElement(t); };

    var line1 = createEl('line');
    line1.setAttribute('x1', '0');
    line1.setAttribute('y1', '22');
    line1.setAttribute('x2', '800');
    line1.setAttribute('y2', '22');
    line1.setAttribute('class', 'swimlane-lane-line');
    els.swimlaneSvg.appendChild(line1);

    var line2 = createEl('line');
    line2.setAttribute('x1', '0');
    line2.setAttribute('y1', '42');
    line2.setAttribute('x2', '800');
    line2.setAttribute('y2', '42');
    line2.setAttribute('class', 'swimlane-lane-line');
    els.swimlaneSvg.appendChild(line2);

    var labels = [
      { text: 'Input', y: 14 },
      { text: 'Model', y: 34 },
      { text: 'Tools', y: 54 }
    ];
    labels.forEach(function (l) {
      var txt = createEl('text');
      txt.setAttribute('x', '10');
      txt.setAttribute('y', String(l.y));
      txt.setAttribute('class', 'swimlane-lane-label');
      txt.textContent = l.text;
      els.swimlaneSvg.appendChild(txt);
    });

    if (!state.trajectory || !state.trajectory.steps || !state.trajectory.steps.length) return;

    var steps = state.trajectory.steps;
    var totalSteps = steps.length;
    var trackW = 720;
    var stepW = Math.max(14, Math.min(42, Math.floor((trackW / Math.max(totalSteps, 16)) - 4)));
    var gap = Math.max(4, Math.min(8, Math.floor((trackW - (totalSteps * stepW)) / Math.max(totalSteps + 1, 1))));
    var stepSpacing = stepW + Math.max(3, Math.min(6, gap));

    for (var i = 0; i < steps.length; i++) {
      var step = steps[i];
      var x = 60 + i * stepSpacing;
      if (x + stepW > 795) break;
      var rect = createEl('rect');
      rect.setAttribute('x', String(x));

      if (step.lane === 'input') {
        rect.setAttribute('y', '6');
        rect.setAttribute('width', '8');
        rect.setAttribute('height', '12');
        rect.setAttribute('rx', '2');
        rect.setAttribute('class', 'swimlane-bar-input');
        rect.setAttribute('fill', '#c084fc');
      } else if (step.lane === 'model') {
        rect.setAttribute('y', '26');
        rect.setAttribute('width', String(stepW));
        rect.setAttribute('height', '12');
        rect.setAttribute('rx', '2');
        rect.setAttribute('class', 'swimlane-bar-model');
        rect.setAttribute('fill', '#fb923c');
      } else {
        rect.setAttribute('y', '46');
        rect.setAttribute('width', String(stepW));
        rect.setAttribute('height', '12');
        rect.setAttribute('rx', '2');
        rect.setAttribute('class', 'swimlane-bar-tools');
        rect.setAttribute('fill', '#34d399');
      }

      if (step.label) {
        var titleEl = createEl('title');
        titleEl.textContent = String(step.label);
        rect.appendChild(titleEl);
      }

      if (step.callId) {
        rect.setAttribute('data-call-id', step.callId);
        rect.style.cursor = 'pointer';
        (function (cid) {
          rect.addEventListener('click', function () {
            openToolInspector(cid);
          });
        })(step.callId);
      }

      els.swimlaneSvg.appendChild(rect);
    }
  }

  function openToolInspector(callId) {
    if (!els.toolInspector) return;
    var record = (state.trajectory && state.trajectory.toolCalls && state.trajectory.toolCalls[callId])
      ? state.trajectory.toolCalls[callId]
      : { id: callId, name: 'Tool', status: 'running', rawInput: null, rawOutput: null };
    els.toolInspector.dataset.activeCallId = callId;

    if (els.inspectorTitle) {
      text(els.inspectorTitle, record.name || 'Tool Call Details');
    }
    if (els.inspectorStatus) {
      var st = record.status || 'running';
      if (st === 'in_progress') st = 'running';
      text(els.inspectorStatus, st);
      els.inspectorStatus.className = 'inspector-status tool-status-badge ' + (st === 'completed' ? 'status-completed' : (st === 'failed' ? 'status-failed' : 'status-running'));
    }
    if (els.inspectorArgs) {
      var rawIn = record.rawInput;
      var formattedIn = '';
      if (rawIn != null) {
        if (typeof rawIn === 'string') {
          formattedIn = stripAnsi(rawIn);
        } else {
          try {
            formattedIn = JSON.stringify(rawIn, null, 2);
          } catch (_) {
            formattedIn = String(rawIn);
          }
        }
      }
      text(els.inspectorArgs, formattedIn || '(none)');
    }
    if (els.inspectorOutput) {
      var rawOut = record.rawOutput;
      var formattedOut = formatToolPayload(rawOut);
      text(els.inspectorOutput, formattedOut || '(no output yet)');
    }

    if (els.btnCloseInspector && !els.btnCloseInspector._bound) {
      els.btnCloseInspector.addEventListener('click', closeToolInspector);
      els.btnCloseInspector._bound = true;
    }
    els.toolInspector.hidden = false;
  }

  function closeToolInspector() {
    if (els.toolInspector) {
      els.toolInspector.hidden = true;
      delete els.toolInspector.dataset.activeCallId;
    }
  }

  function fallbackCopy(str) {
    try {
      var ta = document.createElement('textarea');
      ta.value = str;
      ta.style.position = 'fixed';
      ta.style.opacity = '0';
      document.body.appendChild(ta);
      ta.select();
      document.execCommand('copy');
      document.body.removeChild(ta);
      showToast('Tool output copied to clipboard', false);
    } catch (_) {
      showToast('Failed to copy output', true);
    }
  }

  function copyToolOutput() {
    if (!els.inspectorOutput) return;
    var textToCopy = els.inspectorOutput.textContent || '';
    if (!textToCopy) return;
    if (navigator.clipboard && navigator.clipboard.writeText) {
      navigator.clipboard.writeText(textToCopy).then(function () {
        showToast('Tool output copied to clipboard', false);
      }).catch(function () {
        fallbackCopy(textToCopy);
      });
    } else {
      fallbackCopy(textToCopy);
    }
  }

  function filterTrajectoryRows() {
    if (!els.trajectoryStream) return;
    var q = (state.trajectory && state.trajectory.filterQuery) || '';
    var rows = els.trajectoryStream.querySelectorAll('.trajectory-row');
    rows.forEach(function (row) {
      if (!q) {
        row.hidden = false;
      } else {
        var rowText = (row.textContent || '').toLowerCase();
        row.hidden = rowText.indexOf(q) === -1;
      }
    });
  }

  var cacheTrajectoryTimer = null;
  function scheduleCacheTrajectory(itemId) {
    if (cacheTrajectoryTimer) return;
    cacheTrajectoryTimer = setTimeout(function () {
      cacheTrajectoryTimer = null;
      cacheTrajectory(itemId);
    }, 1000);
  }

  function cacheTrajectory(itemId) {
    if (cacheTrajectoryTimer) {
      clearTimeout(cacheTrajectoryTimer);
      cacheTrajectoryTimer = null;
    }
    if (!itemId || !state.trajectory) return;
    state.trajectoryCache = state.trajectoryCache || {};
    state.trajectoryCache[itemId] = {
      trajectory: {
        startTime: state.trajectory.startTime,
        startedAt: state.trajectory.startedAt,
        finishedAt: state.trajectory.finishedAt,
        turns: state.trajectory.turns,
        calls: state.trajectory.calls,
        steps: state.trajectory.steps ? state.trajectory.steps.slice() : [],
        intervals: {
          input: (state.trajectory.intervals && state.trajectory.intervals.input) ? state.trajectory.intervals.input.slice() : [],
          model: (state.trajectory.intervals && state.trajectory.intervals.model) ? state.trajectory.intervals.model.slice() : [],
          tools: (state.trajectory.intervals && state.trajectory.intervals.tools) ? state.trajectory.intervals.tools.slice() : []
        },
        toolIntervals: Object.assign({}, state.trajectory.toolIntervals || {}),
        toolCalls: Object.assign({}, state.trajectory.toolCalls || {}),
        filterQuery: state.trajectory.filterQuery || ''
      },
      streamHtml: els.trajectoryStream ? els.trajectoryStream.innerHTML : '',
      durationText: els.metricDuration ? els.metricDuration.textContent : '',
      turnsText: els.metricTurns ? els.metricTurns.textContent : '',
      callsText: els.metricCalls ? els.metricCalls.textContent : '',
      statusText: els.agentStatusPill ? els.agentStatusPill.textContent : '',
      statusClass: els.agentStatusPill ? els.agentStatusPill.className : ''
    };
  }

  function updateDurationDisplay() {
    if (!state.trajectory || !els.metricDuration) return;
    var startedAt = state.trajectory.startedAt;
    var finishedAt = state.trajectory.finishedAt;
    if (startedAt != null) {
      var s = finishedAt != null ? Math.max(0, finishedAt - startedAt) : Math.max(0, (Date.now() / 1000) - startedAt);
      text(els.metricDuration, 'Duration: ' + s.toFixed(1) + 's');
    } else if (state.trajectory.startTime) {
      var elapsed = ((Date.now() - state.trajectory.startTime) / 1000).toFixed(1) + 's';
      text(els.metricDuration, 'Duration: ' + elapsed);
    }
  }

  function closeAgentDrawer() {
    if (state.currentStreamingId) {
      cacheTrajectory(state.currentStreamingId);
    }
    if (els.agentDrawer) {
      els.agentDrawer.hidden = true;
    }
    if (state.currentStream) {
      state.currentStream.close();
      state.currentStream = null;
    }
    state.currentStreamingId = null;
    if (state.trajectory && state.trajectory.timer) {
      clearInterval(state.trajectory.timer);
      state.trajectory.timer = null;
    }
    closeToolInspector();
    setReaderTab('spec');
  }

  function openAgentDrawer(itemId) {
    state.trajectoryCache = state.trajectoryCache || {};

    if (state.currentStreamingId && state.currentStreamingId !== itemId) {
      cacheTrajectory(state.currentStreamingId);
    }

    if (state.currentStream) {
      state.currentStream.close();
      state.currentStream = null;
    }
    state.currentStreamingId = itemId;

    if (state.trajectory && state.trajectory.timer) {
      clearInterval(state.trajectory.timer);
      state.trajectory.timer = null;
    }

    if (state.viewMode !== 'detail') {
      setViewMode('detail');
    }
    if (state.selected !== itemId) {
      state.selected = itemId;
      applyRoute();
    }

    state.readerTab = 'trajectory';
    if (els.tabAgentTrajectory) els.tabAgentTrajectory.classList.add('active');
    if (els.tabTaskSpec) els.tabTaskSpec.classList.remove('active');
    if (els.proposal) els.proposal.hidden = true;
    if (els.agentDrawer) els.agentDrawer.hidden = false;

    var itm = find(itemId);
    if (els.agentTaskTitle) text(els.agentTaskTitle, (itm && itm.title) ? itm.title : '');
    if (els.agentItemId) {
      text(els.agentItemId, itemId.slice(0, 8));
      els.agentItemId.dataset.fullId = itemId;
    }
    if (els.agentLogInfo) {
      text(els.agentLogInfo, 'Logs: .pin_vault/runs/' + itemId + '.log');
    }

    var isRunning = state.activeRuns && state.activeRuns.has(itemId);
    var itemObj = find(itemId);
    var itemSt = itemObj ? itemStatus(itemObj) : '';
    if (els.agentStatusPill) {
      if (isRunning) {
        els.agentStatusPill.className = 'agent-status-pill';
        text(els.agentStatusPill, 'Running');
      } else if (itemSt === 'in_progress') {
        els.agentStatusPill.className = 'agent-status-pill status-interrupted';
        text(els.agentStatusPill, 'Interrupted');
      } else if (itemSt === 'done' || itemSt === 'review' || itemSt === 'closed') {
        els.agentStatusPill.className = 'agent-status-pill status-completed';
        text(els.agentStatusPill, 'Completed');
      } else {
        els.agentStatusPill.className = 'agent-status-pill';
        text(els.agentStatusPill, 'Idle');
      }
    }
    if (els.btnCancelAgent) {
      els.btnCancelAgent.disabled = !isRunning;
    }

    var planSection = $('agent-plan-section');
    if (planSection) planSection.hidden = true;
    clear(els.agentPlanList);
    clear(els.agentThoughts);
    clear(els.agentTools);
    if (els.btnToggleTools) {
      els.btnToggleTools.hidden = true;
      text(els.btnToggleTools, 'Expand all');
    }

    closeToolInspector();
    var cached = (!isRunning && state.trajectoryCache[itemId]) ? state.trajectoryCache[itemId] : null;
    var timingStartedAt = (state.activeRunsTimings && state.activeRunsTimings[itemId]) || null;
    if (cached) {
      state.trajectory = {
        startTime: cached.trajectory.startTime,
        startedAt: cached.trajectory.startedAt != null ? cached.trajectory.startedAt : timingStartedAt,
        finishedAt: cached.trajectory.finishedAt,
        turns: cached.trajectory.turns || 0,
        calls: cached.trajectory.calls || 0,
        timer: null,
        steps: cached.trajectory.steps ? cached.trajectory.steps.slice() : [],
        activeAssistantRow: null,
        activeAssistantTextEl: null,
        intervals: {
          input: (cached.trajectory.intervals && cached.trajectory.intervals.input) ? cached.trajectory.intervals.input.slice() : [],
          model: (cached.trajectory.intervals && cached.trajectory.intervals.model) ? cached.trajectory.intervals.model.slice() : [],
          tools: (cached.trajectory.intervals && cached.trajectory.intervals.tools) ? cached.trajectory.intervals.tools.slice() : []
        },
        currentModelInterval: null,
        toolIntervals: Object.assign({}, cached.trajectory.toolIntervals || {}),
        toolCalls: Object.assign({}, cached.trajectory.toolCalls || {}),
        toolRows: {},
        filterQuery: ''
      };
      updateDurationDisplay();
      if (els.metricTurns) text(els.metricTurns, cached.turnsText || 'Turns: ' + state.trajectory.turns);
      if (els.metricCalls) text(els.metricCalls, cached.callsText || 'Calls: ' + state.trajectory.calls);
      if (els.agentStatusPill) {
        text(els.agentStatusPill, cached.statusText || 'Completed');
        els.agentStatusPill.className = cached.statusClass || 'agent-status-pill status-completed';
      }
      if (els.btnCancelAgent) els.btnCancelAgent.disabled = true;
      if (els.trajectoryStream) {
        els.trajectoryStream.innerHTML = cached.streamHtml || '';
      }
      renderSwimlaneSvg();
      return;
    } else {
      clear(els.trajectoryStream);
      var now = Date.now();
      state.trajectory = {
        startTime: timingStartedAt ? (timingStartedAt * 1000) : now,
        startedAt: timingStartedAt,
        finishedAt: null,
        turns: 0,
        calls: 0,
        timer: null,
        steps: [{ lane: 'input', label: 'Start' }],
        activeAssistantRow: null,
        activeAssistantTextEl: null,
        intervals: {
          input: [{ start: now, end: now + 50 }],
          model: [],
          tools: []
        },
        currentModelInterval: null,
        toolIntervals: {},
        toolCalls: {},
        toolRows: {},
        filterQuery: ''
      };

      appendTrajectoryContextRow('Agent run started for ' + itemId);
      renderSwimlaneSvg();
      updateDurationDisplay();
      if (els.metricTurns) text(els.metricTurns, 'Turns: 0');
      if (els.metricCalls) text(els.metricCalls, 'Calls: 0');
    }

    if (isRunning) {
      updateDurationDisplay();
      state.trajectory.timer = setInterval(function () {
        updateDurationDisplay();
      }, 1000);
    }

    var receivedEvents = 0;
    var streamUrl = BASE + 'items/' + encodeURIComponent(itemId) + '/stream';
    var source = new EventSource(streamUrl);
    state.currentStream = source;

    source.onmessage = function (e) {
      try {
        var data = JSON.parse(e.data);
        if (receivedEvents === 0 && cached) {
          clear(els.trajectoryStream);
          var now = Date.now();
          state.trajectory = {
            startTime: now,
            turns: 0,
            calls: 0,
            timer: null,
            steps: [{ lane: 'input', label: 'Start' }],
            activeAssistantRow: null,
            activeAssistantTextEl: null,
            intervals: {
              input: [{ start: now, end: now + 50 }],
              model: [],
              tools: []
            },
            currentModelInterval: null,
            toolIntervals: {},
            toolCalls: {},
            toolRows: {},
            filterQuery: ''
          };
          appendTrajectoryContextRow('Agent run started for ' + itemId);
        }
        receivedEvents++;
        handleAgentStreamEvent(data);
        scheduleCacheTrajectory(itemId);
      } catch (err) {
        console.error('Error parsing SSE event:', err);
      }
    };

    source.onerror = function () {
      if (!state.activeRuns.has(itemId)) {
        updateDurationDisplay();
        if (state.trajectory && state.trajectory.startTime && els.metricDuration) {
          var elapsed = ((Date.now() - state.trajectory.startTime) / 1000).toFixed(1) + 's';
          text(els.metricDuration, 'Duration: ' + elapsed);
        }
        if (els.agentStatusPill && els.agentStatusPill.textContent === 'Running') {
          text(els.agentStatusPill, 'Completed');
          els.agentStatusPill.className = 'agent-status-pill status-completed';
          appendTrajectoryContextRow('Agent run completed.');
          renderSwimlaneSvg();
        }
        if (els.btnCancelAgent) els.btnCancelAgent.disabled = true;
        source.close();
        if (state.currentStream === source) state.currentStream = null;

        if (receivedEvents === 0) {
          fetch(BASE + 'items/' + encodeURIComponent(itemId) + '/log')
            .then(function (res) {
              if (res.status === 200) {
                var logLink = node('span');
                text(logLink, 'Log available: ');
                var a = node('a');
                a.href = BASE + 'items/' + encodeURIComponent(itemId) + '/log';
                a.target = '_blank';
                a.rel = 'noopener';
                text(a, '.pin_vault/runs/' + itemId + '.log');
                logLink.appendChild(a);
                appendTrajectoryContextRow(logLink);
                if (els.agentLogInfo) {
                  text(els.agentLogInfo, 'Logs: .pin_vault/runs/' + itemId + '.log');
                }
                cacheTrajectory(itemId);
              }
            })
            .catch(function () {});
        } else {
          cacheTrajectory(itemId);
        }
      }
    };
  }

  function handleAgentStreamEvent(data) {
    if (!data) return;

    if (data.type === 'run_timing') {
      if (state.trajectory) {
        if (data.startedAt != null) {
          state.trajectory.startedAt = data.startedAt;
          state.trajectory.startTime = data.startedAt * 1000;
        }
        if (data.finishedAt != null) {
          state.trajectory.finishedAt = data.finishedAt;
        }
        updateDurationDisplay();
      }
      return;
    }

    if (data.type === 'finished') {
      if (state.trajectory) {
        if (data.startedAt != null) state.trajectory.startedAt = data.startedAt;
        if (data.finishedAt != null) state.trajectory.finishedAt = data.finishedAt;
        if (state.trajectory.timer) {
          clearInterval(state.trajectory.timer);
          state.trajectory.timer = null;
        }
        updateDurationDisplay();
      }
      if (state.activeRuns) state.activeRuns.delete(state.currentStreamingId);
      if (els.readerTrajectoryBadge) els.readerTrajectoryBadge.hidden = true;
      appendTrajectoryContextRow('Agent run completed.');
      if (els.agentStatusPill) {
        text(els.agentStatusPill, 'Completed');
        els.agentStatusPill.className = 'agent-status-pill status-completed';
      }
      if (els.btnCancelAgent) els.btnCancelAgent.disabled = true;
      if (state.currentStream) {
        state.currentStream.close();
        state.currentStream = null;
      }
      renderSwimlaneSvg();
      updateActiveRuns();
      refreshData();
      cacheTrajectory(state.currentStreamingId);
      return;
    }

    if (data.type === 'error') {
      if (state.trajectory) {
        if (data.startedAt != null) state.trajectory.startedAt = data.startedAt;
        if (data.finishedAt != null) state.trajectory.finishedAt = data.finishedAt;
        if (state.trajectory.timer) {
          clearInterval(state.trajectory.timer);
          state.trajectory.timer = null;
        }
        updateDurationDisplay();
      }
      if (state.activeRuns) state.activeRuns.delete(state.currentStreamingId);
      if (els.readerTrajectoryBadge) els.readerTrajectoryBadge.hidden = true;
      var errMsg = data.message || data.error || 'Error';
      appendTrajectoryContextRow('Agent run failed: ' + errMsg);
      if (els.agentStatusPill) {
        text(els.agentStatusPill, 'Failed: ' + errMsg);
        els.agentStatusPill.className = 'agent-status-pill status-failed';
      }
      if (els.btnCancelAgent) els.btnCancelAgent.disabled = true;
      if (state.currentStream) {
        state.currentStream.close();
        state.currentStream = null;
      }
      renderSwimlaneSvg();
      updateActiveRuns();
      refreshData();
      cacheTrajectory(state.currentStreamingId);
      return;
    }

    var update = (data.params && data.params.update) ? data.params.update : (data.update || data);
    var updateType = (update && typeof update === 'object' && (update.sessionUpdate || update.type)) || (data.params && (data.params.sessionUpdate || data.params.type)) || data.sessionUpdate || data.type;

    if (updateType === 'plan') {
      renderAgentPlan(update);
    } else if (updateType === 'agent_thought_chunk' || updateType === 'agent_message_chunk') {
      var rawThought = extractThoughtContent(update);
      appendAgentThought(rawThought);

      var cleaned = cleanThoughtText(rawThought);
      if (!state.trajectory) return;

      if (!state.trajectory.activeAssistantRow) {
        state.trajectory.turns++;
        if (els.metricTurns) {
          text(els.metricTurns, 'Turns: ' + state.trajectory.turns);
        }
        var row = node('div', 'trajectory-row trajectory-row-assistant');
        var gutter = node('div', 'trajectory-gutter', '●');
        var badge = node('span', 'trajectory-badge badge-assistant', 'ASSISTANT');
        var body = node('div', 'trajectory-body');
        var thoughtEl = node('div', 'trajectory-thought-text');
        body.appendChild(thoughtEl);
        row.appendChild(gutter);
        row.appendChild(badge);
        row.appendChild(body);

        if (els.trajectoryStream) {
          els.trajectoryStream.appendChild(row);
        }
        state.trajectory.activeAssistantRow = row;
        state.trajectory.activeAssistantTextEl = thoughtEl;

        var mNow = Date.now();
        var modelInterval = { start: mNow, end: mNow };
        state.trajectory.intervals.model.push(modelInterval);
        state.trajectory.currentModelInterval = modelInterval;

        var lastStep = state.trajectory.steps && state.trajectory.steps[state.trajectory.steps.length - 1];
        if (!lastStep || lastStep.lane !== 'model') {
          if (!state.trajectory.steps) state.trajectory.steps = [];
          state.trajectory.steps.push({ lane: 'model', label: 'Turn ' + state.trajectory.turns });
          renderSwimlaneSvg();
        }
      }

      if (cleaned && state.trajectory.activeAssistantTextEl) {
        var aNodes = state.trajectory.activeAssistantTextEl.childNodes || state.trajectory.activeAssistantTextEl.children;
        var aLast = aNodes && aNodes.length > 0 ? aNodes[aNodes.length - 1] : null;
        if (aLast && (aLast.nodeType === 3 || typeof aLast.textContent === 'string') && aLast.parentNode === state.trajectory.activeAssistantTextEl) {
          aLast.textContent = (aLast.textContent || '') + cleaned;
          if (aLast.nodeValue !== undefined) aLast.nodeValue = aLast.textContent;
        } else {
          state.trajectory.activeAssistantTextEl.appendChild(document.createTextNode(cleaned));
        }
      }
      scrollTrajectoryStreamToBottom();

      if (state.trajectory.currentModelInterval) {
        state.trajectory.currentModelInterval.end = Date.now();
      }

      if (state.trajectory.filterQuery && state.trajectory.activeAssistantRow) {
        var rText = (state.trajectory.activeAssistantRow.textContent || '').toLowerCase();
        state.trajectory.activeAssistantRow.hidden = rText.indexOf(state.trajectory.filterQuery) === -1;
      }
    } else if (updateType === 'tool_call') {
      if (!state.trajectory) return;

      if (state.trajectory.currentModelInterval) {
        state.trajectory.currentModelInterval.end = Date.now();
        state.trajectory.currentModelInterval = null;
      }
      state.trajectory.activeAssistantRow = null;
      state.trajectory.activeAssistantTextEl = null;

      state.trajectory.calls++;
      if (els.metricCalls) {
        text(els.metricCalls, 'Calls: ' + state.trajectory.calls);
      }

      var callId = update.toolCallId || update.id || ('call_' + Date.now() + '_' + Math.random().toString(36).slice(2, 7));
      var tNow = Date.now();
      var toolInterval = { id: callId, start: tNow, end: tNow };
      state.trajectory.intervals.tools.push(toolInterval);
      state.trajectory.toolIntervals[callId] = toolInterval;

      state.trajectory.toolCalls[callId] = {
        id: callId,
        name: formatToolTitle(update),
        title: update.title || '',
        rawInput: update.rawInput || update.input || update.arguments || update.params || update.args || null,
        rawOutput: update.rawOutput || update.output || update.result || update.content || null,
        status: update.status || 'running',
        startTime: tNow,
        endTime: null
      };

      var inline = formatToolCallInline(update);
      var row = node('div', 'trajectory-row trajectory-row-tool');
      row.dataset.callId = callId;
      if (state.trajectory && state.trajectory.toolRows) state.trajectory.toolRows[callId] = row;
      var gutter = node('div', 'trajectory-gutter', '●');
      var badge = node('span', 'trajectory-badge badge-tool', 'TOOL');
      var body = node('div', 'trajectory-body');
      var toolLine = node('div', 'trajectory-tool-line');
      toolLine.dataset.callId = callId;

      var nameEl = node('span', 'tool-name', inline.name);
      var argsEl = node('span', 'tool-args', inline.argsStr);
      var arrowEl = node('span', 'tool-arrow', '→');
      var previewEl = node('span', 'tool-result-preview', inline.previewStr);
      var stClass = inline.status === 'completed' ? 'status-completed' : (inline.status === 'failed' ? 'status-failed' : 'status-running');
      var statusBadge = node('span', 'tool-status-badge ' + stClass, inline.status);

      toolLine.appendChild(nameEl);
      toolLine.appendChild(argsEl);
      toolLine.appendChild(arrowEl);
      toolLine.appendChild(previewEl);
      toolLine.appendChild(statusBadge);

      row.classList.add('clickable-tool-row');
      toolLine.addEventListener('click', function (e) {
        if (e && e.stopPropagation) e.stopPropagation();
        openToolInspector(callId);
      });
      row.addEventListener('click', function () {
        openToolInspector(callId);
      });

      body.appendChild(toolLine);
      row.appendChild(gutter);
      row.appendChild(badge);
      row.appendChild(body);

      if (els.trajectoryStream) {
        els.trajectoryStream.appendChild(row);
        scrollTrajectoryStreamToBottom();
      }
      if (state.trajectory.filterQuery) {
        var rText = (row.textContent || '').toLowerCase();
        row.hidden = rText.indexOf(state.trajectory.filterQuery) === -1;
      }

      renderAgentToolCall(update);

      if (!state.trajectory.steps) state.trajectory.steps = [];
      state.trajectory.steps.push({ lane: 'tools', label: inline.name, callId: callId });
      renderSwimlaneSvg();
    } else if (updateType === 'tool_call_update') {
      if (!state.trajectory) return;

      var callId = update.toolCallId || update.id;
      if (!callId) return;

      var record = state.trajectory.toolCalls[callId];
      if (!record) {
        record = {
          id: callId,
          name: formatToolTitle(update),
          status: update.status || 'completed',
          startTime: Date.now(),
          endTime: null
        };
        state.trajectory.toolCalls[callId] = record;
      }

      if (update.rawOutput || update.output || update.result || update.error || update.content) {
        record.rawOutput = update.rawOutput || update.output || update.result || update.error || update.content;
      }
      if (update.rawInput || update.input || update.arguments || update.params) {
        record.rawInput = update.rawInput || update.input || update.arguments || update.params || update.args;
      }
      if (update.status) {
        record.status = update.status;
      }
      record.endTime = Date.now();

      var toolIv = state.trajectory.toolIntervals[callId];
      if (toolIv) {
        toolIv.end = Date.now();
      }

      if (els.trajectoryStream) {
        var toolRow = (state.trajectory && state.trajectory.toolRows && state.trajectory.toolRows[callId]) || null;
        if (!toolRow) {
          try {
            toolRow = els.trajectoryStream.querySelector('[data-call-id="' + callId + '"]');
          } catch (_) {}
          if (!toolRow && els.trajectoryStream.children) {
            for (var rIdx = 0; rIdx < els.trajectoryStream.children.length; rIdx++) {
              var cEl = els.trajectoryStream.children[rIdx];
              if (cEl && cEl.dataset && cEl.dataset.callId === callId) {
                toolRow = cEl;
                break;
              }
            }
          }
          if (toolRow && state.trajectory && state.trajectory.toolRows) {
            state.trajectory.toolRows[callId] = toolRow;
          }
        }
        if (toolRow) {
          var inline = formatToolCallInline(record);
          var previewEl = toolRow.querySelector('.tool-result-preview');
          if (previewEl) text(previewEl, inline.previewStr);
          var statusBadge = toolRow.querySelector('.tool-status-badge');
          if (statusBadge) {
            var st = record.status || 'completed';
            if (st === 'in_progress') st = 'running';
            text(statusBadge, st);
            statusBadge.className = 'tool-status-badge ' + (st === 'completed' ? 'status-completed' : (st === 'failed' ? 'status-failed' : 'status-running'));
          }
          var argsEl = toolRow.querySelector('.tool-args');
          if (argsEl && !argsEl.textContent) text(argsEl, inline.argsStr);
        }
      }

      if (els.toolInspector && !els.toolInspector.hidden && els.toolInspector.dataset.activeCallId === callId) {
        openToolInspector(callId);
      }

      updateAgentToolCall(update);
    }
  }

  function renderAgentPlan(update) {
    if (!els.agentPlanList) return;
    var planSection = $('agent-plan-section');
    var entries = Array.isArray(update.entries) ? update.entries : (Array.isArray(update.plan) ? update.plan : []);
    if (!entries.length) {
      if (planSection) planSection.hidden = true;
      return;
    }
    if (planSection) planSection.hidden = false;
    clear(els.agentPlanList);
    entries.forEach(function (entry) {
      var li = node('li', 'agent-plan-item' + (entry.completed || entry.status === 'completed' ? ' completed' : ''));
      var cb = node('input');
      cb.type = 'checkbox';
      cb.disabled = true;
      cb.checked = !!(entry.completed || entry.status === 'completed');
      li.appendChild(cb);
      li.appendChild(document.createTextNode(entry.content || entry.text || entry.title || JSON.stringify(entry)));
      els.agentPlanList.appendChild(li);
    });
  }

  function stripAnsi(str) {
    if (!str) return '';
    return String(str).replace(/\x1b\[[0-9;]*[a-zA-Z]|\u001b\[[0-9;]*[a-zA-Z]/g, '');
  }

  function cleanThoughtText(thoughtText) {
    if (!thoughtText) return '';
    var str = typeof thoughtText === 'string' ? thoughtText : String(thoughtText);
    return stripAnsi(str)
      .replace(/<(?:\/)?(?:think|thought|thinking)(?:\s+[^>]*)?>/gi, '')
      .replace(/\r\n?/g, '\n')
      .replace(/[ \t]+$/gm, '')
      .replace(/\n{3,}/g, '\n\n');
  }

  function extractThoughtContent(update) {
    if (!update) return '';
    if (typeof update === 'string') return update;
    if (update.text) return update.text;
    if (update.thought) return update.thought;
    if (update.content) {
      if (typeof update.content === 'string') return update.content;
      if (Array.isArray(update.content)) {
        return update.content.map(function (c) {
          if (!c) return '';
          if (typeof c === 'string') return c;
          return c.text || c.thought || (c.content && c.content.text) || '';
        }).join('');
      }
      if (typeof update.content === 'object') {
        return update.content.text || update.content.thought || '';
      }
    }
    return '';
  }

  function appendAgentThought(thoughtText) {
    if (!els.agentThoughts || !thoughtText) return;
    var cleaned = cleanThoughtText(thoughtText);
    if (!cleaned) return;

    var aNodes = els.agentThoughts.childNodes || els.agentThoughts.children;
    var lastChild = aNodes && aNodes.length > 0 ? aNodes[aNodes.length - 1] : null;
    var lastText = (lastChild && (lastChild.nodeValue || lastChild.textContent)) || '';
    if (!lastText && !els.agentThoughts.textContent) {
      cleaned = cleaned.replace(/^\n+/, '');
    } else if (lastText.endsWith('\n\n')) {
      cleaned = cleaned.replace(/^\n+/, '');
    } else if (lastText.endsWith('\n')) {
      cleaned = cleaned.replace(/^\n{2,}/, '\n');
    }

    if (!cleaned) return;
    if (lastChild && (lastChild.nodeType === 3 || typeof lastChild.textContent === 'string') && lastChild.parentNode === els.agentThoughts) {
      lastChild.textContent = (lastChild.textContent || '') + cleaned;
      if (lastChild.nodeValue !== undefined) lastChild.nodeValue = lastChild.textContent;
    } else {
      els.agentThoughts.appendChild(document.createTextNode(cleaned));
    }
    scrollAgentThoughtsToBottom();
  }

  function formatToolTitle(update) {
    if (update.title && update.title.trim()) return stripAnsi(update.title.trim());
    if (update.name) {
      return stripAnsi(update.kind ? (update.kind + ': ' + update.name) : update.name);
    }
    return stripAnsi(update.tool || 'tool');
  }

  function formatToolPayload(data) {
    if (data === null || data === undefined) return '';
    if (typeof data === 'string') return stripAnsi(data).trim();
    if (Array.isArray(data)) {
      return data.map(function (item) {
        if (item && item.type === 'content' && item.content && item.content.text) return stripAnsi(item.content.text);
        if (item && item.text) return stripAnsi(item.text);
        if (item && item.type === 'diff') return stripAnsi((item.path ? item.path + '\n' : '') + (item.newText || ''));
        return typeof item === 'object' ? JSON.stringify(item, null, 2) : stripAnsi(String(item));
      }).join('\n').trim();
    }
    if (typeof data === 'object') {
      if (Array.isArray(data.content)) {
        return formatToolPayload(data.content);
      }
      if (data.command) return '$ ' + stripAnsi(data.command);
      if (data.path) return stripAnsi(data.path) + (data.pattern ? ' [grep: ' + stripAnsi(data.pattern) + ']' : '');
      try { return JSON.stringify(data, null, 2); } catch (_) { return stripAnsi(String(data)); }
    }
    return stripAnsi(String(data)).trim();
  }

  function renderAgentToolCall(update) {
    if (!els.agentTools) return;
    var callId = update.toolCallId || update.id || ('tool-' + Date.now());
    var card = $('agent-tool-' + callId);
    if (!card) {
      card = node('details', 'agent-tool-card');
      card.id = 'agent-tool-' + callId;
      card.open = false;

      var summary = node('summary', 'agent-tool-header');
      var titleWrap = node('div', 'agent-tool-title-wrap');
      titleWrap.appendChild(node('span', 'agent-tool-chevron', '▸'));
      titleWrap.appendChild(node('span', 'agent-tool-name', formatToolTitle(update)));
      summary.appendChild(titleWrap);

      var statusBadge = node('span', 'agent-tool-status ' + (update.status || 'in_progress'), update.status || 'in_progress');
      summary.appendChild(statusBadge);
      card.appendChild(summary);

      var contentDiv = node('div', 'agent-tool-content');

      var inputDiv = node('div', 'agent-tool-input');
      var rawIn = update.rawInput || update.input || update.arguments || update.params;
      var formattedIn = formatToolPayload(rawIn);
      if (formattedIn) text(inputDiv, formattedIn);
      contentDiv.appendChild(inputDiv);

      var body = node('div', 'agent-tool-body');
      contentDiv.appendChild(body);

      card.appendChild(contentDiv);
      card.addEventListener('toggle', updateToggleToolsButtonState);
      els.agentTools.appendChild(card);
      updateToggleToolsButtonState();
    }
  }

  function updateAgentToolCall(update) {
    if (!els.agentTools) return;
    var callId = update.toolCallId || update.id;
    if (!callId) return;
    var card = $('agent-tool-' + callId);
    if (!card) {
      renderAgentToolCall(update);
      card = $('agent-tool-' + callId);
    }
    if (!card) return;

    var nameEl = card.querySelector('.agent-tool-name');
    if (nameEl) {
      var newTitle = formatToolTitle(update);
      if (newTitle !== 'tool') text(nameEl, newTitle);
    }

    var statusBadge = card.querySelector('.agent-tool-status');
    if (statusBadge && update.status) {
      text(statusBadge, update.status);
      statusBadge.className = 'agent-tool-status ' + (update.status === 'completed' ? 'completed' : (update.status === 'failed' ? 'failed' : 'in_progress'));
      if (update.status === 'completed' || update.status === 'failed') {
        card.open = false;
      }
    }

    var inputDiv = card.querySelector('.agent-tool-input');
    if (inputDiv && !inputDiv.textContent) {
      var rawIn = update.rawInput || update.input || update.arguments || update.params;
      var formattedIn = formatToolPayload(rawIn);
      if (formattedIn) text(inputDiv, formattedIn);
    }

    var body = card.querySelector('.agent-tool-body');
    if (body) {
      var content = update.rawOutput || update.output || update.result || update.error || update.content;
      var formatted = formatToolPayload(content);
      if (formatted) {
        text(body, formatted);
      }
    }
    updateToggleToolsButtonState();
  }

  var toggleToolsPending = false;
  function updateToggleToolsButtonState() {
    if (!els.btnToggleTools || !els.agentTools) return;
    if (toggleToolsPending) return;
    toggleToolsPending = true;
    var raf = window.requestAnimationFrame || function (cb) { setTimeout(cb, 0); };
    raf(function () {
      toggleToolsPending = false;
      syncToggleToolsButtonState();
    });
  }

  function syncToggleToolsButtonState() {
    if (!els.btnToggleTools || !els.agentTools) return;
    var cards = els.agentTools.children;
    if (!cards || !cards.length) {
      els.btnToggleTools.hidden = true;
      return;
    }
    var cardCount = 0;
    var allOpen = true;
    for (var i = 0; i < cards.length; i++) {
      var c = cards[i];
      if (c && c.classList && c.classList.contains('agent-tool-card')) {
        cardCount++;
        if (!c.open) allOpen = false;
      }
    }
    if (!cardCount) {
      els.btnToggleTools.hidden = true;
      return;
    }
    els.btnToggleTools.hidden = false;
    text(els.btnToggleTools, allOpen ? 'Collapse all' : 'Expand all');
  }

  function toggleAllToolCards() {
    if (!els.agentTools) return;
    var cards = els.agentTools.querySelectorAll('.agent-tool-card');
    if (!cards.length) return;
    var anyOpen = false;
    cards.forEach(function (c) {
      if (c.open) anyOpen = true;
    });
    var nextOpen = !anyOpen;
    cards.forEach(function (c) {
      c.open = nextOpen;
    });
    updateToggleToolsButtonState();
  }

  function cancelAgentRun(itemId) {
    if (!itemId) {
      if (els.agentItemId && els.agentItemId.dataset.fullId) {
        itemId = els.agentItemId.dataset.fullId;
      } else if (els.agentItemId) {
        itemId = els.agentItemId.textContent.trim();
      }
    }
    if (!itemId) return;
    if (els.btnCancelAgent) els.btnCancelAgent.disabled = true;
    fetch(BASE + 'items/' + encodeURIComponent(itemId) + '/cancel', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'X-Pin-Action': 'true'
      }
    })
    .then(function (res) {
      if (!res.ok) return res.json().then(function (e) { throw new Error(e.error || 'Cancel failed'); });
      return res.json();
    })
    .then(function () {
      showToast('Agent cancelled', false);
      if (state.trajectory && state.trajectory.timer) {
        clearInterval(state.trajectory.timer);
        state.trajectory.timer = null;
      }
      if (state.trajectory && state.trajectory.currentModelInterval) {
        state.trajectory.currentModelInterval.end = Date.now();
        state.trajectory.currentModelInterval = null;
      }
      appendTrajectoryContextRow('Agent run cancelled.');
      renderSwimlaneSvg();
      if (els.agentStatusPill) {
        text(els.agentStatusPill, 'Cancelled');
        els.agentStatusPill.className = 'agent-status-pill status-cancelled';
      }
      if (state.currentStream) {
        state.currentStream.close();
        state.currentStream = null;
      }
      updateActiveRuns();
      refreshData();
    })
    .catch(function (err) {
      showToast('Cancel failed: ' + err.message, true);
      if (els.btnCancelAgent) els.btnCancelAgent.disabled = false;
    });
  }

  function bind() {
    els.search.addEventListener('input', function () { state.filters.text = this.value; renderList(); });
    els.status.addEventListener('change', function () { state.filters.status = this.value; renderList(); });
    els.type.addEventListener('change', function () { state.filters.type = this.value; renderList(); });
    els.kind.addEventListener('change', function () { state.filters.kind = this.value; renderList(); });
    els.priority.addEventListener('change', function () { state.filters.priority = this.value; renderList(); });
    els.project.addEventListener('input', function () { state.filters.project = this.value; renderList(); });

    els.filterToggle.addEventListener('click', function () {
      var open = els.filters.hidden;
      els.filters.hidden = !open;
      this.setAttribute('aria-expanded', String(open));
    });

    els.clear.addEventListener('click', function () {
      state.filters = { text: '', status: '', type: '', kind: '', priority: '', project: '' };
      els.search.value = els.status.value = els.type.value = els.kind.value = els.priority.value = els.project.value = '';
      renderList();
    });

    els.back.addEventListener('click', function () { setRoute(null); });
    window.addEventListener('hashchange', applyRoute);

    if (els.viewBoardBtn) {
      els.viewBoardBtn.addEventListener('click', function () { setViewMode('board'); });
    }
    if (els.viewDetailBtn) {
      els.viewDetailBtn.addEventListener('click', function () { setViewMode('detail'); });
    }
    if (els.tabTaskSpec) {
      els.tabTaskSpec.addEventListener('click', function () { setReaderTab('spec'); });
    }
    if (els.tabAgentTrajectory) {
      els.tabAgentTrajectory.addEventListener('click', function () { setReaderTab('trajectory'); });
    }

    if (els.dialogSubmitBtn) {
      els.dialogSubmitBtn.addEventListener('click', triggerModalSubmit);
    }
    if (els.dialogCloseBtn) {
      els.dialogCloseBtn.addEventListener('click', closeActionModal);
    }
    if (els.dialogCancelBtn) {
      els.dialogCancelBtn.addEventListener('click', closeActionModal);
    }
    if (els.btnCloseAgentDrawer) {
      els.btnCloseAgentDrawer.addEventListener('click', closeAgentDrawer);
    }
    if (els.btnToggleTools) {
      els.btnToggleTools.addEventListener('click', toggleAllToolCards);
    }
    if (els.btnCancelAgent) {
      els.btnCancelAgent.addEventListener('click', function () {
        var idToCancel = state.currentStreamingId;
        if (!idToCancel && els.agentItemId) {
          idToCancel = els.agentItemId.dataset.fullId || els.agentItemId.textContent.trim();
        }
        if (idToCancel) {
          cancelAgentRun(idToCancel);
        }
      });
    }
    if (els.trajectorySearch) {
      els.trajectorySearch.addEventListener('input', function () {
        if (!state.trajectory) return;
        state.trajectory.filterQuery = this.value.toLowerCase().trim();
        filterTrajectoryRows();
      });
    }
    if (els.trajectoryStream) {
      els.trajectoryStream.addEventListener('click', function (e) {
        var r = e.target && e.target.closest && e.target.closest('.trajectory-row-tool');
        if (r && r.dataset && r.dataset.callId) {
          openToolInspector(r.dataset.callId);
        }
      });
    }
    if (els.btnCloseInspector) {
      els.btnCloseInspector.addEventListener('click', closeToolInspector);
    }
    if (els.btnCopyToolOutput) {
      els.btnCopyToolOutput.addEventListener('click', copyToolOutput);
    }
    if (els.btnWorktreeClose) {
      els.btnWorktreeClose.addEventListener('click', closeWorktreeModal);
    }
    if (els.btnWorktreeCancel) {
      els.btnWorktreeCancel.addEventListener('click', closeWorktreeModal);
    }
    if (els.btnWorktreePrimary) {
      els.btnWorktreePrimary.addEventListener('click', function () {
        var item = state.pendingWorktreeItem;
        closeWorktreeModal();
        if (!item) return;
        sendRunItem(item.id, false);
      });
    }
    if (els.btnWorktreeConfirm) {
      els.btnWorktreeConfirm.addEventListener('click', function () {
        var item = state.pendingWorktreeItem;
        closeWorktreeModal();
        if (!item) return;
        // One dispatcher for every run trigger. A success toast only fires on a
        // 200, so failed worktree provisioning is reported as an error.
        sendRunItem(item.id, true, 'Worktree start failed: ');
      });
    }

    if (els.dialogBody) {
      els.dialogBody.addEventListener('keydown', function (e) {
        if (e.key === 'Enter') {
          if (e.target.tagName === 'INPUT') {
            e.preventDefault();
            triggerModalSubmit();
          } else if (e.target.tagName === 'TEXTAREA' && (e.ctrlKey || e.metaKey)) {
            e.preventDefault();
            triggerModalSubmit();
          }
        }
      });
    }

    document.querySelectorAll('.board-col-cards').forEach(function (container) {
      container.addEventListener('dragover', function (e) {
        e.preventDefault();
        e.dataTransfer.dropEffect = 'move';
        container.classList.add('drag-over');
      });
      container.addEventListener('dragleave', function () {
        container.classList.remove('drag-over');
      });
      container.addEventListener('drop', function (e) {
        e.preventDefault();
        container.classList.remove('drag-over');
        var id = e.dataTransfer.getData('text/plain');
        var status = container.dataset.dropStatus;
        if (id && status) handleCardDrop(id, status);
      });
    });
    if (els.quickAddTrigger) {
      els.quickAddTrigger.addEventListener('click', function () {
        expandQuickAdd('title');
      });
    }
    if (els.quickAddCloseBtn) {
      els.quickAddCloseBtn.addEventListener('click', function () {
        collapseQuickAdd();
      });
    }
    if (els.quickAddCancel) {
      els.quickAddCancel.addEventListener('click', function () {
        collapseQuickAdd();
      });
    }
    if (els.quickAddExpandBtn) {
      els.quickAddExpandBtn.addEventListener('click', function () {
        var initial = {
          title: els.quickAddInput ? els.quickAddInput.value : '',
          body: els.quickAddBody ? els.quickAddBody.value : '',
          type: els.quickAddType ? els.quickAddType.value : 'task',
          kind: els.quickAddKind ? els.quickAddKind.value : 'technical',
          priority: els.quickAddPriority ? els.quickAddPriority.value : '',
          tags: els.quickAddTags ? els.quickAddTags.value : ''
        };
        collapseQuickAdd();
        openSpecModal(initial);
      });
    }
    if (els.quickAddInput) {
      els.quickAddInput.addEventListener('keydown', function (e) {
        if (e.key === 'Enter' && !e.metaKey && !e.ctrlKey) {
          e.preventDefault();
          if (els.quickAddBody) {
            els.quickAddBody.focus();
          }
        }
      });
    }
    if (els.quickAddForm) {
      els.quickAddForm.addEventListener('submit', function (e) {
        e.preventDefault();
        submitQuickAdd();
      });
      els.quickAddForm.addEventListener('keydown', function (e) {
        if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
          e.preventDefault();
          submitQuickAdd();
        } else if (e.key === 'Escape') {
          e.preventDefault();
          collapseQuickAdd();
        }
      });
    }

    if (els.btnNewTicket) {
      els.btnNewTicket.addEventListener('click', function () {
        openSpecModal();
      });
    }
    if (els.specModalCloseBtn) {
      els.specModalCloseBtn.addEventListener('click', function () {
        closeSpecModal();
      });
    }
    if (els.specModalCancelBtn) {
      els.specModalCancelBtn.addEventListener('click', function () {
        closeSpecModal();
      });
    }
    if (els.specTabWrite) {
      els.specTabWrite.addEventListener('click', function () {
        setSpecModalTab('write');
      });
    }
    if (els.specTabPreview) {
      els.specTabPreview.addEventListener('click', function () {
        setSpecModalTab('preview');
      });
    }
    if (els.specModalForm) {
      els.specModalForm.addEventListener('submit', function (e) {
        e.preventDefault();
        submitSpecModal();
      });
      els.specModalForm.addEventListener('keydown', function (e) {
        if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
          e.preventDefault();
          submitSpecModal();
        } else if (e.key === 'Escape') {
          e.preventDefault();
          closeSpecModal();
        }
      });
    }
    document.querySelectorAll('.snippet-btn').forEach(function (btn) {
      btn.addEventListener('click', function () {
        var key = btn.dataset.insert;
        var snippet = SPEC_SNIPPETS[key];
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
    if (els.quickAddBody) {
      els.quickAddBody.addEventListener('paste', function (e) {
        handleScreenshotPaste(e, els.quickAddBody);
      });
    }
    if (els.specModalBody) {
      els.specModalBody.addEventListener('paste', function (e) {
        handleScreenshotPaste(e, els.specModalBody);
      });
    }
    if (els.quickAddForm) {
      els.quickAddForm.addEventListener('paste', function (e) {
        if (e.target === els.quickAddBody) return;
        if (e.target && (e.target.tagName === 'INPUT' || e.target.tagName === 'SELECT')) return;
        handleScreenshotPaste(e, els.quickAddBody);
      });
    }
    if (els.specModal) {
      els.specModal.addEventListener('paste', function (e) {
        if (e.target === els.specModalBody) return;
        if (e.target && (e.target.tagName === 'INPUT' || e.target.tagName === 'SELECT')) return;
        setSpecModalTab('write');
        handleScreenshotPaste(e, els.specModalBody);
      });
    }

    document.addEventListener('keydown', function (event) {
      if (els.specModal && (els.specModal.open || els.specModal.hasAttribute('open'))) {
        if (event.key === 'Escape') {
          event.preventDefault();
          closeSpecModal();
          return;
        }
        if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
          event.preventDefault();
          submitSpecModal();
          return;
        }
        return;
      }

      if (els.actionDialog && (els.actionDialog.open || els.actionDialog.hasAttribute('open'))) {
        if (event.key === 'Escape') {
          event.preventDefault();
          closeActionModal();
          return;
        }
        if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
          event.preventDefault();
          triggerModalSubmit();
          return;
        }
        return;
      }

      if (event.key === 'Escape') {
        if (els.quickAddForm && !els.quickAddForm.hidden) {
          event.preventDefault();
          collapseQuickAdd();
          return;
        }
        if (els.worktreeModal && (els.worktreeModal.open || els.worktreeModal.hasAttribute('open'))) {
          event.preventDefault();
          closeWorktreeModal();
          return;
        }
        if (els.toolInspector && !els.toolInspector.hidden) {
          event.preventDefault();
          closeToolInspector();
          return;
        }
        if (els.filters && !els.filters.hidden) {
          event.preventDefault();
          els.filters.hidden = true;
          if (els.filterToggle) els.filterToggle.setAttribute('aria-expanded', 'false');
          return;
        }
        if (state.viewMode === 'detail') {
          event.preventDefault();
          setViewMode('board');
          return;
        }
        var isInput = /^(INPUT|SELECT|TEXTAREA)$/.test(event.target.tagName);
        if (isInput) {
          event.target.blur();
          return;
        }
        if (state.selected) {
          setRoute(null);
          return;
        }
        return;
      }

      var input = /^(INPUT|SELECT|TEXTAREA)$/.test(event.target.tagName);
      if (event.key === '/' && !input) {
        event.preventDefault();
        els.search.focus();
        return;
      }

      var card = event.target && event.target.closest && event.target.closest('.board-card');
      if (card && (event.key === 'ArrowUp' || event.key === 'ArrowDown' || event.key === 'ArrowLeft' || event.key === 'ArrowRight' || event.key === 'Enter')) {
        if (event.key === 'Enter') {
          if (event.target === card || !/^(BUTTON|A|INPUT)$/.test(event.target.tagName)) {
            event.preventDefault();
            card.click();
            return;
          }
        }
        var cardContainer = card.closest('.board-col-cards');
        if (cardContainer) {
          var colCards = Array.from(cardContainer.querySelectorAll('.board-card'));
          var cardIndex = colCards.indexOf(card);

          if (event.key === 'ArrowDown') {
            event.preventDefault();
            if (cardIndex >= 0 && cardIndex < colCards.length - 1) {
              colCards[cardIndex + 1].focus();
            }
            return;
          }
          if (event.key === 'ArrowUp') {
            event.preventDefault();
            if (cardIndex > 0) {
              colCards[cardIndex - 1].focus();
            }
            return;
          }
          if (event.key === 'ArrowRight' || event.key === 'ArrowLeft') {
            event.preventDefault();
            var columns = Array.from(document.querySelectorAll('.board-columns .board-col'));
            var currentCol = card.closest('.board-col');
            var colIndex = columns.indexOf(currentCol);
            if (colIndex !== -1) {
              var step = event.key === 'ArrowRight' ? 1 : -1;
              var targetIdx = colIndex + step;
              while (targetIdx >= 0 && targetIdx < columns.length) {
                var targetCards = Array.from(columns[targetIdx].querySelectorAll('.board-card'));
                if (targetCards.length > 0) {
                  var destIdx = Math.min(cardIndex, targetCards.length - 1);
                  if (destIdx < 0) destIdx = 0;
                  targetCards[destIdx].focus();
                  return;
                }
                targetIdx += step;
              }
            }
            return;
          }
        }
      }

      if (input || event.metaKey || event.ctrlKey || event.altKey) return;
      if (event.key === 'n' || event.key === 'c') {
        event.preventDefault();
        openSpecModal();
        return;
      }
      if (event.key !== 'j' && event.key !== 'k' && event.key !== 'Enter') return;

      var index = state.shown.findIndex(function (item) { return item.id === state.selected; });
      if (event.key === 'j' || event.key === 'k') {
        event.preventDefault();
        index = Math.max(0, Math.min(state.shown.length - 1, (index < 0 ? 0 : index) + (event.key === 'j' ? 1 : -1)));
        if (state.shown[index]) setRoute(state.shown[index].id);
      } else if (event.key === 'Enter' && state.shown[index]) {
        event.preventDefault();
        setRoute(state.shown[index].id);
      }
    });
  }

  function ingest(data) {
    state.items = Array.isArray(data.items) ? data.items : [];
    state.scope = String(data.scope || 'all');
    state.archive = String(data.archive_filter || '');
    state.captured = String(data.captured_at || '');

    text(els.scope, state.scope === 'all' ? 'All projects' : state.scope);
    var serverFilters = data.filters || {};
    var filterBits = [];
    if (serverFilters.status) filterBits.push('status: ' + serverFilters.status);
    if (serverFilters.kind) filterBits.push('kind: ' + serverFilters.kind);
    if (serverFilters.item_type) filterBits.push('type: ' + serverFilters.item_type);
    if (serverFilters.tag) filterBits.push('tag: ' + serverFilters.tag);
    text(els.snapshot, 'Vault: ' + state.scope + (state.archive ? ' · ' + state.archive : '') + (filterBits.length ? ' · ' + filterBits.join(' · ') : ''));
    setupFilters();

    var currentRoute = route();
    if (!currentRoute && !isNarrow() && state.items.length && state.viewMode === 'detail') {
      history.replaceState(null, '', '#/item/' + encodeURIComponent(state.items[0].id));
    }
    setViewMode(state.viewMode);
    applyRoute();
  }

  function refreshData() {
    var headers = {};
    if (state.etag) {
      headers['If-None-Match'] = state.etag;
    }
    fetch(BASE + 'data.json', { credentials: 'same-origin', cache: 'no-cache', headers: headers })
      .then(function (res) {
        if (res.status === 304) {
          return null;
        }
        if (!res.ok) throw new Error('Data load error: ' + res.status);
        var etag = res.headers.get('ETag');
        if (etag) state.etag = etag;
        return res.json();
      })
      .then(function (data) {
        if (data) {
          ingest(data);
        }
      })
      .catch(function () {});
    updateActiveRuns();
  }

  function fatal(message) {
    text(els.count, 'Unavailable');
    clear(els.listEmpty);
    els.listEmpty.hidden = false;
    els.listEmpty.appendChild(node('p', '', message));
  }

  function boot() {
    bind();
    var headers = {};
    if (state.etag) {
      headers['If-None-Match'] = state.etag;
    }
    fetch(BASE + 'data.json', { credentials: 'same-origin', cache: 'no-cache', headers: headers })
      .then(function (res) {
        if (!res.ok) throw new Error('Could not load vault (' + res.status + ')');
        var etag = res.headers.get('ETag');
        if (etag) state.etag = etag;
        return res.json();
      })
      .then(function (data) {
        ingest(data);
        updateActiveRuns();
        setInterval(function () {
          if (!document.hidden) refreshData();
        }, 3000);
        setInterval(function () {
          if (!document.hidden) updateActiveRuns();
        }, 3000);
      })
      .catch(function (err) {
        fatal(err.message || 'Could not load vault.');
      });
  }
  window.setReaderTab = setReaderTab;
  window.openAgentDrawer = openAgentDrawer;
  window.closeAgentDrawer = closeAgentDrawer;
  window.handleAgentStreamEvent = handleAgentStreamEvent;
  window.formatToolCallInline = formatToolCallInline;
  window.renderSwimlaneSvg = renderSwimlaneSvg;
  window.openToolInspector = openToolInspector;
  window.closeToolInspector = closeToolInspector;
  window.cleanThoughtText = cleanThoughtText;
  window.stripAnsi = stripAnsi;
  window.submitActionDialog = triggerModalSubmit;
  window.escapeHtml = escapeHtml;
  window._test = {
    els: els,
    state: state,
    setReaderTab: setReaderTab,
    formatToolCallInline: formatToolCallInline,
    renderSwimlaneSvg: renderSwimlaneSvg,
    openToolInspector: openToolInspector,
    closeToolInspector: closeToolInspector,
    handleAgentStreamEvent: handleAgentStreamEvent,
    appendTrajectoryContextRow: appendTrajectoryContextRow,
    filterTrajectoryRows: filterTrajectoryRows,
    cleanThoughtText: cleanThoughtText,
    stripAnsi: stripAnsi,
    bind: bind,
    submitActionDialog: triggerModalSubmit,
    escapeHtml: escapeHtml,
    openSpecModal: openSpecModal,
    closeSpecModal: closeSpecModal,
    setSpecModalTab: setSpecModalTab,
    createItem: createItem,
    SPEC_SNIPPETS: SPEC_SNIPPETS,
    expandQuickAdd: expandQuickAdd,
    collapseQuickAdd: collapseQuickAdd,
    getClipboardImages: getClipboardImages,
    uploadScreenshot: uploadScreenshot,
    handleScreenshotPaste: handleScreenshotPaste,
    openWorktreeModal: openWorktreeModal,
    closeWorktreeModal: closeWorktreeModal,
    sendCommitPrItem: sendCommitPrItem,
    buildBoardCard: buildBoardCard,
    renderActions: renderActions,
  };
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', boot);
  } else {
    boot();
  }
})();
