import { BASE } from './config.js';
import { state, find, itemStatus, getRunInfo, isRunInteractive } from './state.js';
import { els, $ } from './dom.js';
import { fetchRuns, fetchItemLog } from './api.js';
import { text, clear, node, stripAnsi, cleanThoughtText } from './formatters.js';
import { renderMarkdownSnippet } from './markdown.js';
import { setViewMode } from './board.js';
import { setRoute } from './router.js';
import { syncWaitingAlerts } from './alerts.js';
import { renderQuickHelpersInto } from './reply-inbox.js';
var callbacks = { refreshData: function () {}, renderBoard: function () {}, renderDetail: function () {}, renderAgentsTable: function () {} };
export function setAgentCallbacks(cbs) { if (cbs) Object.assign(callbacks, cbs); }

export function announceAgentStatus(message) {
  var el = els.agentStatusAnnouncer || (typeof document !== 'undefined' && document.getElementById('agent-status-announcer'));
  if (el && message) {
    el.textContent = message;
  }
}

function setStatusPill(status, cls) {
  if (els.agentStatusPill) {
    text(els.agentStatusPill, status);
    els.agentStatusPill.className = 'agent-status-pill ' + (cls || '');
  }
}

export function syncAgentChatControls(itemId, interactive, waiting, mode, restoreDraft, lastMessage) {
  if (!els.agentChatContainer) return;
  if (!interactive) {
    els.agentChatContainer.hidden = true;
    if (els.agentChatHelpers) els.agentChatHelpers.hidden = true;
    return;
  }
  els.agentChatContainer.hidden = false;
  if (els.agentChatInput) {
    if (restoreDraft) {
      els.agentChatInput.value = (state.chatDrafts && itemId && state.chatDrafts[itemId]) || '';
    }
    els.agentChatInput.disabled = !waiting;
    els.agentChatInput.placeholder = waiting
      ? 'Reply to agent… (Press Enter to send, Shift+Enter for newline)'
      : 'Agent is working… reply box unlocks when it pauses for input';
  }
  if (els.btnAgentChatSend) {
    els.btnAgentChatSend.disabled = !waiting;
  }
  if (els.btnAgentFinishIdea) {
    els.btnAgentFinishIdea.disabled = false;
    text(els.btnAgentFinishIdea, mode === 'implement' ? 'Finish & Complete' : 'Finish discussion');
  }
  if (els.agentChatHelpers) {
    if (waiting) {
      var runInfo = getRunInfo(itemId);
      var msgForHelpers = lastMessage || (runInfo && runInfo.last_message) || '';
      renderQuickHelpersInto(els.agentChatHelpers, els.agentChatInput, msgForHelpers, function () {
        if (itemId && els.agentChatInput) {
          state.chatDrafts = state.chatDrafts || {};
          state.chatDrafts[itemId] = els.agentChatInput.value;
        }
      });
      els.agentChatHelpers.hidden = false;
    } else {
      els.agentChatHelpers.hidden = true;
    }
  }
}

export function updateActiveRuns() {
  return fetchRuns()
    .then(function (res) { return res.ok ? res.json() : null; })
    .then(function (data) {
      if (!data || !Array.isArray(data.running)) return;
      state.activeRunsTimings = data.timings || {};
      if (Array.isArray(data.runs)) {
        state.activeRunsInfo = data.runs;
      } else {
        state.activeRunsInfo = data.running.map(function (id) {
          return {
            item_id: id,
            mode: 'implement',
            is_primary_worktree: true,
            worktree_path: null,
            started_at: (data.timings && data.timings[id]) || 0
          };
        });
      }

      var activeCount = data.running.length;
      if (els.agentsCountBadge) {
        if (activeCount > 0) {
          els.agentsCountBadge.hidden = false;
          text(els.agentsCountBadge, String(activeCount));
        } else {
          els.agentsCountBadge.hidden = true;
          text(els.agentsCountBadge, '0');
        }
      }

      var newSet = new Set(data.running);
      var newWaitingSet = new Set(
        Array.isArray(data.waiting)
          ? data.waiting
          : state.activeRunsInfo.filter(function (r) { return r && r.waiting_for_user; }).map(function (r) { return r.item_id; })
      );
      var changed = newSet.size !== state.activeRuns.size;
      if (!changed) {
        newSet.forEach(function (id) { if (!state.activeRuns.has(id)) changed = true; });
      }
      var waitingChanged = !state.waitingRuns || newWaitingSet.size !== state.waitingRuns.size;
      if (!waitingChanged && state.waitingRuns) {
        newWaitingSet.forEach(function (id) { if (!state.waitingRuns.has(id)) waitingChanged = true; });
      }
      syncWaitingAlerts({ notify: true });
      if (changed || waitingChanged) {
        state.activeRuns = newSet;
        callbacks.renderBoard();
        callbacks.renderDetail();
        if (state.selected && els.readerTrajectoryBadge) {
          els.readerTrajectoryBadge.hidden = !state.activeRuns.has(state.selected);
        }
      }
      if (state.viewMode === 'agents' && callbacks.renderAgentsTable) {
        callbacks.renderAgentsTable();
      }
    })
    .catch(function () {});
}

export function appendTrajectoryUserRow(content) {
  if (!els.trajectoryStream) return;
  var row = node('div', 'trajectory-row trajectory-row-user');
  row.appendChild(node('div', 'trajectory-gutter', '●'));
  row.appendChild(node('span', 'trajectory-badge badge-user', 'USER'));
  var body = node('div', 'trajectory-body');
  body.appendChild(node('div', 'trajectory-user-text', content));
  row.appendChild(body);
  els.trajectoryStream.appendChild(row);
  scrollTrajectoryStreamToBottom(true);
}

export function appendTrajectoryContextRow(textOrNode) {
  if (!els.trajectoryStream) return;
  var row = node('div', 'trajectory-row trajectory-row-context');
  row.appendChild(node('div', 'trajectory-gutter', '●'));
  row.appendChild(node('span', 'trajectory-badge badge-context', 'CONTEXT'));
  var body = node('div', 'trajectory-body');
  body.appendChild(typeof textOrNode === 'string' ? node('span', '', textOrNode) : textOrNode);
  row.appendChild(body);
  els.trajectoryStream.appendChild(row);
  scrollTrajectoryStreamToBottom();
}

export function scrollTrajectoryStreamToBottom(force) {
  if (!els.trajectoryStream) return;
  var stream = els.trajectoryStream;
  var clientHeight = stream.clientHeight || 0;
  var scrollHeight = stream.scrollHeight || 0;
  var scrollTop = stream.scrollTop || 0;
  var isNearBottom = (scrollHeight - scrollTop - clientHeight) <= 40;
  if (force || isNearBottom) {
    stream.scrollTop = scrollHeight;
  }
}

export function updateDurationDisplay() {
  if (!state.trajectory || !els.metricDuration) return;
  var start = state.trajectory.startTime;
  if (!start) { text(els.metricDuration, 'Duration: 0.0s'); return; }
  var end = state.trajectory.finishedAt ? (state.trajectory.finishedAt * 1000) : Date.now();
  text(els.metricDuration, 'Duration: ' + Math.max(0, (end - start) / 1000).toFixed(1) + 's');
}

export function closeAgentDrawer() {
  if (state.currentStreamingId && els.agentChatInput && typeof els.agentChatInput.value === 'string') {
    state.chatDrafts = state.chatDrafts || {};
    state.chatDrafts[state.currentStreamingId] = els.agentChatInput.value;
  }
  if (state.trajectory && state.trajectory.timer) { clearInterval(state.trajectory.timer); state.trajectory.timer = null; }
  if (state.currentStream) { state.currentStream.close(); state.currentStream = null; }
  if (state.currentStreamingId) cacheTrajectory(state.currentStreamingId);
  state.currentStreamingId = null; state.readerTab = 'spec';
  if (els.agentDrawer) els.agentDrawer.hidden = true;
  if (els.proposal) els.proposal.hidden = false;
  if (els.tabTaskSpec) {
    els.tabTaskSpec.classList.add('active');
    if (els.tabTaskSpec.setAttribute) {
      els.tabTaskSpec.setAttribute('aria-selected', 'true');
      els.tabTaskSpec.setAttribute('tabindex', '0');
    }
  }
  if (els.tabAgentTrajectory) {
    els.tabAgentTrajectory.classList.remove('active');
    if (els.tabAgentTrajectory.setAttribute) {
      els.tabAgentTrajectory.setAttribute('aria-selected', 'false');
      els.tabAgentTrajectory.setAttribute('tabindex', '-1');
    }
  }
  syncWaitingAlerts({ notify: false });
}

function cacheTrajectory(itemId) {
  flushPendingMarkdown();
  if (!itemId || !state.trajectory) return;
  state.trajectoryCache = state.trajectoryCache || {};
  state.trajectoryCache[itemId] = {
    trajectory: state.trajectory, streamHtml: els.trajectoryStream ? els.trajectoryStream.innerHTML : '',
    turnsText: els.metricTurns ? els.metricTurns.textContent : '', callsText: els.metricCalls ? els.metricCalls.textContent : '',
    statusText: els.agentStatusPill ? els.agentStatusPill.textContent : '', statusClass: els.agentStatusPill ? els.agentStatusPill.className : ''
  };
}

function createTrajectory(timingStart, now) {
  return {
    startTime: timingStart ? (timingStart * 1000) : now, startedAt: timingStart, finishedAt: null, turns: 0, calls: 0, timer: null,
    steps: [{ lane: 'input', label: 'Start' }], activeAssistantRow: null, activeAssistantTextEl: null,
    activeAssistantKind: null, activeAssistantRaw: '',
    intervals: { input: [{ start: now, end: now + 50 }], model: [], tools: [] }, currentModelInterval: null,
    toolIntervals: {}, toolCalls: {}, toolRows: {}, filterQuery: ''
  };
}

var pendingMdRaf = null;
var pendingMdTarget = null;
var pendingMdRaw = '';

function flushPendingMarkdown() {
  if (pendingMdRaf !== null && typeof cancelAnimationFrame === 'function') {
    cancelAnimationFrame(pendingMdRaf);
  }
  pendingMdRaf = null;
  if (pendingMdTarget) {
    var target = pendingMdTarget;
    var raw = pendingMdRaw;
    pendingMdTarget = null;
    pendingMdRaw = '';
    renderMarkdownSnippet(raw, target);
    scrollTrajectoryStreamToBottom();
  }
}

function scheduleMarkdownRender(raw, targetEl) {
  if (typeof requestAnimationFrame !== 'function' || raw.length < 256) {
    renderMarkdownSnippet(raw, targetEl);
    return;
  }
  if (pendingMdTarget && pendingMdTarget !== targetEl) {
    flushPendingMarkdown();
  }
  pendingMdTarget = targetEl;
  pendingMdRaw = raw;
  if (pendingMdRaf === null) {
    pendingMdRaf = requestAnimationFrame(function () {
      pendingMdRaf = null;
      flushPendingMarkdown();
    });
  }
}

function resetActiveAssistantRow() {
  flushPendingMarkdown();
  if (!state.trajectory) return;
  state.trajectory.activeAssistantRow = null;
  state.trajectory.activeAssistantTextEl = null;
  state.trajectory.activeAssistantKind = null;
  state.trajectory.activeAssistantRaw = '';
}

export function openAgentDrawer(itemId) {
  state.trajectoryCache = state.trajectoryCache || {};
  state.chatDrafts = state.chatDrafts || {};
  if (state.currentStreamingId && state.currentStreamingId !== itemId) {
    if (els.agentChatInput && typeof els.agentChatInput.value === 'string') {
      state.chatDrafts[state.currentStreamingId] = els.agentChatInput.value;
    }
    cacheTrajectory(state.currentStreamingId);
  }
  if (state.currentStream) { state.currentStream.close(); state.currentStream = null; }
  state.currentStreamingId = itemId;

  if (state.trajectory && state.trajectory.timer) { clearInterval(state.trajectory.timer); state.trajectory.timer = null; }
  if (state.selected !== itemId) {
    setRoute(itemId);
  }
  if (state.viewMode !== 'detail') {
    setViewMode('detail');
  }
  state.readerTab = 'trajectory';
  if (els.tabAgentTrajectory) {
    els.tabAgentTrajectory.classList.add('active');
    if (els.tabAgentTrajectory.setAttribute) {
      els.tabAgentTrajectory.setAttribute('aria-selected', 'true');
      els.tabAgentTrajectory.setAttribute('tabindex', '0');
    }
  }
  if (els.tabTaskSpec) {
    els.tabTaskSpec.classList.remove('active');
    if (els.tabTaskSpec.setAttribute) {
      els.tabTaskSpec.setAttribute('aria-selected', 'false');
      els.tabTaskSpec.setAttribute('tabindex', '-1');
    }
  }
  if (els.proposal) els.proposal.hidden = true; if (els.agentDrawer) els.agentDrawer.hidden = false;
  syncWaitingAlerts({ notify: false });

  var itm = find(itemId);
  if (els.agentTaskTitle) text(els.agentTaskTitle, (itm && itm.title) ? itm.title : '');
  if (els.agentItemId) { text(els.agentItemId, itemId.slice(0, 8)); els.agentItemId.dataset.fullId = itemId; }
  if (els.agentLogInfo) text(els.agentLogInfo, 'Logs: .pin_vault/runs/' + itemId + '.log');
  var isRunning = state.activeRuns && state.activeRuns.has(itemId);
  var runInfo = getRunInfo(itemId);
  var runInteractive = isRunInteractive(runInfo);
  var runWaiting = !!(runInfo && runInfo.waiting_for_user);
  var itemSt = itm ? itemStatus(itm) : '';
  var pillCls = isRunning
    ? (runWaiting ? 'status-waiting' : 'status-running')
    : (itemSt === 'in_progress' ? 'status-interrupted' : ((itemSt === 'done' || itemSt === 'closed') ? 'status-completed' : ''));
  var pillText = isRunning
    ? (runWaiting ? 'Waiting for reply' : 'Running')
    : (itemSt === 'in_progress' ? 'Interrupted' : ((itemSt === 'done' || itemSt === 'closed') ? 'Completed' : 'Idle'));
  setStatusPill(pillText, pillCls);
  if (els.btnCancelAgent) els.btnCancelAgent.disabled = !isRunning;
  if (isRunning) {
    announceAgentStatus('Viewing running agent for ' + ((itm && itm.title) ? itm.title : itemId));
  }

  var planSec = $('agent-plan-section');
  if (planSec) planSec.hidden = true;
  clear(els.agentPlanList);
  clear(els.agentThoughts);
  clear(els.agentTools);
  if (els.btnToggleTools) { els.btnToggleTools.hidden = true; text(els.btnToggleTools, 'Expand all'); }
  closeToolInspector();

  var cached = (!isRunning && state.trajectoryCache[itemId]) ? state.trajectoryCache[itemId] : null;
  var timingStart = (state.activeRunsTimings && state.activeRunsTimings[itemId]) || null;

  if (cached) {
    state.trajectory = Object.assign({}, cached.trajectory);
    updateDurationDisplay();
    if (els.metricTurns) text(els.metricTurns, cached.turnsText || 'Turns: ' + state.trajectory.turns);
    if (els.metricCalls) text(els.metricCalls, cached.callsText || 'Calls: ' + state.trajectory.calls);
    if (els.agentStatusPill) { text(els.agentStatusPill, cached.statusText || 'Completed'); els.agentStatusPill.className = cached.statusClass || 'agent-status-pill status-completed'; }
    if (els.btnCancelAgent) els.btnCancelAgent.disabled = true;
    if (els.agentChatContainer) els.agentChatContainer.hidden = true;
    if (els.trajectorySearch) els.trajectorySearch.value = state.trajectory.filterQuery || '';
    if (els.trajectoryStream) {
      els.trajectoryStream.innerHTML = cached.streamHtml || '';
      scrollTrajectoryStreamToBottom(true);
    }
    renderSwimlaneSvg();
    return;
  }

  clear(els.trajectoryStream);
  if (els.trajectorySearch) els.trajectorySearch.value = '';
  var now = Date.now();
  state.trajectory = createTrajectory(timingStart, now);
  appendTrajectoryContextRow('Agent run started for ' + itemId);
  renderSwimlaneSvg();
  updateDurationDisplay();
  if (els.metricTurns) text(els.metricTurns, 'Turns: 0');
  if (els.metricCalls) text(els.metricCalls, 'Calls: 0');

  if (isRunning) {
    updateDurationDisplay();
    state.trajectory.timer = setInterval(updateDurationDisplay, 1000);
    syncAgentChatControls(itemId, runInteractive, runWaiting, runInfo && runInfo.mode, true);
  } else if (els.agentChatContainer) {
    els.agentChatContainer.hidden = true;
  }

  var receivedEvents = 0;
  var source = new EventSource(BASE + 'items/' + encodeURIComponent(itemId) + '/stream');
  state.currentStream = source;

  source.onmessage = function (e) {
    try {
      var data = JSON.parse(e.data);
      if (receivedEvents === 0 && cached) {
        clear(els.trajectoryStream);
        state.trajectory = createTrajectory(timingStart, Date.now());
        appendTrajectoryContextRow('Agent run started for ' + itemId);
      }
      receivedEvents++;
      handleAgentStreamEvent(data);
    } catch (err) {
      console.error('Error parsing SSE event:', err);
    }
  };

  source.onerror = function () {
    if (state.activeRuns.has(itemId)) return;
    if (state.currentStreamingId !== itemId) {
      source.close();
      return;
    }
    updateDurationDisplay();
    if (els.agentStatusPill && els.agentStatusPill.textContent === 'Running') {
      setStatusPill('Completed', 'status-completed');
      appendTrajectoryContextRow('Agent run completed.');
      renderSwimlaneSvg();
    }
    if (els.btnCancelAgent) els.btnCancelAgent.disabled = true;
    source.close();
    if (state.currentStream === source) state.currentStream = null;
    if (receivedEvents === 0) {
      fetchItemLog(itemId).then(function (res) {
        if (state.currentStreamingId !== itemId) return;
        if (res.status === 200) {
          var link = node('span', '', 'Log available: '), a = node('a', '', '.pin_vault/runs/' + itemId + '.log');
          a.href = BASE + 'items/' + encodeURIComponent(itemId) + '/log'; a.target = '_blank'; a.rel = 'noopener';
          link.appendChild(a); appendTrajectoryContextRow(link); cacheTrajectory(itemId);
        }
      }).catch(function () {});
    } else {
      cacheTrajectory(itemId);
    }
  };
}

export function scanForPrUrl(text) {
  if (!text || typeof text !== 'string') return null;
  var markers = ['/pull/', '/merge_requests/'];
  var lastMatch = null;
  for (var i = 0; i < markers.length; i++) {
    var marker = markers[i];
    var searchStart = 0;
    while (true) {
      var idx = text.indexOf(marker, searchStart);
      if (idx === -1) break;
      searchStart = idx + marker.length;
      var endIdx = searchStart;
      while (endIdx < text.length && text.charCodeAt(endIdx) >= 48 && text.charCodeAt(endIdx) <= 57) {
        endIdx++;
      }
      if (endIdx === searchStart) continue;
      var numStr = text.slice(searchStart, endIdx);
      var startIdx = idx;
      var stopChars = ' \t\r\n"\'`()[]<>';
      while (startIdx > 0 && stopChars.indexOf(text.charAt(startIdx - 1)) === -1) {
        startIdx--;
      }
      var token = text.slice(startIdx, endIdx);
      if (token.indexOf('http://') === 0 || token.indexOf('https://') === 0) {
        lastMatch = { url: token, number: parseInt(numStr, 10) };
      }
    }
  }
  return lastMatch;
}

function finishStream(success, data, defaultMsg) {
  if (state.trajectory) {
    if (data.startedAt != null) state.trajectory.startedAt = data.startedAt;
    if (data.finishedAt != null) state.trajectory.finishedAt = data.finishedAt;
    clearInterval(state.trajectory.timer);
    state.trajectory.timer = null;
    updateDurationDisplay();
  }
  if (state.activeRuns && state.currentStreamingId) {
    state.activeRuns.delete(state.currentStreamingId);
  }
  var prUrl = (data && data.pr_url) || (state.trajectory && state.trajectory.prUrl);
  var prNumber = (data && data.pr_number) || (state.trajectory && state.trajectory.prNumber);
  if (state.currentStreamingId) {
    var itm = find(state.currentStreamingId);
    if (itm && prUrl) {
      itm.status = 'done';
      itm.pr_url = prUrl;
      itm.pr_number = prNumber;
      itm.git_status = 'pr_open';
    }
  }
  if (els.readerTrajectoryBadge) els.readerTrajectoryBadge.hidden = true;
  var msg = success ? 'Agent run completed.' : ('Agent run failed: ' + (data.message || data.error || defaultMsg));
  appendTrajectoryContextRow(msg);
  announceAgentStatus(msg);
  setStatusPill(success ? 'Completed' : ('Failed: ' + (data.message || data.error || defaultMsg)), success ? 'status-completed' : 'status-failed');
  if (els.btnCancelAgent) els.btnCancelAgent.disabled = true;
  if (state.currentStream) { state.currentStream.close(); state.currentStream = null; }
  if (els.agentChatContainer) els.agentChatContainer.hidden = true;
  if (success) renderSwimlaneSvg();
  callbacks.renderBoard();
  callbacks.renderDetail();
  updateActiveRuns();
  callbacks.refreshData(true);
  cacheTrajectory(state.currentStreamingId);
}

export function handleAgentStreamEvent(data) {
  if (!data) return;
  if (data.pr_url && state.trajectory) {
    state.trajectory.prUrl = data.pr_url;
    if (data.pr_number != null) state.trajectory.prNumber = data.pr_number;
  }
  if (data.type === 'run_state') {
    var rsInteractive = !!(data.interactive || data.mode === 'idea');
    var rsWaiting = !!data.waiting_for_user;
    syncAgentChatControls(state.currentStreamingId, rsInteractive, rsWaiting, data.mode, false, data.last_message);
    if (rsWaiting) {
      setStatusPill('Waiting for reply', 'status-waiting');
    } else if (state.activeRuns && state.currentStreamingId && state.activeRuns.has(state.currentStreamingId)) {
      setStatusPill('Running', 'status-running');
    }
    return;
  }
  if (data.type === 'waiting_for_user') {
    resetActiveAssistantRow();
    var activeInfo = getRunInfo(state.currentStreamingId);
    syncAgentChatControls(state.currentStreamingId, true, true, activeInfo && activeInfo.mode, false, data.message);
    if (els.agentChatInput && typeof els.agentChatInput.focus === 'function') {
      els.agentChatInput.focus();
    }
    setStatusPill('Waiting for reply', 'status-waiting');
    appendTrajectoryContextRow('Agent is waiting for your reply.');
    announceAgentStatus('Agent is waiting for your reply.');
    scrollTrajectoryStreamToBottom();
    return;
  }
  if (data.type === 'user_message') {
    resetActiveAssistantRow();
    var content = data.text || (data.update && data.update.content) || '';
    if (content) appendTrajectoryUserRow(content);
    var msgRunInfo = getRunInfo(state.currentStreamingId);
    syncAgentChatControls(state.currentStreamingId, true, false, msgRunInfo && msgRunInfo.mode, false);
    setStatusPill('Running', 'status-running');
    return;
  }
  if (data.type === 'run_timing') {
    if (state.trajectory) {
      if (data.startedAt != null) { state.trajectory.startedAt = data.startedAt; state.trajectory.startTime = data.startedAt * 1000; }
      if (data.finishedAt != null) state.trajectory.finishedAt = data.finishedAt;
      updateDurationDisplay();
    }
    return;
  }
  if (data.type === 'finished') { finishStream(true, data, ''); return; }
  if (data.type === 'error') { finishStream(false, data, 'Error'); return; }

  var update = (data.params && data.params.update) ? data.params.update : (data.update || data);
  var updateType = (update && typeof update === 'object' && (update.sessionUpdate || update.type)) || (data.params && (data.params.sessionUpdate || data.params.type)) || data.sessionUpdate || data.type;

  if (updateType === 'plan') {
    renderAgentPlan(update);
  } else if (updateType === 'agent_thought_chunk' || updateType === 'agent_message_chunk' || updateType === 'agent_message') {
    var isMessage = updateType === 'agent_message_chunk' || updateType === 'agent_message';
    var chunkKind = isMessage ? 'message' : 'thought';
    var rawThought = extractThoughtContent(update);
    appendAgentThought(rawThought);
    var cleaned = cleanThoughtText(rawThought);
    if (state.trajectory && !state.trajectory.prUrl && rawThought) {
      var thoughtPr = scanForPrUrl(rawThought);
      if (thoughtPr) {
        state.trajectory.prUrl = thoughtPr.url;
        state.trajectory.prNumber = thoughtPr.number;
      }
    }
    if (!state.trajectory) return;

    if (!state.trajectory.activeAssistantRow || state.trajectory.activeAssistantKind !== chunkKind) {
      state.trajectory.turns++;
      if (els.metricTurns) text(els.metricTurns, 'Turns: ' + state.trajectory.turns);
      var row = node('div', 'trajectory-row trajectory-row-assistant ' + (isMessage ? 'trajectory-row-message' : 'trajectory-row-thought'));
      row.appendChild(node('div', 'trajectory-gutter', '●'));
      row.appendChild(node('span', 'trajectory-badge badge-assistant' + (isMessage ? '' : ' badge-thought'), isMessage ? 'ASSISTANT' : 'THOUGHT'));
      var body = node('div', 'trajectory-body');
      var textEl = node('div', isMessage ? 'trajectory-message-text prose' : 'trajectory-thought-text');
      body.appendChild(textEl);
      row.appendChild(body);
      if (els.trajectoryStream) els.trajectoryStream.appendChild(row);
      state.trajectory.activeAssistantRow = row;
      state.trajectory.activeAssistantTextEl = textEl;
      state.trajectory.activeAssistantKind = chunkKind;
      state.trajectory.activeAssistantRaw = '';

      var mNow = Date.now();
      state.trajectory.intervals.model.push({ start: mNow, end: mNow });
      state.trajectory.currentModelInterval = state.trajectory.intervals.model[state.trajectory.intervals.model.length - 1];

      var lastStep = state.trajectory.steps && state.trajectory.steps[state.trajectory.steps.length - 1];
      if (!lastStep || lastStep.lane !== 'model') {
        state.trajectory.steps.push({ lane: 'model', label: 'Turn ' + state.trajectory.turns });
        renderSwimlaneSvg();
      }
    }

    if (cleaned && state.trajectory.activeAssistantTextEl) {
      if (isMessage) {
        state.trajectory.activeAssistantRaw = (state.trajectory.activeAssistantRaw || '') + cleaned;
        scheduleMarkdownRender(state.trajectory.activeAssistantRaw, state.trajectory.activeAssistantTextEl);
      } else {
        var aNodes = state.trajectory.activeAssistantTextEl.childNodes || state.trajectory.activeAssistantTextEl.children;
        var aLast = aNodes && aNodes.length > 0 ? aNodes[aNodes.length - 1] : null;
        if (aLast && (aLast.nodeType === 3 || typeof aLast.textContent === 'string') && aLast.parentNode === state.trajectory.activeAssistantTextEl) {
          aLast.textContent = (aLast.textContent || '') + cleaned;
          if (aLast.nodeValue !== undefined) aLast.nodeValue = aLast.textContent;
        } else {
          state.trajectory.activeAssistantTextEl.appendChild(document.createTextNode(cleaned));
        }
      }
    }
    scrollTrajectoryStreamToBottom();
    if (state.trajectory.currentModelInterval) state.trajectory.currentModelInterval.end = Date.now();
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
    resetActiveAssistantRow();
    state.trajectory.calls++;
    if (els.metricCalls) text(els.metricCalls, 'Calls: ' + state.trajectory.calls);

    var callId = update.toolCallId || update.id || ('call_' + Date.now() + '_' + Math.random().toString(36).slice(2, 7));
    var tNow = Date.now();
    var toolInterval = { id: callId, start: tNow, end: tNow };
    state.trajectory.intervals.tools.push(toolInterval);
    state.trajectory.toolIntervals[callId] = toolInterval;
    state.trajectory.toolCalls[callId] = {
      id: callId, name: formatToolTitle(update), title: update.title || '',
      rawInput: update.rawInput || update.input || update.arguments || update.params || update.args || null,
      rawOutput: update.rawOutput || update.output || update.result || update.content || null,
      status: update.status || 'running', startTime: tNow, endTime: null
    };

    var inline = formatToolCallInline(update);
    var tRow = node('div', 'trajectory-row trajectory-row-tool');
    tRow.dataset.callId = callId;
    if (state.trajectory.toolRows) state.trajectory.toolRows[callId] = tRow;
    var tLine = node('div', 'trajectory-tool-line');
    tLine.dataset.callId = callId;
    tLine.appendChild(node('span', 'tool-name', inline.name));
    tLine.appendChild(node('span', 'tool-args', inline.argsStr));
    tLine.appendChild(node('span', 'tool-arrow', '→'));
    tLine.appendChild(node('span', 'tool-result-preview', inline.previewStr));
    var stCls = inline.status === 'completed' ? 'status-completed' : (inline.status === 'failed' ? 'status-failed' : 'status-running');
    tLine.appendChild(node('span', 'tool-status-badge ' + stCls, inline.status));
    tRow.classList.add('clickable-tool-row');
    tRow.tabIndex = 0;
    tRow.setAttribute('tabindex', '0');
    tRow.setAttribute('role', 'button');
    tRow.setAttribute('aria-label', 'Inspect tool call ' + inline.name);
    function inspect(e) { if (e && e.stopPropagation) e.stopPropagation(); openToolInspector(callId); }
    tLine.addEventListener('click', inspect);
    tRow.addEventListener('click', inspect);
    tRow.addEventListener('keydown', function (e) {
      if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        inspect(e);
      }
    });
    var tBody = node('div', 'trajectory-body');
    tBody.appendChild(tLine);
    tRow.appendChild(node('div', 'trajectory-gutter', '●'));
    tRow.appendChild(node('span', 'trajectory-badge badge-tool', 'TOOL'));
    tRow.appendChild(tBody);
    if (els.trajectoryStream) {
      els.trajectoryStream.appendChild(tRow);
      scrollTrajectoryStreamToBottom();
    }
    if (state.trajectory.filterQuery) {
      tRow.hidden = (tRow.textContent || '').toLowerCase().indexOf(state.trajectory.filterQuery) === -1;
    }
    renderAgentToolCall(update);
    state.trajectory.steps.push({ lane: 'tools', label: inline.name, callId: callId });
    renderSwimlaneSvg();
  } else if (updateType === 'tool_call_update') {
    if (!state.trajectory) return;
    var callId = update.toolCallId || update.id;
    if (!callId) return;

    var record = state.trajectory.toolCalls[callId] || (state.trajectory.toolCalls[callId] = {
      id: callId, name: formatToolTitle(update), status: update.status || 'completed', startTime: Date.now(), endTime: null
    });
    var out = update.rawOutput || update.output || update.result || update.error || update.content;
    if (out) record.rawOutput = out;
    if (state.trajectory && !state.trajectory.prUrl && out) {
      var outPr = scanForPrUrl(typeof out === 'string' ? out : JSON.stringify(out));
      if (outPr) {
        state.trajectory.prUrl = outPr.url;
        state.trajectory.prNumber = outPr.number;
      }
    }
    var inp = update.rawInput || update.input || update.arguments || update.params || update.args;
    if (inp) record.rawInput = inp;
    if (update.status) record.status = update.status;
    record.endTime = Date.now();
    var toolIv = state.trajectory.toolIntervals[callId];
    if (toolIv) toolIv.end = Date.now();
    if (els.trajectoryStream) {
      var rowEl = (state.trajectory.toolRows && state.trajectory.toolRows[callId]) || els.trajectoryStream.querySelector('[data-call-id="' + callId + '"]');
      if (rowEl) {
        var inf = formatToolCallInline(record);
        var prev = rowEl.querySelector('.tool-result-preview');
        if (prev) text(prev, inf.previewStr);
        var sBadge = rowEl.querySelector('.tool-status-badge');
        if (sBadge) {
          var sTxt = record.status || 'completed';
          if (sTxt === 'in_progress') sTxt = 'running';
          text(sBadge, sTxt);
          sBadge.className = 'tool-status-badge ' + (sTxt === 'completed' ? 'status-completed' : (sTxt === 'failed' ? 'status-failed' : 'status-running'));
        }
        var aEl = rowEl.querySelector('.tool-args');
        if (aEl && !aEl.textContent) text(aEl, inf.argsStr);
      }
    }
    if (els.toolInspector && !els.toolInspector.hidden && els.toolInspector.dataset.activeCallId === callId) {
      openToolInspector(callId);
    }
    updateAgentToolCall(update);
  }
}

export function formatToolTitle(update) {
  var name = update.name || update.tool || 'tool';
  if (name === 'bash' || name === 'sh') {
    var raw = update.rawInput || update.input || update.arguments || update.params || update.args;
    var cmd = typeof raw === 'object' && raw ? (raw.command || raw.cmd || '') : '';
    if (cmd) return 'bash: ' + cmd.trim().split(/\s+/).slice(0, 3).join(' ');
  }
  return update.title || name;
}

export function formatToolCallInline(update) {
  var raw = update.rawInput || update.input || update.arguments || update.params || update.args;
  var rawOut = update.rawOutput || update.output || update.result || update.content;
  var st = (update.status === 'in_progress') ? 'running' : (update.status || 'running');
  var name = formatToolTitle(update);
  var argsStr = raw ? (typeof raw === 'string' ? raw.trim() : (raw.command || raw.path || raw.pattern || JSON.stringify(raw)).trim()) : '';
  if (argsStr.length > 80) argsStr = argsStr.slice(0, 80) + '…';
  var prevStr = rawOut ? stripAnsi(typeof rawOut === 'string' ? rawOut : JSON.stringify(rawOut)).trim().replace(/\s+/g, ' ') : '';
  if (prevStr.length > 60) prevStr = prevStr.slice(0, 60) + '…';
  return { name: name, argsStr: argsStr, previewStr: prevStr, status: st };
}

var lastInspectorTrigger = null;

function syncInspectorNavButtons(callId) {
  if (!state.trajectory || !state.trajectory.toolCalls) return;
  var ids = Object.keys(state.trajectory.toolCalls);
  var idx = ids.indexOf(callId);
  if (els.btnInspectorPrev) els.btnInspectorPrev.disabled = idx <= 0;
  if (els.btnInspectorNext) els.btnInspectorNext.disabled = idx < 0 || idx >= ids.length - 1;
}

export function stepToolInspector(delta) {
  if (!els.toolInspector || els.toolInspector.hidden || !state.trajectory || !state.trajectory.toolCalls) return;
  var ids = Object.keys(state.trajectory.toolCalls);
  var cur = els.toolInspector.dataset.activeCallId;
  var idx = ids.indexOf(cur);
  if (idx < 0) return;
  var next = idx + delta;
  if (next >= 0 && next < ids.length) {
    openToolInspector(ids[next], true);
  }
}

export function openToolInspector(callId, preserveTrigger) {
  if (!els.toolInspector || !state.trajectory) return;
  var rec = state.trajectory.toolCalls[callId];
  if (!rec) return;
  if (!preserveTrigger && typeof document !== 'undefined' && document.activeElement) {
    lastInspectorTrigger = document.activeElement;
  }
  els.toolInspector.dataset.activeCallId = callId;
  els.toolInspector.hidden = false;
  if (els.inspectorTitle) text(els.inspectorTitle, rec.name || 'Tool Call');
  if (els.inspectorStatus) {
    text(els.inspectorStatus, rec.status || 'running');
    els.inspectorStatus.className = 'inspector-status-badge ' + (rec.status === 'completed' ? 'status-completed' : (rec.status === 'failed' ? 'status-failed' : 'status-running'));
  }
  if (els.inspectorArgs) text(els.inspectorArgs, typeof rec.rawInput === 'object' && rec.rawInput ? JSON.stringify(rec.rawInput, null, 2) : String(rec.rawInput || ''));
  if (els.inspectorOutput) text(els.inspectorOutput, typeof rec.rawOutput === 'object' && rec.rawOutput ? JSON.stringify(rec.rawOutput, null, 2) : stripAnsi(String(rec.rawOutput || '')));
  syncInspectorNavButtons(callId);
  if (!preserveTrigger && els.btnCloseInspector && typeof els.btnCloseInspector.focus === 'function') {
    try { els.btnCloseInspector.focus(); } catch (_) {}
  }
}

export function closeToolInspector() {
  if (els.toolInspector) { els.toolInspector.hidden = true; delete els.toolInspector.dataset.activeCallId; }
  if (lastInspectorTrigger && lastInspectorTrigger.isConnected && typeof lastInspectorTrigger.focus === 'function') {
    try { lastInspectorTrigger.focus(); } catch (_) {}
  }
  lastInspectorTrigger = null;
}

var copyFeedbackTimer = null;
export function copyToolOutput() {
  if (!els.inspectorOutput) return;
  if (!navigator.clipboard || !navigator.clipboard.writeText) return;
  var value = els.inspectorOutput.textContent || '';
  Promise.resolve(navigator.clipboard.writeText(value)).then(function () {
    if (!els.btnCopyToolOutput) return;
    text(els.btnCopyToolOutput, 'Copied');
    if (copyFeedbackTimer) clearTimeout(copyFeedbackTimer);
    copyFeedbackTimer = setTimeout(function () {
      if (els.btnCopyToolOutput) text(els.btnCopyToolOutput, 'Copy');
    }, 1500);
  }).catch(function () {
    if (els.btnCopyToolOutput) text(els.btnCopyToolOutput, 'Copy failed');
  });
}

function svgNode(tag, className, value) {
  var n = (typeof document !== 'undefined' && typeof document.createElementNS === 'function')
    ? document.createElementNS('http://www.w3.org/2000/svg', tag)
    : document.createElement(tag);
  if (className) n.setAttribute('class', className);
  if (value != null) text(n, value);
  return n;
}

export function renderSwimlaneSvg() {
  if (!els.swimlaneSvg || !state.trajectory) return;
  clear(els.swimlaneSvg);
  var lanes = { input: 14, model: 32, tools: 50 };
  var laneOrder = [
    { key: 'input', label: 'Input' },
    { key: 'model', label: 'Model' },
    { key: 'tools', label: 'Tools' }
  ];
  laneOrder.forEach(function (lane) {
    var y = lanes[lane.key];
    var line = svgNode('line', 'swimlane-lane-line');
    line.setAttribute('x1', '48');
    line.setAttribute('x2', '790');
    line.setAttribute('y1', String(y));
    line.setAttribute('y2', String(y));
    els.swimlaneSvg.appendChild(line);

    var txt = svgNode('text', 'swimlane-lane-label', lane.label);
    txt.setAttribute('x', '8');
    txt.setAttribute('y', String(y + 3));
    els.swimlaneSvg.appendChild(txt);
  });

  var steps = state.trajectory.steps || [];
  var maxSpan = 720;
  var gap = steps.length > 1 ? Math.min(32, maxSpan / (steps.length - 1)) : 32;
  var prevX = null;
  var prevY = null;

  steps.forEach(function (s, idx) {
    var cx = Math.round(62 + idx * gap);
    var cy = lanes[s.lane] || lanes.model;
    if (prevX != null && prevY != null) {
      var seg = svgNode('line', 'swimlane-connector');
      seg.setAttribute('x1', String(prevX));
      seg.setAttribute('y1', String(prevY));
      seg.setAttribute('x2', String(cx));
      seg.setAttribute('y2', String(cy));
      els.swimlaneSvg.appendChild(seg);
    }
    var c = svgNode('circle', 'swimlane-node swimlane-node-' + s.lane + (s.callId ? ' swimlane-node-clickable' : ''));
    c.setAttribute('cx', String(cx));
    c.setAttribute('cy', String(cy));
    c.setAttribute('r', '5');
    if (s.label) {
      c.appendChild(svgNode('title', '', s.label));
    }
    if (s.callId) {
      c.addEventListener('click', function () {
        openToolInspector(s.callId);
      });
    }
    els.swimlaneSvg.appendChild(c);
    prevX = cx;
    prevY = cy;
  });
}

export function renderAgentPlan(update) {
  if (!els.agentPlanList) return;
  var sec = $('agent-plan-section'), entries = Array.isArray(update.entries) ? update.entries : (Array.isArray(update.plan) ? update.plan : []);
  if (!entries.length) return;
  if (sec) sec.hidden = false;
  clear(els.agentPlanList);
  entries.forEach(function (ent) {
    var li = node('li', 'agent-plan-item ' + (ent.status || 'pending'));
    li.appendChild(node('span', 'plan-bullet', ent.status === 'completed' ? '✓' : (ent.status === 'in_progress' ? '▶' : '○')));
    li.appendChild(node('span', 'plan-content', ent.content || ent.title || ''));
    els.agentPlanList.appendChild(li);
  });
}

export function extractThoughtContent(u) {
  if (!u) return '';
  if (typeof u === 'string') return u;
  if (u.text || u.thought) return u.text || u.thought;
  if (Array.isArray(u.content)) return u.content.map(function (c) { return typeof c === 'string' ? c : (c ? (c.text || c.thought || (c.content && c.content.text) || '') : ''); }).join('');
  if (typeof u.content === 'object' && u.content) return u.content.text || u.content.thought || '';
  return typeof u.content === 'string' ? u.content : '';
}

export function appendAgentThought(thoughtText) {
  if (!els.agentThoughts || !thoughtText) return;
  var cleaned = cleanThoughtText(thoughtText);
  if (!cleaned) return;
  var sec = $('agent-thoughts-section');
  if (sec) sec.hidden = false;
  els.agentThoughts.appendChild(node('p', 'agent-thought-chunk', cleaned));
}

function formatToolPayload(data) {
  if (data == null) return '';
  if (typeof data === 'object') { try { return JSON.stringify(data, null, 2); } catch (_) { return String(data); } }
  return stripAnsi(String(data)).trim();
}

export function renderAgentToolCall(update) {
  if (!els.agentTools) return;
  var callId = update.toolCallId || update.id || ('tool-' + Date.now());
  if ($('agent-tool-' + callId)) return;
  var card = node('details', 'agent-tool-card');
  card.id = 'agent-tool-' + callId;
  card.open = false;
  var summary = node('summary', 'agent-tool-header'), titleWrap = node('div', 'agent-tool-title-wrap');
  titleWrap.appendChild(node('span', 'agent-tool-chevron', '▸'));
  titleWrap.appendChild(node('span', 'agent-tool-name', formatToolTitle(update)));
  summary.appendChild(titleWrap);
  summary.appendChild(node('span', 'agent-tool-status ' + (update.status || 'in_progress'), update.status || 'in_progress'));
  card.appendChild(summary);
  var contentDiv = node('div', 'agent-tool-content');
  var inText = formatToolPayload(update.rawInput || update.input || update.arguments || update.params);
  if (inText) contentDiv.appendChild(node('div', 'agent-tool-input', inText));
  contentDiv.appendChild(node('div', 'agent-tool-body'));
  card.appendChild(contentDiv);
  card.addEventListener('toggle', updateToggleToolsButtonState);
  els.agentTools.appendChild(card);
  updateToggleToolsButtonState();
}

export function updateAgentToolCall(update) {
  if (!els.agentTools) return;
  var callId = update.toolCallId || update.id;
  if (!callId) return;
  var card = $('agent-tool-' + callId);
  if (!card) { renderAgentToolCall(update); card = $('agent-tool-' + callId); }
  if (!card) return;
  var nameEl = card.querySelector('.agent-tool-name');
  if (nameEl) { var t = formatToolTitle(update); if (t !== 'tool') text(nameEl, t); }
  var badge = card.querySelector('.agent-tool-status');
  if (badge && update.status) {
    text(badge, update.status);
    badge.className = 'agent-tool-status ' + (update.status === 'completed' ? 'completed' : (update.status === 'failed' ? 'failed' : 'in_progress'));
    if (update.status === 'completed' || update.status === 'failed') card.open = false;
  }
  var body = card.querySelector('.agent-tool-body');
  if (body) {
    var outText = formatToolPayload(update.rawOutput || update.output || update.result || update.error || update.content);
    if (outText) text(body, outText);
  }
  updateToggleToolsButtonState();
}

var toggleToolsPending = false;
export function updateToggleToolsButtonState() {
  if (!els.btnToggleTools || !els.agentTools || toggleToolsPending) return;
  toggleToolsPending = true;
  var raf = (typeof window !== 'undefined' && window && window.requestAnimationFrame) ? window.requestAnimationFrame : (typeof requestAnimationFrame !== 'undefined' ? requestAnimationFrame : function (cb) { setTimeout(cb, 0); });
  raf(function () { toggleToolsPending = false; syncToggleToolsButtonState(); });
}

export function syncToggleToolsButtonState() {
  if (!els.btnToggleTools || !els.agentTools) return;
  var cards = els.agentTools.querySelectorAll('.agent-tool-card');
  els.btnToggleTools.hidden = !cards.length;
  if (!cards.length) return;
  text(els.btnToggleTools, Array.from(cards).every(function (c) { return c.open; }) ? 'Collapse all' : 'Expand all');
}

export function toggleAllToolCards() {
  if (!els.agentTools) return;
  var cards = els.agentTools.querySelectorAll('.agent-tool-card');
  if (!cards.length) return;
  var nextOpen = !Array.from(cards).some(function (c) { return c.open; });
  cards.forEach(function (c) { c.open = nextOpen; });
  updateToggleToolsButtonState();
}

export function filterTrajectoryRows(query) {
  var q = (query == null ? (els.trajectorySearch ? els.trajectorySearch.value : '') : query).toLowerCase().trim();
  if (state.trajectory) state.trajectory.filterQuery = q;
  if (!els.trajectoryStream) return;
  els.trajectoryStream.querySelectorAll('.trajectory-row').forEach(function (r) {
    r.hidden = q ? (r.textContent || '').toLowerCase().indexOf(q) === -1 : false;
  });
}
