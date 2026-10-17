import { els } from './dom.js';
import { state, find } from './state.js';
import { clear, node, text, showToast } from './formatters.js';
import { sendCancel } from './api.js';

var callbacks = {
  openAgentDrawer: function () {},
  updateActiveRuns: function () {},
  setViewMode: function () {},
  setRoute: function () {}
};

export function setAgentsViewCallbacks(cbs) {
  if (cbs) Object.assign(callbacks, cbs);
}

var tickerTimer = null;

export function formatDuration(startedAt) {
  if (!startedAt) return '00:00';
  var startMs = startedAt > 1e11 ? startedAt : (startedAt * 1000);
  var elapsedSec = Math.max(0, Math.floor((Date.now() - startMs) / 1000));
  var m = Math.floor(elapsedSec / 60);
  var s = elapsedSec % 60;
  var mm = String(m).padStart(2, '0');
  var ss = String(s).padStart(2, '0');
  return mm + ':' + ss;
}

export function updateAgentDurations() {
  if (!els.agentsTableBody) return;
  var tickers = els.agentsTableBody.querySelectorAll('.agent-duration-ticker');
  if (!tickers || !tickers.length) return;
  tickers.forEach(function (ticker) {
    var raw = ticker.dataset ? ticker.dataset.startedAt : ticker.getAttribute('data-started-at');
    var ts = Number(raw) || 0;
    text(ticker, formatDuration(ts));
  });
}

export function startAgentsTicker() {
  if (tickerTimer) return;
  updateAgentDurations();
  if (typeof setInterval !== 'undefined') {
    tickerTimer = setInterval(updateAgentDurations, 1000);
  }
}

export function stopAgentsTicker() {
  if (tickerTimer) {
    if (typeof clearInterval !== 'undefined') {
      clearInterval(tickerTimer);
    }
    tickerTimer = null;
  }
}

function getRunActivity(itemId) {
  var traj = (state.currentStreamingId === itemId ? state.trajectory : (state.trajectoryCache && state.trajectoryCache[itemId] && state.trajectoryCache[itemId].trajectory));
  if (traj && traj.toolCalls) {
    var calls = Object.values(traj.toolCalls);
    if (calls.length > 0) {
      var latest = calls[calls.length - 1];
      if (latest && latest.name) {
        return latest.name;
      }
    }
  }
  return 'Running';
}

export function renderAgentsTable() {
  if (!els.agents) return;

  var runs = Array.isArray(state.activeRunsInfo) ? state.activeRunsInfo : [];
  if (runs.length === 0 && state.activeRuns && state.activeRuns.size > 0) {
    // Defensive fallback: synthesize from activeRuns set if activeRunsInfo is empty
    runs = Array.from(state.activeRuns).map(function (id) {
      return {
        item_id: id,
        mode: 'implement',
        is_primary_worktree: true,
        worktree_path: null,
        started_at: (state.activeRunsTimings && state.activeRunsTimings[id]) || 0
      };
    });
  }

  if (runs.length === 0) {
    if (els.agentsEmpty) {
      els.agentsEmpty.hidden = false;
      if (!els.agentsEmpty.textContent || !els.agentsEmpty.textContent.trim()) {
        text(els.agentsEmpty, 'No agents currently running');
      }
    }
    if (els.agentsTableWrap) els.agentsTableWrap.hidden = true;
    if (els.agentsTableBody) clear(els.agentsTableBody);
    stopAgentsTicker();
    return;
  }

  if (els.agentsEmpty) els.agentsEmpty.hidden = true;
  if (els.agentsTableWrap) els.agentsTableWrap.hidden = false;
  if (!els.agentsTableBody) return;

  clear(els.agentsTableBody);

  runs.forEach(function (run) {
    var row = node('tr', 'agent-row');
    row.dataset.itemId = run.item_id;

    var itm = find(run.item_id);

    // 1. Item ID & Title
    var itemCell = node('td', 'agent-col-item');
    var itemBtn = node('button', 'agent-item-link');
    itemBtn.type = 'button';
    var shortId = run.item_id.slice(0, 8);
    var itemTitle = (itm && itm.title) ? itm.title : run.item_id;
    itemBtn.appendChild(node('span', 'agent-item-id', shortId));
    itemBtn.appendChild(document.createTextNode(' '));
    itemBtn.appendChild(node('span', 'agent-item-title', itemTitle));
    itemBtn.addEventListener('click', function () {
      if (callbacks.setRoute) callbacks.setRoute(run.item_id);
      if (callbacks.setViewMode) callbacks.setViewMode('detail');
    });
    itemCell.appendChild(itemBtn);
    row.appendChild(itemCell);

    // 2. Mode
    var modeCell = node('td', 'agent-col-mode');
    var modeLabel = 'Implement';
    var modeClass = 'badge-mode-implement';
    if (run.mode === 'commit_pr') {
      modeLabel = 'Commit & PR';
      modeClass = 'badge-mode-commit-pr';
    } else if (run.mode === 'idea') {
      modeLabel = 'Idea';
      modeClass = 'badge-mode-idea';
    }
    var modeBadge = node('span', 'badge ' + modeClass, modeLabel);
    modeCell.appendChild(modeBadge);
    row.appendChild(modeCell);

    // 3. Worktree Target
    var wtCell = node('td', 'agent-col-worktree');
    var wtText = run.is_primary_worktree ? 'Primary' : (run.worktree_path || 'Worktree');
    var wtBadge = node('span', run.is_primary_worktree ? 'badge-wt-primary' : 'badge-wt-isolated', wtText);
    if (run.worktree_path) wtBadge.title = run.worktree_path;
    wtCell.appendChild(wtBadge);
    row.appendChild(wtCell);

    // 4. Actor
    var actorCell = node('td', 'agent-col-actor');
    var actorName = (itm && itm.claimed_by) ? itm.claimed_by : '@agent:omp';
    actorCell.appendChild(node('span', 'agent-actor', actorName));
    row.appendChild(actorCell);

    // 5. Duration Ticker
    var durCell = node('td', 'agent-col-duration');
    var ticker = node('span', 'agent-duration-ticker', formatDuration(run.started_at));
    ticker.dataset.startedAt = String(run.started_at || 0);
    ticker.setAttribute('data-started-at', String(run.started_at || 0));
    durCell.appendChild(ticker);
    row.appendChild(durCell);

    // 6. Activity
    var actCell = node('td', 'agent-col-activity');
    var activityText = getRunActivity(run.item_id);
    var actSpan = node('span', 'agent-activity', activityText);
    actSpan.title = activityText;
    actCell.appendChild(actSpan);
    row.appendChild(actCell);

    // 7. Actions
    var actBtnsCell = node('td', 'agent-col-actions');

    // View Run
    var viewRunBtn = node('button', 'control text-button btn-view-run', 'View Run');
    viewRunBtn.type = 'button';
    viewRunBtn.addEventListener('click', function () {
      if (callbacks.openAgentDrawer) callbacks.openAgentDrawer(run.item_id);
    });
    actBtnsCell.appendChild(viewRunBtn);

    // Open Chat (Idea mode)
    if (run.mode === 'idea') {
      var openChatBtn = node('button', 'control text-button btn-open-chat', 'Open Chat');
      openChatBtn.type = 'button';
      openChatBtn.addEventListener('click', function () {
        if (callbacks.openAgentDrawer) callbacks.openAgentDrawer(run.item_id);
        if (els.agentChatInput) {
          try { els.agentChatInput.focus(); } catch (_) {}
        }
      });
      actBtnsCell.appendChild(openChatBtn);
    }

    // Cancel
    var cancelBtn = node('button', 'control text-button danger btn-cancel-run', 'Cancel');
    cancelBtn.type = 'button';
    cancelBtn.addEventListener('click', function () {
      cancelBtn.disabled = true;
      sendCancel(run.item_id)
        .then(function (res) {
          if (!res.ok) {
            return res.json().then(function (d) {
              throw new Error((d && d.error) || 'Failed to cancel');
            });
          }
          showToast('Agent cancelled', false);
          if (callbacks.updateActiveRuns) {
            return callbacks.updateActiveRuns();
          }
        })
        .then(function () {
          if (state.viewMode === 'agents') {
            renderAgentsTable();
          }
        })
        .catch(function (err) {
          cancelBtn.disabled = false;
          showToast(err.message || 'Failed to cancel', true);
        });
    });
    actBtnsCell.appendChild(cancelBtn);

    row.appendChild(actBtnsCell);
    els.agentsTableBody.appendChild(row);
  });

  startAgentsTicker();
}
