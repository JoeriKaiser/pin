import { state, find, itemStatus, itemType, kind, priority, tags, matches } from './state.js';
import { els } from './dom.js';
import { sendAction } from './api.js';
import { node, text, clear, date, timeAgo, showToast } from './formatters.js';
import { renderMarkdownSnippet, renderMarkdown } from './markdown.js';
import { setRoute } from './router.js';
import { renderBoard } from './board.js';

var callbacks = {
  sendRunItem: function () {},
  convertIdeaToSpec: function () {},
  openWorktreeModal: function () {},
  openBlockedModal: function () {},
  openAgentDrawer: function () {},
  openCompleteModal: function () {},
  sendCommitPrItem: function () {},
  openCloseModal: function () {}
};

export function setDetailCallbacks(cbs) {
  if (cbs) Object.assign(callbacks, cbs);
}

export function addSummary(value, className) {
  if (value && els.summary) els.summary.appendChild(node('span', className || '', value));
}

export function addMeta(label, value) {
  if (!value || !els.meta) return;
  els.meta.appendChild(node('dt', '', label));
  els.meta.appendChild(node('dd', '', value));
}

export function renderList() {
  state.shown = state.items.filter(matches);
  clear(els.list);
  var countText = state.shown.length === state.items.length
    ? state.items.length + (state.items.length === 1 ? ' item' : ' items')
    : state.shown.length + ' of ' + state.items.length;
  text(els.count, countText);

  if (els.list) els.list.hidden = !state.shown.length;
  if (els.listEmpty) els.listEmpty.hidden = !!state.shown.length;

  if (!state.shown.length) {
    if (els.listEmpty) {
      clear(els.listEmpty);
      els.listEmpty.appendChild(node('strong', '', state.items.length ? 'No items match filters.' : 'No items in this vault.'));
      els.listEmpty.appendChild(node('p', '', state.items.length ? 'Try clearing search or filters.' : 'Add items with pin add.'));
    }
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
    if (els.list) els.list.appendChild(li);
  });

  renderBoard();
}

export function renderWorkerBox(item) {
  if (!els.workerCard) return;
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

export function renderDependencies(item) {
  if (!els.dependenciesCard) return;
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

export function renderActions(item) {
  if (!els.actions) return;
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
    if (item.type === 'idea') {
      btn('Start Idea Mode', true, function () {
        callbacks.sendRunItem(item.id, false, 'Failed to start Idea Mode: ', 'idea');
      });
      btn('Convert to Spec', false, function () {
        callbacks.convertIdeaToSpec(item);
      });
    } else {
      btn('Discuss in Idea Mode', false, function () {
        callbacks.sendRunItem(item.id, false, 'Failed to start Idea Mode: ', 'idea');
      });
    }
    btn('Move to Planned', item.type !== 'idea', function () {
      sendAction(item.id, { action: 'transition', to: 'planned', expect_revision: rev })
        .then(function () { showToast('Moved to Planned', false); })
        .catch(function (err) { showToast('Action failed: ' + err.message, true); });
    });
  }

  if (st === 'planned') {
    if (item.type === 'idea') {
      btn('Start Idea Mode', true, function () {
        callbacks.sendRunItem(item.id, false, 'Failed to start Idea Mode: ', 'idea');
      });
      btn('Convert to Spec', false, function () {
        callbacks.convertIdeaToSpec(item);
      });
    } else {
      btn('Start Agent', true, function () {
        callbacks.openWorktreeModal(item, null);
      });
      btn('Discuss in Idea Mode', false, function () {
        callbacks.sendRunItem(item.id, false, 'Failed to start Idea Mode: ', 'idea');
      });
    }
    btn('Mark Blocked', false, function () {
      callbacks.openBlockedModal(item);
    });
  }

  if (st === 'in_progress') {
    if (state.activeRuns.has(item.id)) {
      btn('View Agent Run', true, function () {
        callbacks.openAgentDrawer(item.id);
      });
    } else {
      if (item.type === 'idea') {
        btn('Resume Idea Mode', true, function () {
          callbacks.sendRunItem(item.id, false, 'Failed to resume Idea Mode: ', 'idea');
        });
        btn('Convert to Spec', false, function () {
          callbacks.convertIdeaToSpec(item);
        });
      } else {
        btn('Resume Agent', true, function () {
          callbacks.openWorktreeModal(item, null);
        });
      }
    }
    btn('Complete with Evidence', !state.activeRuns.has(item.id), function () {
      callbacks.openCompleteModal(item);
    });
    btn('Mark Blocked', false, function () {
      callbacks.openBlockedModal(item);
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
        callbacks.openAgentDrawer(item.id);
      });
    } else if (item.git_status === 'pr_open' || (item.pr_url && item.pr_url.length > 0)) {
      var prLabel = item.pr_number ? 'View PR #' + item.pr_number + ' ↗' : 'View PR ↗';
      btn(prLabel, true, function () {
        if (item.pr_url) window.open(item.pr_url, '_blank', 'noopener,noreferrer');
      });
    } else if (item.git_status === 'merged') {
      var mergedInd = node('span', 'indicator-merged', 'Merged into main');
      mergedInd.title = 'Merged into main branch';
      els.actions.appendChild(mergedInd);
    } else {
      btn('Commit + PR', true, function () {
        callbacks.sendCommitPrItem(item.id);
      });
    }
    btn('Close Item', false, function () {
      callbacks.openCloseModal(item);
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
        callbacks.openAgentDrawer(item.id);
      });
    } else if (item.git_status === 'pr_open' || (item.pr_url && item.pr_url.length > 0)) {
      var prLabel = item.pr_number ? 'View PR #' + item.pr_number + ' ↗' : 'View PR ↗';
      btn(prLabel, true, function () {
        if (item.pr_url) window.open(item.pr_url, '_blank', 'noopener,noreferrer');
      });
    } else if (item.git_status === 'merged') {
      var mergedInd = node('span', 'indicator-merged', 'Merged into main');
      mergedInd.title = 'Merged into main branch';
      els.actions.appendChild(mergedInd);
    } else {
      btn('Commit + PR', true, function () {
        callbacks.sendCommitPrItem(item.id);
      });
    }
    btn('Close Item', false, function () {
      callbacks.openCloseModal(item);
    });
    btn('Reopen to Planned', false, function () {
      sendAction(item.id, { action: 'transition', to: 'planned', expect_revision: rev })
        .then(function () { showToast('Reopened to Planned', false); })
        .catch(function (err) { showToast('Action failed: ' + err.message, true); });
    });
  }

  if (st !== 'closed' && st !== 'cancelled' && st !== 'done' && st !== 'review') {
    btn('Close Item', false, function () {
      callbacks.openCloseModal(item);
    });
  }

  els.actions.hidden = !els.actions.children.length;
}

export function renderHandoff(item) {
  if (!els.handoffCard) return;
  clear(els.handoffCard);
  var h = item.handoff;
  if (!h || (!h.progress && !h.next && !h.blocker && !h.verification)) {
    els.handoffCard.hidden = true;
    return;
  }

  els.handoffCard.hidden = false;
  els.handoffCard.appendChild(node('h3', '', 'Agent Handoff & Evidence'));

  var st = itemStatus(item);

  function addField(type, labelText, pillText, pillClass, content, isPreformatted) {
    if (!content || !String(content).trim()) return;
    var field = node('div', 'handoff-field handoff-field-' + type);
    var header = node('div', 'handoff-field-header');
    header.appendChild(node('strong', '', labelText));
    if (pillText) {
      header.appendChild(node('span', 'handoff-pill ' + pillClass, pillText));
    }
    field.appendChild(header);

    if (isPreformatted) {
      field.appendChild(node('pre', 'handoff-evidence', String(content).trim()));
    } else {
      var body = node('div', 'handoff-text');
      renderMarkdownSnippet(content, body);
      field.appendChild(body);
    }
    els.handoffCard.appendChild(field);
  }

  if (st === 'blocked') {
    addField('blocker', 'Blocker:', 'Blocked', 'handoff-pill-blocker', h.blocker, false);
    addField('verification', 'Verification evidence:', 'Evidence', 'handoff-pill-verification', h.verification, true);
    addField('progress', 'Progress:', null, null, h.progress, false);
    addField('next', 'Next steps:', null, null, h.next, false);
  } else {
    addField('verification', 'Verification evidence:', 'Evidence', 'handoff-pill-verification', h.verification, true);
    addField('progress', 'Progress:', null, null, h.progress, false);
    addField('next', 'Next steps:', null, null, h.next, false);
    addField('blocker', 'Blocker:', 'Blocker', 'handoff-pill-blocker', h.blocker, false);
  }
}

export function renderActivity(item) {
  if (!els.activityList || !els.activitySection) return;
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

export function setReaderTab(tab) {
  state.readerTab = tab;
  if (tab === 'trajectory') {
    if (els.tabAgentTrajectory) els.tabAgentTrajectory.classList.add('active');
    if (els.tabTaskSpec) els.tabTaskSpec.classList.remove('active');
    if (els.proposal) els.proposal.hidden = true;
    if (els.agentDrawer) els.agentDrawer.hidden = false;

    if (state.currentStreamingId === state.selected) {
      return;
    }
    if (state.selected) {
      callbacks.openAgentDrawer(state.selected);
    }
  } else {
    if (els.tabTaskSpec) els.tabTaskSpec.classList.add('active');
    if (els.tabAgentTrajectory) els.tabAgentTrajectory.classList.remove('active');
    if (els.proposal) els.proposal.hidden = false;
    if (els.agentDrawer) els.agentDrawer.hidden = true;
  }
}

export function renderDetail() {
  var item = find(state.selected);
  if (typeof document !== 'undefined' && document.body) {
    document.body.classList.toggle('detail-open', !!item);
  }
  if (els.detailEmpty) els.detailEmpty.hidden = !!item;

  if (!item) {
    if (els.readerTabs) els.readerTabs.hidden = true;
    if (els.proposal) els.proposal.hidden = true;
    if (els.agentDrawer) els.agentDrawer.hidden = true;
    if (els.detailEmpty) text(els.detailEmpty, state.selected ? 'This item is not in the vault.' : 'Select an item to view it.');
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
  renderActions(item);
  renderHandoff(item);
  renderDependencies(item);

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
  if (els.more && els.meta) els.more.hidden = !els.meta.children.length;

  clear(els.resolution);
  if (els.resolution) {
    els.resolution.hidden = !(item.resolution || item.resolution_note);
    if (!els.resolution.hidden) {
      els.resolution.appendChild(node('strong', '', item.resolution ? 'Resolution: ' + item.resolution : 'Resolution'));
      if (item.resolution_note) els.resolution.appendChild(node('div', '', item.resolution_note));
    }
  }

  renderMarkdown(item);
  renderActivity(item);
}
