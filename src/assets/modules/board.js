import { state, find, itemStatus, itemType, priority, hasUnresolvedPrereqs } from './state.js';
import { els } from './dom.js';
import { sendAction } from './api.js';
import { node, text, clear, showToast } from './formatters.js';
import { setRoute } from './router.js';

var callbacks = {
  openCompleteModal: function () {},
  openBlockedModal: function () {},
  openWorktreeModal: function () {},
  openCloseModal: function () {},
  openAgentDrawer: function () {},
  sendCommitPrItem: function () {},
  refreshData: function () {}
};

export function setBoardCallbacks(cbs) {
  if (cbs) Object.assign(callbacks, cbs);
}

export function handleCardDrop(droppedId, targetStatus) {
  var item = find(droppedId);
  if (!item) return;
  var cur = itemStatus(item);
  var rev = item.revision;

  if (targetStatus === 'done') {
    if (cur === 'in_progress') {
      callbacks.openCompleteModal(item);
    } else {
      sendAction(item.id, { action: 'transition', to: 'done', expect_revision: rev })
        .then(function () {
          showToast('Moved to Done', false);
          callbacks.refreshData();
        })
        .catch(function (err) { showToast('Action failed: ' + err.message, true); });
    }
    return;
  }

  if (targetStatus === 'blocked') {
    callbacks.openBlockedModal(item);
    return;
  }

  if (targetStatus === 'in_progress') {
    if (cur === 'in_progress') return;
    callbacks.openWorktreeModal(item, null);
    return;
  }

  if (targetStatus === 'planned') {
    if (cur === 'planned') return;
    sendAction(item.id, { action: 'transition', to: 'planned', expect_revision: rev })
      .then(function () {
        showToast('Moved to Planned', false);
        callbacks.refreshData();
      })
      .catch(function (err) { showToast('Action failed: ' + err.message, true); });
    return;
  }

  if (targetStatus === 'created') {
    if (cur !== 'created') {
      sendAction(item.id, { action: 'transition', to: 'created', expect_revision: rev })
        .then(function () {
          showToast('Moved to Backlog', false);
          callbacks.refreshData();
        })
        .catch(function (err) { showToast('Action failed: ' + err.message, true); });
    }
    return;
  }

  if (targetStatus === 'closed') {
    callbacks.openCloseModal(item);
    return;
  }
}

export function buildBoardCard(item) {
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
      callbacks.openAgentDrawer(item.id);
    });
    meta.appendChild(viewRunBtn);
  } else {
    var cardStatus = itemStatus(item);
    if (cardStatus === 'review' || cardStatus === 'done') {
      var isMerged = item.git_status === 'merged';
      var isPrOpen = item.git_status === 'pr_open' || (item.pr_url && item.pr_url.length > 0);
      if (isPrOpen) {
        var prLabel = item.pr_number ? 'PR #' + item.pr_number + ' ↗' : 'PR ↗';
        var prLink = node('a', 'btn-pr-link', prLabel);
        prLink.href = item.pr_url || '#';
        prLink.setAttribute('href', item.pr_url || '#');
        prLink.target = '_blank';
        prLink.rel = 'noopener noreferrer';
        prLink.title = 'Open Pull Request: ' + (item.pr_url || '');
        prLink.addEventListener('click', function (e) {
          e.stopPropagation();
        });
        meta.appendChild(prLink);
      } else if (isMerged) {
        var mergedBadge = node('span', 'indicator-merged', 'Merged into main');
        mergedBadge.title = 'Merged into main branch';
        meta.appendChild(mergedBadge);
      } else {
        var commitPrBtn = node('button', 'btn-commit-pr', 'Commit + PR');
        commitPrBtn.type = 'button';
        commitPrBtn.title = 'Start agent to commit worktree and create PR';
        commitPrBtn.addEventListener('click', function (e) {
          e.stopPropagation();
          callbacks.sendCommitPrItem(item.id);
        });
        meta.appendChild(commitPrBtn);
      }
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
    if (e.dataTransfer) {
      e.dataTransfer.setData('text/plain', item.id);
      e.dataTransfer.effectAllowed = 'move';
    }
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

export function setViewMode(mode) {
  state.viewMode = mode === 'detail' ? 'detail' : 'board';
  try {
    localStorage.setItem('pin_view_mode', state.viewMode);
  } catch (_) {}
  if (els.workspace) {
    els.workspace.classList.toggle('mode-board', state.viewMode === 'board');
    els.workspace.classList.toggle('mode-detail', state.viewMode === 'detail');
  }
  if (els.viewBoardBtn) els.viewBoardBtn.setAttribute('aria-pressed', String(state.viewMode === 'board'));
  if (els.viewDetailBtn) els.viewDetailBtn.setAttribute('aria-pressed', String(state.viewMode === 'detail'));
  if (state.viewMode === 'detail' && !state.selected && state.shown.length) {
    setRoute(state.shown[0].id);
  }
  if (state.viewMode === 'board') {
    renderBoard();
  }
}

export function renderBoard() {
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
