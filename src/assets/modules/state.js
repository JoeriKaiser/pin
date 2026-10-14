export var ORDER = {
  status: ['created', 'planned', 'in_progress', 'blocked', 'review', 'done', 'closed', 'cancelled'],
  type: ['task', 'bug', 'idea', 'decision'],
  kind: ['technical', 'product', 'business', 'project', 'unspecified'],
  priority: ['high', 'medium', 'low', 'unset']
};

export var state = {
  viewMode: 'board',
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

export function norm(value) {
  return String(value == null ? '' : value).toLowerCase();
}

export function itemStatus(item) {
  return (item && item.status) || 'created';
}

export function itemType(item) {
  return (item && item.type) || 'task';
}

export function kind(item) {
  return (item && item.kind) || 'unspecified';
}

export function priority(item) {
  return (item && item.priority) || 'unset';
}

export function tags(item) {
  return (item && Array.isArray(item.tags)) ? item.tags.map(String) : [];
}

export function find(id) {
  if (!id) return null;
  for (var i = 0; i < state.items.length; i++) {
    if (state.items[i].id === id) return state.items[i];
  }
  for (var j = 0; j < state.items.length; j++) {
    if (state.items[j].id && state.items[j].id.indexOf(id) === 0) return state.items[j];
  }
  return null;
}

export function hasUnresolvedPrereqs(item) {
  if (!item || !item.depends_on || !item.depends_on.length) return false;
  for (var i = 0; i < item.depends_on.length; i++) {
    var depId = item.depends_on[i];
    var depItem = find(depId);
    if (!depItem) return true;
    var s = itemStatus(depItem);
    if (s !== 'done' && s !== 'closed') return true;
  }
  return false;
}

export function matches(item) {
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

export function updateShown() {
  state.shown = state.items.filter(matches);
  return state.shown;
}

export function unique(pick) {
  var seen = Object.create(null), out = [];
  state.items.forEach(function (item) {
    var value = pick(item);
    if (value && !seen[value]) { seen[value] = true; out.push(value); }
  });
  return out;
}

export function fillSelect(select, values, first, order) {
  if (!select) return;
  var cur = select.value;
  if (typeof select.replaceChildren === 'function') {
    select.replaceChildren();
  } else {
    select.textContent = '';
  }
  var base = document.createElement('option');
  base.textContent = first || '';
  base.value = '';
  select.appendChild(base);
  var sorted = values.slice();
  if (order) {
    sorted.sort(function (a, b) {
      var ia = order.indexOf(a), ib = order.indexOf(b);
      if (ia === -1 && ib === -1) return a.localeCompare(b);
      if (ia === -1) return 1;
      if (ib === -1) return -1;
      return ia - ib;
    });
  } else {
    sorted.sort();
  }
  sorted.forEach(function (value) {
    var option = document.createElement('option');
    option.textContent = value;
    option.value = value;
    select.appendChild(option);
  });
  select.value = cur;
}
