(function () {
  'use strict';

  var rawBase = document.documentElement.dataset.base || '';
  var BASE = rawBase ? (rawBase.endsWith('/') ? rawBase : rawBase + '/') : '';

  var PURIFY_CONFIG = {
    ALLOWED_TAGS: ['p','br','strong','em','b','i','code','pre','blockquote','h1','h2','h3','h4','h5','h6','ul','ol','li','a','hr','table','thead','tbody','tr','th','td','del','ins','sub','sup'],
    ALLOWED_ATTR: ['href','title','class'],
    ALLOWED_URI_REGEXP: /^(?:(?:https?|mailto):|[^a-z]|[a-z+.\-]+(?:[^a-z+.\-:]|$))/i,
    FORBID_TAGS: ['script','iframe','object','embed','form','input','button','textarea','select','style','link','meta','base','img','svg','math'],
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
    quickAddForm: $('quick-add-form'), quickAddInput: $('quick-add-input'),
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
    toast: $('toast')
  };

  var state = {
    viewMode: (function () {
      try { return localStorage.getItem('pin_view_mode') || 'board'; } catch (_) { return 'board'; }
    })(),
    items: [],
    shown: [],
    selected: null,
    scope: '',
    archive: '',
    captured: '',
    etag: null,
    filters: { text: '', status: '', type: '', kind: '', priority: '', project: '' }
  };

  var activeModalConfig = null;
  var toastTimer = null;

  function norm(value) { return String(value == null ? '' : value).toLowerCase(); }
  function text(node, value) { if (node) node.textContent = value == null ? '' : String(value); }
  function clear(node) { if (node) while (node.firstChild) node.removeChild(node.firstChild); }
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
  function isNarrow() { return window.matchMedia('(max-width: 799.98px)').matches; }

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

  function formatCountdown(expiresAt) {
    if (!expiresAt) return null;
    var target = Number(expiresAt);
    if (target < 1e12) target *= 1000;
    var now = Date.now();
    var diff = target - now;
    if (diff <= 0) return { text: 'Lease expired', expired: true };
    var totalSec = Math.floor(diff / 1000);
    var hours = Math.floor(totalSec / 3600);
    var mins = Math.floor((totalSec % 3600) / 60);
    var secs = totalSec % 60;
    var parts = [];
    if (hours > 0) parts.push(hours + 'h');
    parts.push(mins + 'm');
    parts.push((secs < 10 && hours > 0 ? '0' : '') + secs + 's');
    return { text: parts.join(' '), expired: false };
  }

  function route() {
    var match = (location.hash || '').match(/^#\/(?:idea|item)\/(.+)$/);
    if (!match) return null;
    try { return decodeURIComponent(match[1]); } catch (_) { return null; }
  }

  function setRoute(id) {
    var next = id ? '#/item/' + encodeURIComponent(id) : '#/';
    if (location.hash !== next) location.hash = next; else applyRoute();
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
          throw new Error(e.error || ('HTTP ' + res.status));
        }).catch(function (parseErr) {
          throw new Error(parseErr.message || ('HTTP ' + res.status));
        });
      }
      return res.json();
    })
    .then(function (data) {
      refreshData();
      return data;
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
          showDialogError(err.message || 'Action failed');
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

  function openClaimModal(item) {
    var rev = item.revision;
    var selectedLease = 3600;
    var savedActor = '';
    try { savedActor = localStorage.getItem('pin_actor') || ''; } catch (_) {}
    if (!savedActor) savedActor = 'human:viewer';

    openActionModal({
      title: 'Claim: ' + (item.title || item.id),
      submitText: 'Claim & Start',
      render: function (body) {
        var actorLabel = node('label');
        actorLabel.appendChild(node('span', '', 'Actor identifier'));
        var actorInput = node('input');
        actorInput.id = 'action-input-actor';
        actorInput.type = 'text';
        actorInput.value = savedActor;
        actorInput.placeholder = 'human:name or agent:model';
        actorLabel.appendChild(actorInput);
        body.appendChild(actorLabel);

        var leaseLabel = node('label');
        leaseLabel.appendChild(node('span', '', 'Lease duration'));
        var pills = node('div', 'preset-pills');
        var presets = [
          { label: '15m', sec: 900 },
          { label: '1h', sec: 3600 },
          { label: '4h', sec: 14400 }
        ];
        presets.forEach(function (p) {
          var btn = node('button', 'preset-pill' + (p.sec === selectedLease ? ' active' : ''), p.label);
          btn.type = 'button';
          btn.addEventListener('click', function () {
            selectedLease = p.sec;
            var siblings = pills.querySelectorAll('.preset-pill');
            siblings.forEach(function (s) { s.classList.remove('active'); });
            btn.classList.add('active');
          });
          pills.appendChild(btn);
        });
        leaseLabel.appendChild(pills);
        body.appendChild(leaseLabel);
      },
      onSubmit: function () {
        var actorEl = $('action-input-actor');
        var actorVal = actorEl ? actorEl.value.trim() : '';
        if (!actorVal) actorVal = 'human:viewer';
        try { localStorage.setItem('pin_actor', actorVal); } catch (_) {}
        return sendAction(item.id, { action: 'claim', actor: actorVal, lease: selectedLease, expect_revision: rev });
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
      openClaimModal(item);
      return;
    }

    if (targetStatus === 'planned') {
      if (cur === 'in_progress') {
        sendAction(item.id, { action: 'release', force: true, expect_revision: rev })
          .catch(function (err) { showToast('Action failed: ' + err.message, true); });
      } else {
        sendAction(item.id, { action: 'transition', to: 'planned', expect_revision: rev })
          .catch(function (err) { showToast('Action failed: ' + err.message, true); });
      }
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

    if (item.claimed_by) {
      meta.appendChild(node('span', 'claimer-badge', '@' + item.claimed_by));
    }

    if (item.claim_expires_at) {
      var countdown = node('span', 'lease-countdown');
      countdown.dataset.expiresAt = String(item.claim_expires_at);
      var cd = formatCountdown(item.claim_expires_at);
      if (cd) {
        text(countdown, cd.text);
        if (cd.expired) countdown.classList.add('expired');
      }
      meta.appendChild(countdown);
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

      if (item.claimed_by) {
        meta.appendChild(node('span', 'claimer-badge', '@' + item.claimed_by));
      }

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

  function renderMarkdown(item) {
    var source = bodyWithoutDuplicateTitle(item.content || item.body, item.title), dirty;
    try { dirty = marked.parse(source, { async: false }); } catch (_) { text(els.body, source); return; }
    try { els.body.innerHTML = DOMPurify.sanitize(dirty, PURIFY_CONFIG); } catch (_) { text(els.body, source); }
  }

  function renderWorkerBox(item) {
    clear(els.workerCard);
    els.workerCard.appendChild(node('h3', '', 'Active Worker & State'));

    var header = node('div', 'worker-card-header');
    if (item.claimed_by) {
      header.appendChild(node('span', 'worker-actor', '@' + item.claimed_by));
    } else {
      header.appendChild(node('span', 'worker-actor', 'Unclaimed'));
    }
    if (item.revision != null) {
      header.appendChild(node('span', 'worker-revision', 'rev ' + item.revision));
    }
    els.workerCard.appendChild(header);

    var meta = node('div', 'worker-meta');
    if (item.claim_expires_at) {
      var timer = node('span', 'lease-countdown');
      timer.dataset.expiresAt = String(item.claim_expires_at);
      var cd = formatCountdown(item.claim_expires_at);
      if (cd) {
        text(timer, cd.text);
        if (cd.expired) timer.classList.add('expired');
      }
      meta.appendChild(timer);
    }
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
      btn('Claim (Start Work)', true, function () {
        openClaimModal(item);
      });
      btn('Mark Blocked', false, function () {
        openBlockedModal(item);
      });
    }

    if (st === 'in_progress') {
      btn('Complete with Evidence', true, function () {
        openCompleteModal(item);
      });
      btn('Release Claim', false, function () {
        sendAction(item.id, { action: 'release', force: true, expect_revision: rev })
          .then(function () { showToast('Claim released', false); })
          .catch(function (err) { showToast('Action failed: ' + err.message, true); });
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

    if (st === 'done') {
      btn('Close Item', true, function () {
        openCloseModal(item);
      });
      btn('Reopen to Planned', false, function () {
        sendAction(item.id, { action: 'transition', to: 'planned', expect_revision: rev })
          .then(function () { showToast('Reopened to Planned', false); })
          .catch(function (err) { showToast('Action failed: ' + err.message, true); });
      });
    }

    if (st !== 'closed' && st !== 'cancelled' && st !== 'done') {
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

  function renderDetail() {
    var item = find(state.selected);
    document.body.classList.toggle('detail-open', !!item);
    els.proposal.hidden = !item;
    els.detailEmpty.hidden = !!item;

    if (!item) {
      text(els.detailEmpty, state.selected ? 'This item is not in the vault.' : 'Select an item to view it.');
      return;
    }

    var context = [];
    if (item.project) context.push(item.project);
    context.push(itemType(item).toUpperCase());
    context.push(kind(item));
    text(els.context, context.join(' · '));
    text(els.title, item.title || '(untitled)');

    clear(els.summary);
    addSummary(itemStatus(item), 'status-badge status-' + itemStatus(item));
    if (item.claimed_by) addSummary('@' + item.claimed_by, 'claimer-badge');
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
    addMeta('Claimed by', item.claimed_by);
    if (item.claim_expires_at) addMeta('Claim lease expires', date(item.claim_expires_at, true));
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
    renderList();
    renderDetail();
    if (state.selected && isNarrow()) {
      try { els.reader.focus({ preventScroll: true }); } catch (_) { els.reader.focus(); }
    }
  }

  function updateCountdowns() {
    var timers = document.querySelectorAll('.lease-countdown');
    timers.forEach(function (el) {
      var exp = el.dataset.expiresAt;
      if (!exp) return;
      var info = formatCountdown(exp);
      if (info) {
        text(el, info.text);
        el.classList.toggle('expired', info.expired);
      }
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

    if (els.dialogSubmitBtn) {
      els.dialogSubmitBtn.addEventListener('click', triggerModalSubmit);
    }
    if (els.dialogCloseBtn) {
      els.dialogCloseBtn.addEventListener('click', closeActionModal);
    }
    if (els.dialogCancelBtn) {
      els.dialogCancelBtn.addEventListener('click', closeActionModal);
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
    if (els.quickAddForm && els.quickAddInput) {
      els.quickAddForm.addEventListener('submit', function (e) {
        e.preventDefault();
        var title = els.quickAddInput.value.trim();
        if (!title) return;
        var activeProject = (state.scope && state.scope !== 'all') ? state.scope : undefined;
        els.quickAddInput.disabled = true;
        fetch(BASE + 'items', {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
            'X-Pin-Action': 'true'
          },
          body: JSON.stringify({
            title: title,
            type: 'task',
            status: 'created',
            project: activeProject
          })
        })
        .then(function (res) {
          if (!res.ok) {
            return res.json().then(function (err) { throw new Error(err.error || 'Failed to create task'); })
              .catch(function (pErr) { throw new Error(pErr.message || 'Failed to create task'); });
          }
          return res.json();
        })
        .then(function (newItem) {
          els.quickAddInput.value = '';
          els.quickAddInput.disabled = false;
          showToast('Created task: ' + (newItem.title || title), false);
          refreshData();
        })
        .catch(function (err) {
          els.quickAddInput.disabled = false;
          showToast(err.message || 'Failed to create task', true);
        });
      });
    }

    document.addEventListener('keydown', function (event) {
      if (els.actionDialog && els.actionDialog.open) {
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

      var input = /^(INPUT|SELECT|TEXTAREA)$/.test(event.target.tagName);
      if (event.key === '/' && !input) {
        event.preventDefault();
        els.search.focus();
        return;
      }
      if (event.key === 'Escape') {
        if (input) event.target.blur();
        else if (state.selected) setRoute(null);
        return;
      }
      if (input || event.metaKey || event.ctrlKey || event.altKey) return;
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

    setInterval(updateCountdowns, 1000);
  }

  function ingest(data) {
    state.items = Array.isArray(data.items) ? data.items : [];
    state.scope = String(data.scope || 'all');
    state.archive = String(data.archive_filter || '');
    state.captured = String(data.captured_at || '');

    text(els.scope, state.scope === 'all' ? 'All projects' : state.scope);
    text(els.snapshot, 'Vault: ' + state.scope + (state.archive ? ' · ' + state.archive : ''));
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
    fetch(BASE + 'data.json', { credentials: 'same-origin', cache: 'no-store', headers: headers })
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
    fetch(BASE + 'data.json', { credentials: 'same-origin', cache: 'no-store', headers: headers })
      .then(function (res) {
        if (!res.ok) throw new Error('Could not load vault (' + res.status + ')');
        var etag = res.headers.get('ETag');
        if (etag) state.etag = etag;
        return res.json();
      })
      .then(function (data) {
        ingest(data);
        setInterval(function () {
          if (!document.hidden) refreshData();
        }, 3000);
      })
      .catch(function (err) {
        fatal(err.message || 'Could not load vault.');
      });
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', boot);
  } else {
    boot();
  }
})();
