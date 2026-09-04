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
    scope: $('scope'), search: $('search'), count: $('count'), filterToggle: $('filter-toggle'), filters: $('filters'),
    status: $('status-filter'), type: $('type-filter'), kind: $('kind-filter'), priority: $('priority-filter'),
    projectWrap: $('project-wrap'), project: $('project-filter'), projects: $('projects'), clear: $('clear-filters'),
    list: $('list'), listEmpty: $('list-empty'), reader: $('reader'), detailEmpty: $('detail-empty'), proposal: $('proposal'), back: $('back'),
    context: $('proposal-context'), title: $('proposal-title'), summary: $('proposal-summary'),
    handoffCard: $('handoff-card'), actions: $('proposal-actions'),
    more: $('proposal-more'), meta: $('proposal-meta'), resolution: $('resolution'), body: $('proposal-body'),
    activitySection: $('activity-section'), activityList: $('activity-list'),
    snapshot: $('snapshot'), liveStatus: $('live-status')
  };

  var state = {
    items: [],
    shown: [],
    selected: null,
    scope: '',
    archive: '',
    captured: '',
    filters: { text: '', status: '', type: '', kind: '', priority: '', project: '' }
  };

  function norm(value) { return String(value == null ? '' : value).toLowerCase(); }
  function text(node, value) { node.textContent = value == null ? '' : String(value); }
  function clear(node) { while (node.firstChild) node.removeChild(node.firstChild); }
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

  function captured(value) {
    var d = new Date(value); if (isNaN(d.getTime())) return value || '';
    try { return d.toLocaleString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }); } catch (_) { return d.toISOString(); }
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
      return;
    }

    state.shown.forEach(function (item) {
      var li = node('li');
      var button = node('button', 'proposal-item');
      button.type = 'button';
      button.dataset.id = item.id;
      button.setAttribute('aria-current', item.id === state.selected ? 'true' : 'false');

      var titleEl = node('strong', '', item.title || '(untitled)');
      button.appendChild(titleEl);

      var meta = node('span');
      meta.appendChild(node('span', 'type-badge', itemType(item)));
      meta.appendChild(node('span', 'status-badge status-' + itemStatus(item), itemStatus(item)));

      if (item.claimed_by) {
        meta.appendChild(node('span', 'claimer-badge', '@' + item.claimed_by));
      }

      if (priority(item) !== 'unset') {
        meta.appendChild(node('span', 'priority-' + priority(item), priority(item)));
      }

      var time = node('time', '', date(item.timestamp, false));
      time.dateTime = String(item.timestamp || '');
      meta.appendChild(time);

      button.appendChild(meta);
      button.addEventListener('click', function () { setRoute(item.id); });
      li.appendChild(button);
      els.list.appendChild(li);
    });
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

  function sendAction(id, payload) {
    var url = BASE + 'items/' + encodeURIComponent(id) + '/action';
    fetch(url, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'X-Pin-Action': 'true'
      },
      body: JSON.stringify(payload)
    })
    .then(function (res) {
      if (!res.ok) {
        return res.json().then(function (e) { throw new Error(e.error || ('HTTP ' + res.status)); });
      }
      return res.json();
    })
    .then(function () {
      refreshData();
    })
    .catch(function (err) {
      alert('Action failed: ' + err.message);
    });
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
        sendAction(item.id, { action: 'transition', to: 'planned', expect_revision: rev });
      });
    }

    if (st === 'planned') {
      btn('Claim (Start Work)', true, function () {
        sendAction(item.id, { action: 'claim', lease: 3600, expect_revision: rev });
      });
      btn('Mark Blocked', false, function () {
        var note = prompt('Reason for blocking:');
        if (note != null) sendAction(item.id, { action: 'transition', to: 'blocked', note: note, expect_revision: rev });
      });
    }

    if (st === 'in_progress') {
      btn('Complete with Evidence', true, function () {
        var evidence = prompt('Verification evidence (tests output, command output):');
        if (evidence && evidence.trim()) {
          sendAction(item.id, { action: 'complete', evidence: evidence.trim(), expect_revision: rev });
        }
      });
      btn('Release Claim', false, function () {
        sendAction(item.id, { action: 'release', force: true, expect_revision: rev });
      });
      btn('Mark Blocked', false, function () {
        var note = prompt('Reason for blocking:');
        if (note != null) sendAction(item.id, { action: 'transition', to: 'blocked', note: note, expect_revision: rev });
      });
    }

    if (st === 'blocked') {
      btn('Unblock (Move to Planned)', true, function () {
        sendAction(item.id, { action: 'transition', to: 'planned', expect_revision: rev });
      });
    }

    if (st === 'done') {
      btn('Close Item', true, function () {
        sendAction(item.id, { action: 'close', expect_revision: rev });
      });
      btn('Reopen to Planned', false, function () {
        sendAction(item.id, { action: 'transition', to: 'planned', expect_revision: rev });
      });
    }

    if (st !== 'closed' && st !== 'cancelled') {
      btn('Cancel', false, function () {
        if (confirm('Cancel this work item?')) {
          sendAction(item.id, { action: 'transition', to: 'cancelled', expect_revision: rev });
        }
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
      p.appendChild(document.createTextNode(h.progress));
      els.handoffCard.appendChild(p);
    }
    if (h.next) {
      var n = node('div', 'handoff-field');
      n.appendChild(node('strong', '', 'Next steps:'));
      n.appendChild(document.createTextNode(h.next));
      els.handoffCard.appendChild(n);
    }
    if (h.blocker) {
      var b = node('div', 'handoff-field');
      b.appendChild(node('strong', '', 'Blocker:'));
      b.appendChild(document.createTextNode(h.blocker));
      els.handoffCard.appendChild(b);
    }
    if (h.verification) {
      var v = node('div', 'handoff-field');
      v.appendChild(node('strong', '', 'Verification evidence:'));
      v.appendChild(document.createTextNode(h.verification));
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

    renderHandoff(item);
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

    document.addEventListener('keydown', function (event) {
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
    if (!currentRoute && !isNarrow() && state.items.length) {
      history.replaceState(null, '', '#/item/' + encodeURIComponent(state.items[0].id));
    }
    applyRoute();
  }

  function refreshData() {
    fetch(BASE + 'data.json', { credentials: 'same-origin', cache: 'no-store' })
      .then(function (res) {
        if (!res.ok) throw new Error('Data load error: ' + res.status);
        return res.json();
      })
      .then(function (data) {
        ingest(data);
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
    fetch(BASE + 'data.json', { credentials: 'same-origin', cache: 'no-store' })
      .then(function (res) {
        if (!res.ok) throw new Error('Could not load vault (' + res.status + ')');
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
