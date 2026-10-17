import { state, ORDER, unique, fillSelect, itemStatus, itemType, kind, priority, updateShown } from './state.js';
import { els } from './dom.js';
import { fetchData, fetchUpdate } from './api.js';
import { BASE } from './config.js';
import { text, clear, node, isNarrow } from './formatters.js';
import { route, applyRoute } from './router.js';
import { setViewMode } from './board.js';
import { updateActiveRuns } from './agent.js';
import { bind } from './bind.js';

export function setupFilters() {
  fillSelect(els.status, unique(itemStatus), 'Any status', ORDER.status);
  fillSelect(els.type, unique(itemType), 'Any type', ORDER.type);
  fillSelect(els.kind, unique(kind), 'Any kind', ORDER.kind);
  fillSelect(els.priority, unique(priority), 'Any priority', ORDER.priority);

  var projects = unique(function (item) { return item.project || ''; }).filter(Boolean).sort();
  if (els.projects) {
    clear(els.projects);
    projects.forEach(function (value) {
      var option = node('option');
      option.value = value;
      els.projects.appendChild(option);
    });
  }
  if (els.projectWrap) {
    els.projectWrap.hidden = projects.length < 2;
  }
}

export function fatal(message) {
  text(els.count, 'Unavailable');
  clear(els.listEmpty);
  if (els.listEmpty) {
    els.listEmpty.hidden = false;
    els.listEmpty.appendChild(node('p', '', message));
  }
}

export function ingest(data) {
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
  updateShown();

  if (state.viewMode === 'agents' || (typeof location !== 'undefined' && location && location.hash === '#/agents')) {
    state.viewMode = 'agents';
    state.selected = null;
  } else if (state.viewMode === 'board') {
    if (typeof history !== 'undefined' && history.replaceState && typeof location !== 'undefined' && location.hash && location.hash !== '#/' && location.hash !== '#') {
      history.replaceState(null, '', '#/');
    }
    state.selected = null;
  } else {
    var currentRoute = route();
    if (!currentRoute && !isNarrow() && state.items.length) {
      if (typeof history !== 'undefined' && history.replaceState) {
        history.replaceState(null, '', '#/item/' + encodeURIComponent(state.items[0].id));
      }
    }
  }
  setViewMode(state.viewMode);
  applyRoute();
}

export function load(isInitial, force) {
  var etag = force ? null : state.etag;
  return fetchData(etag)
    .then(function (res) {
      if (res.status === 304) return null;
      if (!res.ok) throw new Error((isInitial ? 'Could not load vault (' : 'Data load error: ') + res.status + (isInitial ? ')' : ''));
      var etagHeader = res.headers && res.headers.get ? res.headers.get('ETag') : null;
      if (etagHeader) state.etag = etagHeader;
      return res.json();
    })
    .then(function (data) {
      if (data) ingest(data);
      updateActiveRuns();
    })
    .catch(function (err) {
      if (isInitial) fatal(err.message || 'Could not load vault.');
    });
}

export function refreshData(force) {
  return load(false, force);
}

export function checkUpdate(force) {
  return fetchUpdate(force)
    .then(function (res) {
      if (!res.ok) return null;
      return res.json();
    })
    .then(function (data) {
      if (!data) return null;
      if (data.update_available) {
        if (els.btnUpdate) {
          els.btnUpdate.hidden = false;
          text(els.updateVersion, 'v' + data.latest_version);
          els.btnUpdate.title = 'Update pin to v' + data.latest_version;
        }
        if (els.updateFooter) {
          text(els.updateFooter, 'Update available: v' + data.latest_version);
        }
      } else {
        if (els.btnUpdate) {
          els.btnUpdate.hidden = true;
        }
        if (els.updateFooter && data.current_version) {
          text(els.updateFooter, 'v' + data.current_version);
        }
      }
      return data;
    })
    .catch(function () {
      return null;
    });
}
export function connectEvents() {
  if (typeof EventSource === 'undefined') return null;
  try {
    var source = new EventSource(BASE + 'events');
    source.onmessage = function (e) {
      if (!e || !e.data) return;
      try {
        var msg = JSON.parse(e.data);
        if (msg.type === 'connected') {
          updateActiveRuns();
        } else if (msg.type === 'vault_changed' || msg.type === 'git_updated') {
          refreshData(true);
        } else if (msg.type === 'run_started' || msg.type === 'run_finished') {
          updateActiveRuns();
          refreshData(true);
        }
      } catch (_) {}
    };
    source.onerror = function () {
      // EventSource auto-reconnects with backoff
    };
    return source;
  } catch (_) {
    return null;
  }
}

export function boot() {
  bind({
    refreshData: refreshData,
    ingest: ingest
  });

  checkUpdate();
  connectEvents();

  load(true).then(function () {
    setInterval(function () {
      if (typeof document !== 'undefined' && !document.hidden) {
        refreshData();
      }
    }, 60000);
    setInterval(function () {
      if (typeof document !== 'undefined' && !document.hidden) {
        checkUpdate();
      }
    }, 30 * 60 * 1000);
  });
}
