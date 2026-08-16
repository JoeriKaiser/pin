(function () {
  'use strict';

  var BASE = document.documentElement.dataset.base || '';
  var PURIFY_CONFIG = {
    ALLOWED_TAGS: ['p','br','strong','em','b','i','code','pre','blockquote','h1','h2','h3','h4','h5','h6','ul','ol','li','a','hr','table','thead','tbody','tr','th','td','del','ins','sub','sup'],
    ALLOWED_ATTR: ['href','title','class'],
    ALLOWED_URI_REGEXP: /^(?:(?:https?|mailto):|[^a-z]|[a-z+.\-]+(?:[^a-z+.\-:]|$))/i,
    FORBID_TAGS: ['script','iframe','object','embed','form','input','button','textarea','select','style','link','meta','base','img','svg','math'],
    FORBID_ATTR: ['style','src','srcdoc','onerror','onclick','onload','onfocus','onblur','onsubmit'],
    KEEP_CONTENT: true
  };
  var ORDER = { kind: ['technical','product','business','project','unspecified'], priority: ['high','medium','low','unset'] };
  var $ = function (id) { return document.getElementById(id); };
  var els = {
    scope: $('scope'), search: $('search'), count: $('count'), filterToggle: $('filter-toggle'), filters: $('filters'),
    kind: $('kind-filter'), priority: $('priority-filter'), projectWrap: $('project-wrap'), project: $('project-filter'), projects: $('projects'), clear: $('clear-filters'),
    list: $('list'), listEmpty: $('list-empty'), reader: $('reader'), detailEmpty: $('detail-empty'), proposal: $('proposal'), back: $('back'),
    context: $('proposal-context'), title: $('proposal-title'), summary: $('proposal-summary'), more: $('proposal-more'), meta: $('proposal-meta'), resolution: $('resolution'), body: $('proposal-body'), snapshot: $('snapshot')
  };
  var state = { items: [], shown: [], selected: null, scope: '', archive: '', captured: '', filters: { text: '', kind: '', priority: '', project: '' } };

  function norm(value) { return String(value == null ? '' : value).toLowerCase(); }
  function text(node, value) { node.textContent = value == null ? '' : String(value); }
  function clear(node) { while (node.firstChild) node.removeChild(node.firstChild); }
  function node(tag, className, value) { var n = document.createElement(tag); if (className) n.className = className; if (value != null) text(n, value); return n; }
  function kind(item) { return item.kind || 'unspecified'; }
  function priority(item) { return item.priority || 'unset'; }
  function tags(item) { return Array.isArray(item.tags) ? item.tags.map(String) : []; }
  function isNarrow() { return window.matchMedia('(max-width: 799.98px)').matches; }
  function date(value, long) {
    var n = Number(value); if (!isFinite(n)) return '';
    if (n < 1e12) n *= 1000;
    var opts = long ? { year: 'numeric', month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' } : { month: 'short', day: 'numeric' };
    try { return new Date(n).toLocaleDateString(undefined, opts); } catch (_) { return new Date(n).toISOString(); }
  }
  function captured(value) {
    var d = new Date(value); if (isNaN(d.getTime())) return value || '';
    try { return d.toLocaleString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }); } catch (_) { return d.toISOString(); }
  }
  function route() {
    var match = (location.hash || '').match(/^#\/idea\/(.+)$/);
    if (!match) return null;
    try { return decodeURIComponent(match[1]); } catch (_) { return null; }
  }
  function setRoute(id) {
    var next = id ? '#/idea/' + encodeURIComponent(id) : '#/';
    if (location.hash !== next) location.hash = next; else applyRoute();
  }
  function find(id) { for (var i = 0; i < state.items.length; i++) if (state.items[i].id === id) return state.items[i]; return null; }

  function unique(pick) {
    var seen = Object.create(null), out = [];
    state.items.forEach(function (item) { var value = pick(item); if (!seen[value]) { seen[value] = true; out.push(value); } });
    return out;
  }
  function fillSelect(select, values, first, order) {
    clear(select); var base = node('option', '', first); base.value = ''; select.appendChild(base);
    if (order) values.sort(function (a, b) { return order.indexOf(a) - order.indexOf(b); }); else values.sort();
    values.forEach(function (value) { var option = node('option', '', value); option.value = value; select.appendChild(option); });
  }
  function setupFilters() {
    fillSelect(els.kind, unique(kind), 'Any kind', ORDER.kind);
    fillSelect(els.priority, unique(priority), 'Any priority', ORDER.priority);
    var projects = unique(function (item) { return item.project || ''; }).filter(Boolean).sort();
    clear(els.projects);
    projects.forEach(function (value) { var option = node('option'); option.value = value; els.projects.appendChild(option); });
    els.projectWrap.hidden = projects.length < 2;
  }
  function matches(item) {
    var f = state.filters, q = norm(f.text).trim();
    if (f.kind && kind(item) !== f.kind) return false;
    if (f.priority && priority(item) !== f.priority) return false;
    if (f.project && norm(item.project) !== norm(f.project).trim()) return false;
    if (!q) return true;
    return norm([item.title, item.project, item.id, tags(item).join(' '), item.body].join('\n')).indexOf(q) !== -1;
  }
  function filtersActive() { var f = state.filters; return !!(f.text || f.kind || f.priority || f.project); }

  function renderList() {
    state.shown = state.items.filter(matches);
    clear(els.list);
    text(els.count, state.shown.length === state.items.length ? state.items.length + (state.items.length === 1 ? ' proposal' : ' proposals') : state.shown.length + ' of ' + state.items.length);
    els.list.hidden = !state.shown.length;
    els.listEmpty.hidden = !!state.shown.length;
    if (!state.shown.length) {
      clear(els.listEmpty);
      els.listEmpty.appendChild(node('strong', '', state.items.length ? 'No proposals match.' : 'No proposals in this snapshot.'));
      els.listEmpty.appendChild(node('p', '', state.items.length ? 'Try a broader search or clear the filters.' : 'Try another scope or archive mode.'));
      return;
    }
    state.shown.forEach(function (item) {
      var li = node('li');
      var button = node('button', 'proposal-item'); button.type = 'button'; button.dataset.id = item.id;
      button.setAttribute('aria-current', item.id === state.selected ? 'true' : 'false');
      button.appendChild(node('strong', '', item.title || '(untitled)'));
      var meta = node('span');
      meta.appendChild(node('span', '', state.scope === 'all' && item.project ? item.project : kind(item)));
      if (priority(item) !== 'unset') meta.appendChild(node('span', '', priority(item)));
      var time = node('time', '', date(item.timestamp, false)); time.dateTime = String(item.timestamp || ''); meta.appendChild(time);
      button.appendChild(meta);
      button.addEventListener('click', function () { setRoute(item.id); });
      li.appendChild(button); els.list.appendChild(li);
    });
  }

  function addSummary(value, className) { if (value) els.summary.appendChild(node('span', className || '', value)); }
  function addMeta(label, value) { if (!value) return; els.meta.appendChild(node('dt', '', label)); els.meta.appendChild(node('dd', '', value)); }
  function bodyWithoutDuplicateTitle(body, title) {
    var source = String(body || ''), match = source.match(/^\s*#\s+(.+?)\s*(?:\n|$)/);
    return match && norm(match[1].replace(/[*_`]/g, '').trim()) === norm(title).trim() ? source.slice(match[0].length).replace(/^\s+/, '') : source;
  }
  function renderMarkdown(item) {
    var source = bodyWithoutDuplicateTitle(item.body, item.title), dirty;
    try { dirty = marked.parse(source, { async: false }); } catch (_) { text(els.body, source); return; }
    try { els.body.innerHTML = DOMPurify.sanitize(dirty, PURIFY_CONFIG); } catch (_) { text(els.body, source); }
  }
  function renderDetail() {
    var item = find(state.selected);
    document.body.classList.toggle('detail-open', !!item);
    els.proposal.hidden = !item; els.detailEmpty.hidden = !!item;
    if (!item) { text(els.detailEmpty, state.selected ? 'This proposal is not in the snapshot.' : 'Select a proposal to read it.'); return; }
    var context = [];
    if (item.project) context.push(item.project); context.push(kind(item));
    text(els.context, context.join(' · ')); text(els.title, item.title || '(untitled)');
    clear(els.summary); addSummary(priority(item) === 'unset' ? '' : priority(item) + ' priority', 'priority-' + priority(item)); addSummary(date(item.timestamp, true)); addSummary(tags(item).join(' · '));
    clear(els.meta); addMeta('ID', item.id); addMeta('File', item.filename); if (item.archived_at) addMeta('Archived', date(item.archived_at, true));
    els.more.hidden = !els.meta.children.length;
    clear(els.resolution); els.resolution.hidden = !(item.resolution || item.resolution_note);
    if (!els.resolution.hidden) { els.resolution.appendChild(node('strong', '', item.resolution ? 'Resolution: ' + item.resolution : 'Resolution')); if (item.resolution_note) els.resolution.appendChild(node('div', '', item.resolution_note)); }
    renderMarkdown(item);
  }
  function applyRoute() {
    state.selected = route();
    renderList(); renderDetail();
    if (state.selected && isNarrow()) { try { els.reader.focus({ preventScroll: true }); } catch (_) { els.reader.focus(); } }
  }

  function bind() {
    els.search.addEventListener('input', function () { state.filters.text = this.value; renderList(); });
    els.kind.addEventListener('change', function () { state.filters.kind = this.value; renderList(); });
    els.priority.addEventListener('change', function () { state.filters.priority = this.value; renderList(); });
    els.project.addEventListener('input', function () { state.filters.project = this.value; renderList(); });
    els.filterToggle.addEventListener('click', function () { var open = els.filters.hidden; els.filters.hidden = !open; this.setAttribute('aria-expanded', String(open)); });
    els.clear.addEventListener('click', function () { state.filters = { text: '', kind: '', priority: '', project: '' }; els.search.value = els.kind.value = els.priority.value = els.project.value = ''; renderList(); });
    els.back.addEventListener('click', function () { setRoute(null); });
    window.addEventListener('hashchange', applyRoute);
    document.addEventListener('keydown', function (event) {
      var input = /^(INPUT|SELECT|TEXTAREA)$/.test(event.target.tagName);
      if (event.key === '/' && !input) { event.preventDefault(); els.search.focus(); return; }
      if (event.key === 'Escape') { if (input) event.target.blur(); else if (state.selected) setRoute(null); return; }
      if (input || event.metaKey || event.ctrlKey || event.altKey || (event.key !== 'j' && event.key !== 'k' && event.key !== 'Enter')) return;
      var index = state.shown.findIndex(function (item) { return item.id === state.selected; });
      if (event.key === 'j' || event.key === 'k') { event.preventDefault(); index = Math.max(0, Math.min(state.shown.length - 1, (index < 0 ? 0 : index) + (event.key === 'j' ? 1 : -1))); if (state.shown[index]) setRoute(state.shown[index].id); }
      else if (event.key === 'Enter' && state.shown[index]) { event.preventDefault(); setRoute(state.shown[index].id); }
    });
  }

  function ingest(data) {
    state.items = Array.isArray(data.items) ? data.items : []; state.scope = String(data.scope || 'all'); state.archive = String(data.archive_filter || ''); state.captured = String(data.captured_at || '');
    text(els.scope, state.scope === 'all' ? 'All projects' : state.scope); text(els.snapshot, 'Captured ' + captured(state.captured) + (state.archive ? ' · ' + state.archive : ''));
    setupFilters();
    if (!route() && !isNarrow() && state.items.length) history.replaceState(null, '', '#/idea/' + encodeURIComponent(state.items[0].id));
    applyRoute();
  }
  function fatal(message) { text(els.count, 'Unavailable'); clear(els.listEmpty); els.listEmpty.hidden = false; els.listEmpty.appendChild(node('p', '', message)); }
  function boot() { bind(); fetch(BASE + '/data.json', { credentials: 'same-origin', cache: 'no-store' }).then(function (res) { if (!res.ok) throw new Error('Could not load snapshot (' + res.status + ')'); return res.json(); }).then(ingest).catch(function (err) { fatal(err.message || 'Could not load snapshot.'); }); }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', boot); else boot();
})();
