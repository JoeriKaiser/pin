import { BASE } from './config.js';
import { state, find, itemStatus } from './state.js';
import { els, $ } from './dom.js';
import { fetchRuns, fetchItemLog } from './api.js';
import { text, clear, node, stripAnsi, cleanThoughtText } from './formatters.js';
import { setViewMode } from './board.js';
import { setRoute } from './router.js';
var callbacks = { refreshData: function () {}, renderBoard: function () {}, renderDetail: function () {}, renderAgentsTable: function () {} };
export function setAgentCallbacks(cbs) { if (cbs) Object.assign(callbacks, cbs); }

function setStatusPill(status, cls) {
  if (els.agentStatusPill) {
    text(els.agentStatusPill, status);
    els.agentStatusPill.className = 'agent-status-pill ' + (cls || '');
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
      var changed = newSet.size !== state.activeRuns.size;
      if (!changed) {
        newSet.forEach(function (id) { if (!state.activeRuns.has(id)) changed = true; });
      }
      if (changed) {
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
  scrollTrajectoryStreamToBottom();
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

function scrollTrajectoryStreamToBottom() {
  if (els.trajectoryStream) els.trajectoryStream.scrollTop = els.trajectoryStream.scrollHeight;
}

export function updateDurationDisplay() {
  if (!state.trajectory || !els.metricDuration) return;
  var start = state.trajectory.startTime;
  if (!start) { text(els.metricDuration, 'Duration: 0.0s'); return; }
  var end = state.trajectory.finishedAt ? (state.trajectory.finishedAt * 1000) : Date.now();
  text(els.metricDuration, 'Duration: ' + Math.max(0, (end - start) / 1000).toFixed(1) + 's');
}

export function closeAgentDrawer() {
  if (state.trajectory && state.trajectory.timer) { clearInterval(state.trajectory.timer); state.trajectory.timer = null; }
  if (state.currentStream) { state.currentStream.close(); state.currentStream = null; }
  if (state.currentStreamingId) cacheTrajectory(state.currentStreamingId);
  state.currentStreamingId = null; state.readerTab = 'spec';
  if (els.agentDrawer) els.agentDrawer.hidden = true;
  if (els.proposal) els.proposal.hidden = false;
  if (els.tabTaskSpec) els.tabTaskSpec.classList.add('active');
  if (els.tabAgentTrajectory) els.tabAgentTrajectory.classList.remove('active');
}

function cacheTrajectory(itemId) {
  if (!itemId || !state.trajectory) return;
  state.trajectoryCache = state.trajectoryCache || {};
  state.trajectoryCache[itemId] = {
    trajectory: state.trajectory, streamHtml: els.trajectoryStream ? els.trajectoryStream.innerHTML : '',
    turnsText: els.metricTurns ? els.metricTurns.textContent : '', callsText: els.metricCalls ? els.metricCalls.textContent : '',
    statusText: els.agentStatusPill ? els.agentStatusPill.textContent : '', statusClass: els.agentStatusPill ? els.agentStatusPill.className : ''
  };
}

var scheduleTimer = null;
function scheduleCacheTrajectory(itemId) {
  clearTimeout(scheduleTimer);
  scheduleTimer = setTimeout(function () { cacheTrajectory(itemId); }, 200);
}

function createTrajectory(timingStart, now) {
  return {
    startTime: timingStart ? (timingStart * 1000) : now, startedAt: timingStart, finishedAt: null, turns: 0, calls: 0, timer: null,
    steps: [{ lane: 'input', label: 'Start' }], activeAssistantRow: null, activeAssistantTextEl: null,
    intervals: { input: [{ start: now, end: now + 50 }], model: [], tools: [] }, currentModelInterval: null,
    toolIntervals: {}, toolCalls: {}, toolRows: {}, filterQuery: ''
  };
}

export function openAgentDrawer(itemId) {
  state.trajectoryCache = state.trajectoryCache || {};
  if (state.currentStreamingId && state.currentStreamingId !== itemId) cacheTrajectory(state.currentStreamingId);
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
  if (els.tabAgentTrajectory) els.tabAgentTrajectory.classList.add('active'); if (els.tabTaskSpec) els.tabTaskSpec.classList.remove('active');
  if (els.proposal) els.proposal.hidden = true; if (els.agentDrawer) els.agentDrawer.hidden = false;

  var itm = find(itemId);
  if (els.agentTaskTitle) text(els.agentTaskTitle, (itm && itm.title) ? itm.title : '');
  if (els.agentItemId) { text(els.agentItemId, itemId.slice(0, 8)); els.agentItemId.dataset.fullId = itemId; }
  if (els.agentLogInfo) text(els.agentLogInfo, 'Logs: .pin_vault/runs/' + itemId + '.log');
  var isRunning = state.activeRuns && state.activeRuns.has(itemId);
  var itemSt = itm ? itemStatus(itm) : '';
  var pillCls = isRunning ? '' : (itemSt === 'in_progress' ? 'status-interrupted' : ((itemSt === 'done' || itemSt === 'closed') ? 'status-completed' : ''));
  var pillText = isRunning ? 'Running' : (itemSt === 'in_progress' ? 'Interrupted' : ((itemSt === 'done' || itemSt === 'closed') ? 'Completed' : 'Idle'));
  setStatusPill(pillText, pillCls);
  if (els.btnCancelAgent) els.btnCancelAgent.disabled = !isRunning;

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
    if (els.trajectoryStream) els.trajectoryStream.innerHTML = cached.streamHtml || '';
    renderSwimlaneSvg();
    return;
  }

  clear(els.trajectoryStream);
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
    if (els.agentChatContainer) { els.agentChatContainer.hidden = false; if (els.agentChatInput) els.agentChatInput.value = ''; }
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
      scheduleCacheTrajectory(itemId);
    } catch (err) {
      console.error('Error parsing SSE event:', err);
    }
  };

  source.onerror = function () {
    if (state.activeRuns.has(itemId)) return;
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
  if (els.readerTrajectoryBadge) els.readerTrajectoryBadge.hidden = true;
  var msg = success ? 'Agent run completed.' : ('Agent run failed: ' + (data.message || data.error || defaultMsg));
  appendTrajectoryContextRow(msg);
  setStatusPill(success ? 'Completed' : ('Failed: ' + (data.message || data.error || defaultMsg)), success ? 'status-completed' : 'status-failed');
  if (els.btnCancelAgent) els.btnCancelAgent.disabled = true;
  if (state.currentStream) { state.currentStream.close(); state.currentStream = null; }
  if (els.agentChatContainer) els.agentChatContainer.hidden = true;
  if (success) renderSwimlaneSvg();
  callbacks.renderBoard();
  callbacks.renderDetail();
  updateActiveRuns();
  callbacks.refreshData();
  cacheTrajectory(state.currentStreamingId);
}

export function handleAgentStreamEvent(data) {
  if (!data) return;
  if (data.type === 'waiting_for_user') {
    if (els.agentChatContainer) {
      els.agentChatContainer.hidden = false;
      if (els.agentChatInput) { els.agentChatInput.disabled = false; els.agentChatInput.focus(); }
      if (els.btnAgentChatSend) els.btnAgentChatSend.disabled = false;
    }
    setStatusPill('Waiting for reply', 'status-running');
    appendTrajectoryContextRow('Agent is waiting for your reply.');
    scrollTrajectoryStreamToBottom();
    return;
  }
  if (data.type === 'user_message') {
    var content = data.text || (data.update && data.update.content) || '';
    if (content) appendTrajectoryUserRow(content);
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
  } else if (updateType === 'agent_thought_chunk' || updateType === 'agent_message_chunk') {
    var rawThought = extractThoughtContent(update);
    appendAgentThought(rawThought);
    var cleaned = cleanThoughtText(rawThought);
    if (!state.trajectory) return;

    if (!state.trajectory.activeAssistantRow) {
      state.trajectory.turns++;
      if (els.metricTurns) text(els.metricTurns, 'Turns: ' + state.trajectory.turns);
      var row = node('div', 'trajectory-row trajectory-row-assistant');
      row.appendChild(node('div', 'trajectory-gutter', '●'));
      row.appendChild(node('span', 'trajectory-badge badge-assistant', 'ASSISTANT'));
      var body = node('div', 'trajectory-body');
      var thoughtEl = node('div', 'trajectory-thought-text');
      body.appendChild(thoughtEl);
      row.appendChild(body);
      if (els.trajectoryStream) els.trajectoryStream.appendChild(row);
      state.trajectory.activeAssistantRow = row;
      state.trajectory.activeAssistantTextEl = thoughtEl;

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
    state.trajectory.activeAssistantRow = null;
    state.trajectory.activeAssistantTextEl = null;
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
    function inspect(e) { if (e && e.stopPropagation) e.stopPropagation(); openToolInspector(callId); }
    tLine.addEventListener('click', inspect);
    tRow.addEventListener('click', inspect);
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

export function openToolInspector(callId) {
  if (!els.toolInspector || !state.trajectory) return;
  var rec = state.trajectory.toolCalls[callId];
  if (!rec) return;
  els.toolInspector.dataset.activeCallId = callId;
  els.toolInspector.hidden = false;
  if (els.inspectorTitle) text(els.inspectorTitle, rec.name || 'Tool Call');
  if (els.inspectorStatus) {
    text(els.inspectorStatus, rec.status || 'running');
    els.inspectorStatus.className = 'inspector-status-badge ' + (rec.status === 'completed' ? 'status-completed' : (rec.status === 'failed' ? 'status-failed' : 'status-running'));
  }
  if (els.inspectorArgs) text(els.inspectorArgs, typeof rec.rawInput === 'object' && rec.rawInput ? JSON.stringify(rec.rawInput, null, 2) : String(rec.rawInput || ''));
  if (els.inspectorOutput) text(els.inspectorOutput, typeof rec.rawOutput === 'object' && rec.rawOutput ? JSON.stringify(rec.rawOutput, null, 2) : stripAnsi(String(rec.rawOutput || '')));
}

export function closeToolInspector() {
  if (els.toolInspector) { els.toolInspector.hidden = true; delete els.toolInspector.dataset.activeCallId; }
}

export function copyToolOutput() {
  if (!els.inspectorOutput) return;
  if (navigator.clipboard && navigator.clipboard.writeText) navigator.clipboard.writeText(els.inspectorOutput.textContent || '');
}

export function renderSwimlaneSvg() {
  if (!els.swimlaneSvg || !state.trajectory) return;
  clear(els.swimlaneSvg);
  var lanes = { input: 20, model: 50, tools: 80 };
  ['Input', 'Model', 'Tools'].forEach(function (l) {
    var txt = node('text', 'swimlane-lane-label', l);
    txt.setAttribute('x', '10');
    txt.setAttribute('y', String(lanes[l.toLowerCase()] + 4));
    els.swimlaneSvg.appendChild(txt);
  });
  (state.trajectory.steps || []).forEach(function (s, idx) {
    var c = node('circle', 'swimlane-node swimlane-node-' + s.lane);
    c.setAttribute('cx', String(60 + idx * 30));
    c.setAttribute('cy', String(lanes[s.lane] || 50));
    c.setAttribute('r', '6');
    els.swimlaneSvg.appendChild(c);
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
