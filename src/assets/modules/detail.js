import { state, find, itemStatus, itemType, kind, priority, tags, matches, updateShown, isRunWaiting, getRunInfo } from './state.js';
import { els } from './dom.js';
import { sendAction, ensureActionOk, fetchWorktree, deleteWorktree } from './api.js';
import { node, text, clear, date, timeAgo, showToast, formatStatusLabel } from './formatters.js';
import { renderMarkdownSnippet, renderMarkdown } from './markdown.js';
import { setRoute } from './router.js';
import { summarizeQuestionPreview } from './alerts.js';


var callbacks = {
  sendRunItem: function () {},
  convertIdeaToSpec: function () {},
  openWorktreeModal: function () {},
  openBlockedModal: function () {},
  openAgentDrawer: function () {},
  openReplyModal: function () {},
  openCompleteModal: function () {},
  sendCommitPrItem: function () {},
  openCloseModal: function () {},
  refreshData: function () {}
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
  state.shown = updateShown();
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
      var hasItems = state.items.length > 0;
      var queryText = state.filters && state.filters.text ? state.filters.text.trim() : '';
      var heading = hasItems
        ? (queryText ? ('No items match "' + queryText + '"') : 'No items match these filters')
        : 'No items yet';
      var hint = hasItems
        ? 'Clear active filters or search terms to see all items.'
        : 'Create a task here or add one from the terminal with pin add.';
      els.listEmpty.appendChild(node('strong', '', heading));
      els.listEmpty.appendChild(node('p', '', hint));
      var actionBtn = node('button', hasItems ? 'control empty-action-btn' : 'action-btn primary empty-action-btn', hasItems ? 'Clear filters' : 'New task');
      actionBtn.type = 'button';
      actionBtn.addEventListener('click', function () {
        if (hasItems) {
          if (els.clear && els.clear.click) els.clear.click();
        } else if (els.btnNewTicket && els.btnNewTicket.click) {
          els.btnNewTicket.click();
        }
      });
      els.listEmpty.appendChild(actionBtn);
    }

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
    meta.appendChild(node('span', 'status-badge status-' + itemStatus(item), formatStatusLabel(itemStatus(item))));

    if (priority(item) !== 'unset') {
      meta.appendChild(node('span', 'priority-' + priority(item), priority(item)));
    }

    var time = node('time', '', timeAgo(item.timestamp));
    time.dateTime = String(item.timestamp || '');
    var fullDate = date(item.timestamp, true);
    if (fullDate) time.title = fullDate;
    meta.appendChild(time);

    button.appendChild(meta);
    button.addEventListener('click', function () { setRoute(item.id); });
    li.appendChild(button);
    if (els.list) els.list.appendChild(li);
  });


}

export function renderWorkerBox(item) {
  if (!els.workerCard) return;
  clear(els.workerCard);
  els.workerCard.appendChild(node('h3', '', 'Agent Run'));

  var header = node('div', 'worker-card-header');
  var isActive = state.activeRuns.has(item.id);
  var waiting = isActive && isRunWaiting(item.id);
  var runInfo = isActive ? getRunInfo(item.id) : null;
  if (waiting) {
    var waitBadge = node('span', 'card-waiting-indicator');
    var waitDot = node('span', 'needs-reply-dot');
    waitDot.setAttribute('aria-hidden', 'true');
    waitBadge.appendChild(waitDot);
    waitBadge.appendChild(document.createTextNode('Needs reply'));
    header.appendChild(waitBadge);
  } else if (isActive) {
    var modeText = runInfo && runInfo.mode === 'commit_pr'
      ? 'Commit + PR'
      : (runInfo && runInfo.mode === 'idea' ? 'Idea agent' : 'Implementing');
    var actorLabel = item.claimed_by ? '@' + item.claimed_by + ' · ' + modeText : modeText;
    header.appendChild(node('span', 'worker-actor', actorLabel));
  } else {
    header.appendChild(node('span', 'worker-actor', 'No active run'));
  }
  if (item.revision != null) {
    header.appendChild(node('span', 'worker-revision', 'rev ' + item.revision));
  }
  els.workerCard.appendChild(header);

  if (waiting) {
    var callout = node('div', 'worker-waiting-callout');
    callout.appendChild(node('p', 'worker-waiting-preview', summarizeQuestionPreview(runInfo && runInfo.last_message, 160)));
    var replyNowBtn = node('button', 'action-btn primary action-btn-small', 'Reply Now');
    replyNowBtn.type = 'button';
    replyNowBtn.addEventListener('click', function () {
      callbacks.openReplyModal(item.id);
    });
    callout.appendChild(replyNowBtn);
    els.workerCard.appendChild(callout);
  }

  var meta = node('div', 'worker-meta');
  if (runInfo) {
    if (runInfo.model) {
      meta.appendChild(node('span', '', 'Model: ' + runInfo.model));
    }
    if (runInfo.worktree_path) {
      meta.appendChild(node('span', '', 'Worktree: ' + runInfo.worktree_path));
    } else if (runInfo.is_primary_worktree) {
      meta.appendChild(node('span', '', 'Checkout: primary'));
    }
  }
  if (item.created_by) {
    meta.appendChild(node('span', '', 'Created by @' + item.created_by));
  }
  if (meta.children.length) {
    els.workerCard.appendChild(meta);
  }

  var showWorktree = !!(item.worktree_path || (runInfo && runInfo.worktree_path) || item.git_status);
  if (showWorktree && typeof document !== 'undefined') {
    var wtDetails = document.createElement('details');
    wtDetails.className = 'worktree-inspector';
    var wtSummary = document.createElement('summary');
    wtSummary.textContent = 'Inspect worktree';
    wtDetails.appendChild(wtSummary);

    var wtBody = node('div', 'worktree-inspector-body');
    wtDetails.appendChild(wtBody);

    function loadWorktreeInfo() {
      clear(wtBody);
      wtBody.appendChild(node('div', 'help-text', 'Loading worktree status…'));
      fetchWorktree(item.id)
        .then(function (res) { return res.ok ? res.json() : Promise.reject(new Error('Failed to inspect worktree')); })
        .then(function (info) {
          clear(wtBody);
          if (!info || !info.exists) {
            wtBody.appendChild(node('div', 'help-text', 'No active worktree on disk.'));
            return;
          }
          var summaryLine = [];
          if (info.branch) summaryLine.push('Branch: ' + info.branch);
          if (info.ahead_count > 0) summaryLine.push(info.ahead_count + ' commit' + (info.ahead_count === 1 ? '' : 's') + ' ahead');
          summaryLine.push(info.changed_files + ' modified file' + (info.changed_files === 1 ? '' : 's'));
          wtBody.appendChild(node('div', 'worktree-summary-line', summaryLine.join(' · ')));

          if (info.status_lines && info.status_lines.length) {
            var preStatus = document.createElement('pre');
            preStatus.className = 'worktree-diff-pre';
            preStatus.textContent = info.status_lines.join('\n');
            wtBody.appendChild(preStatus);
          }

          if (info.diff_preview && info.diff_preview.trim()) {
            var diffDetails = document.createElement('details');
            diffDetails.className = 'worktree-diff-details';
            var diffSum = document.createElement('summary');
            diffSum.textContent = 'Diff preview';
            diffDetails.appendChild(diffSum);
            var preDiff = document.createElement('pre');
            preDiff.className = 'worktree-diff-pre';
            preDiff.textContent = info.diff_preview;
            diffDetails.appendChild(preDiff);
            wtBody.appendChild(diffDetails);
          }

          var wtActions = node('div', 'worktree-inspector-actions');
          var refreshBtn = node('button', 'control', 'Refresh');
          refreshBtn.type = 'button';
          refreshBtn.addEventListener('click', loadWorktreeInfo);
          wtActions.appendChild(refreshBtn);

          if (!info.running) {
            var discardBtn = node('button', 'action-btn danger action-btn-small', 'Discard worktree');
            discardBtn.type = 'button';
            discardBtn.addEventListener('click', function () {
              discardBtn.disabled = true;
              deleteWorktree(item.id)
                .then(ensureActionOk)
                .then(function () {
                  showToast('Worktree discarded', false);
                  callbacks.refreshData();
                })
                .catch(function (err) {
                  discardBtn.disabled = false;
                  showToast(err.message || 'Failed to discard worktree', true);
                });
            });
            wtActions.appendChild(discardBtn);
          }
          wtBody.appendChild(wtActions);
        })
        .catch(function (err) {
          clear(wtBody);
          wtBody.appendChild(node('div', 'help-text', err.message || 'Could not inspect worktree'));
        });
    }

    wtDetails.addEventListener('toggle', function () {
      if (wtDetails.open) loadWorktreeInfo();
    });
    els.workerCard.appendChild(wtDetails);
  }
}

export function renderDependencies(item) {
  if (!els.dependenciesCard) return;
  var preservedForm = null;
  if (els.dependenciesCard.dataset && els.dependenciesCard.dataset.itemId === item.id && els.dependenciesCard.querySelector) {
    var existingForm = els.dependenciesCard.querySelector('.dep-add-form');
    if (existingForm && existingForm.style && existingForm.style.display !== 'none') {
      var existingSel = existingForm.querySelector('.dep-add-select');
      var existingInp = existingForm.querySelector('.dep-add-input');
      preservedForm = {
        relType: existingSel ? existingSel.value : 'depend',
        inputVal: existingInp ? existingInp.value : '',
        hadFocus: typeof document !== 'undefined' && document.activeElement === existingInp
      };
    }
  }
  if (els.dependenciesCard.dataset) els.dependenciesCard.dataset.itemId = item.id;
  clear(els.dependenciesCard);
  els.dependenciesCard.appendChild(node('h3', '', 'Relationships'));

  var list = node('ul', 'dependencies-list');
  var hasAny = false;

  // 1. Parent epic or item
  if (item.parent_id) {
    hasAny = true;
    var parentItem = find(item.parent_id);
    var parentStatus = parentItem ? itemStatus(parentItem) : null;
    var parentLi = node('li', 'dependency-item');
    var parentInd = node('span', 'dep-indicator', '⇡');
    parentInd.title = 'Parent item';
    parentLi.appendChild(parentInd);

    var parentBtn = node('button', 'dependency-link', (parentItem && parentItem.title ? parentItem.title : item.parent_id));
    parentBtn.type = 'button';
    parentBtn.title = 'Parent: ' + item.parent_id;
    parentBtn.addEventListener('click', function () { setRoute(item.parent_id); });
    parentLi.appendChild(parentBtn);

    if (parentStatus) {
      parentLi.appendChild(node('span', 'status-badge status-' + parentStatus, formatStatusLabel(parentStatus)));
    }

    var parentRm = node('button', 'dep-remove-btn', '✕');
    parentRm.type = 'button';
    parentRm.title = 'Remove parent';
    parentRm.setAttribute('aria-label', 'Remove parent');
    parentRm.addEventListener('click', function (e) {
      e.stopPropagation();
      sendAction(item.id, { action: 'unparent', expect_revision: item.revision })
        .then(ensureActionOk)
        .then(function () {
          showToast('Parent removed', false);
          callbacks.refreshData();
        })
        .catch(function (err) { showToast('Action failed: ' + err.message, true); });
    });
    parentLi.appendChild(parentRm);

    list.appendChild(parentLi);
  }

  // 2. Subtasks
  var subtasks = (state.items || []).filter(function (other) {
    return other.parent_id === item.id;
  });

  if (subtasks.length) {
    subtasks.forEach(function (sub) {
      hasAny = true;
      var subStatus = itemStatus(sub);
      var subLi = node('li', 'dependency-item');
      var subInd = node('span', 'dep-indicator', '⇣');
      subInd.title = 'Subtask';
      subLi.appendChild(subInd);

      var subBtn = node('button', 'dependency-link', sub.title || sub.id);
      subBtn.type = 'button';
      subBtn.title = 'Subtask: ' + sub.id;
      subBtn.addEventListener('click', function () { setRoute(sub.id); });
      subLi.appendChild(subBtn);
      subLi.appendChild(node('span', 'status-badge status-' + subStatus, formatStatusLabel(subStatus)));

      var subRm = node('button', 'dep-remove-btn', '✕');
      subRm.type = 'button';
      subRm.title = 'Unlink subtask';
      subRm.setAttribute('aria-label', 'Unlink subtask ' + sub.id);
      subRm.addEventListener('click', function (e) {
        e.stopPropagation();
        sendAction(sub.id, { action: 'unparent', expect_revision: sub.revision })
          .then(ensureActionOk)
          .then(function () {
            showToast('Subtask unlinked', false);
            callbacks.refreshData();
          })
          .catch(function (err) { showToast('Action failed: ' + err.message, true); });
      });
      subLi.appendChild(subRm);

      list.appendChild(subLi);
    });
  }

  // 3. Prerequisites
  if (item.depends_on && item.depends_on.length) {
    item.depends_on.forEach(function (depId) {
      hasAny = true;
      var depItem = find(depId);
      var isDone = depItem && (itemStatus(depItem) === 'done' || itemStatus(depItem) === 'closed');
      var li = node('li', 'dependency-item');
      var ind = node('span', 'dep-indicator ' + (isDone ? 'resolved' : 'unresolved'), isDone ? '✓' : '○');
      ind.title = isDone ? 'Resolved prerequisite' : 'Unresolved prerequisite';
      li.appendChild(ind);

      var btn = node('button', 'dependency-link', (depItem && depItem.title ? depItem.title : depId));
      btn.type = 'button';
      btn.title = 'Prerequisite: ' + depId;
      btn.addEventListener('click', function () { setRoute(depId); });
      li.appendChild(btn);

      if (depItem) {
        li.appendChild(node('span', 'status-badge status-' + itemStatus(depItem), formatStatusLabel(itemStatus(depItem))));
      }

      var depRm = node('button', 'dep-remove-btn', '✕');
      depRm.type = 'button';
      depRm.title = 'Remove prerequisite';
      depRm.setAttribute('aria-label', 'Remove prerequisite ' + depId);
      depRm.addEventListener('click', function (e) {
        e.stopPropagation();
        sendAction(item.id, { action: 'undepend', target: depId, expect_revision: item.revision })
          .then(ensureActionOk)
          .then(function () {
            showToast('Prerequisite removed', false);
            callbacks.refreshData();
          })
          .catch(function (err) { showToast('Action failed: ' + err.message, true); });
      });
      li.appendChild(depRm);

      list.appendChild(li);
    });
  }

  // 4. Dependents
  var dependents = (state.items || []).filter(function (other) {
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
      li.appendChild(node('span', 'status-badge status-' + itemStatus(dep), formatStatusLabel(itemStatus(dep))));
      list.appendChild(li);
    });
  }

  // 5. Related items
  var relatedIds = [];
  if (item.related && item.related.length) {
    item.related.forEach(function (r) {
      if (relatedIds.indexOf(r) === -1) relatedIds.push(r);
    });
  }
  (state.items || []).forEach(function (other) {
    if (other.id !== item.id && other.related && other.related.indexOf(item.id) !== -1) {
      if (relatedIds.indexOf(other.id) === -1) relatedIds.push(other.id);
    }
  });

  if (relatedIds.length) {
    relatedIds.forEach(function (relId) {
      hasAny = true;
      var relItem = find(relId);
      var relStatus = relItem ? itemStatus(relItem) : null;
      var relLi = node('li', 'dependency-item');
      var relInd = node('span', 'dep-indicator', '↔');
      relInd.title = 'Related item';
      relLi.appendChild(relInd);

      var relBtn = node('button', 'dependency-link', (relItem && relItem.title ? relItem.title : relId));
      relBtn.type = 'button';
      relBtn.title = 'Related: ' + relId;
      relBtn.addEventListener('click', function () { setRoute(relId); });
      relLi.appendChild(relBtn);

      if (relStatus) {
        relLi.appendChild(node('span', 'status-badge status-' + relStatus, formatStatusLabel(relStatus)));
      }

      var relRm = node('button', 'dep-remove-btn', '✕');
      relRm.type = 'button';
      relRm.title = 'Remove relationship';
      relRm.setAttribute('aria-label', 'Remove relationship ' + relId);
      relRm.addEventListener('click', function (e) {
        e.stopPropagation();
        var isOutbound = Array.isArray(item.related) && item.related.indexOf(relId) !== -1;
        var hasInbound = relItem && Array.isArray(relItem.related) && relItem.related.indexOf(item.id) !== -1;
        var calls = [];
        if (isOutbound || !hasInbound) {
          calls.push(sendAction(item.id, { action: 'unrelate', target: relId, expect_revision: item.revision }).then(ensureActionOk));
        }
        if (hasInbound) {
          calls.push(sendAction(relItem.id, { action: 'unrelate', target: item.id, expect_revision: relItem.revision }).then(ensureActionOk));
        }
        Promise.all(calls)
          .then(function () {
            showToast('Relationship removed', false);
            callbacks.refreshData();
          })
          .catch(function (err) { showToast('Action failed: ' + err.message, true); });
      });
      relLi.appendChild(relRm);

      list.appendChild(relLi);
    });
  }

  if (hasAny) {
    els.dependenciesCard.appendChild(list);
  } else {
    els.dependenciesCard.appendChild(node('div', 'help-text', 'No relationships'));
  }

  // 6. Interactive add relationship control
  var addBtn = node('button', 'dep-add-btn', '+ Add relationship');
  addBtn.type = 'button';

  var form = node('div', 'dep-add-form');
  form.style.display = 'none';

  var row = node('div', 'dep-add-row');

  var sel = document.createElement('select');
  sel.className = 'dep-add-select';
  sel.setAttribute('aria-label', 'Relationship type');
  var optDep = document.createElement('option');
  optDep.value = 'depend';
  optDep.textContent = 'Prerequisite';
  var optParent = document.createElement('option');
  optParent.value = 'parent';
  optParent.textContent = 'Parent';
  var optRel = document.createElement('option');
  optRel.value = 'relate';
  optRel.textContent = 'Related';
  sel.appendChild(optDep);
  sel.appendChild(optParent);
  sel.appendChild(optRel);
  row.appendChild(sel);

  var inp = document.createElement('input');
  inp.className = 'dep-add-input';
  inp.placeholder = 'Search item ID or title…';
  inp.setAttribute('aria-label', 'Target item ID or title');
  inp.setAttribute('list', 'dep-items-datalist');
  row.appendChild(inp);
  form.appendChild(row);

  var actionsRow = node('div', 'dep-add-row');
  var confirmBtn = node('button', 'action-btn primary', 'Add');
  confirmBtn.type = 'button';
  var cancelBtn = node('button', 'action-btn', 'Cancel');
  cancelBtn.type = 'button';
  actionsRow.appendChild(confirmBtn);
  actionsRow.appendChild(cancelBtn);
  form.appendChild(actionsRow);

  addBtn.addEventListener('click', function () {
    addBtn.style.display = 'none';
    form.style.display = 'flex';
    inp.focus();
  });

  if (preservedForm) {
    sel.value = preservedForm.relType;
    inp.value = preservedForm.inputVal;
    addBtn.style.display = 'none';
    form.style.display = 'flex';
    if (preservedForm.hadFocus && typeof inp.focus === 'function') {
      try { inp.focus(); } catch (_) {}
    }
  }

  cancelBtn.addEventListener('click', function () {
    form.style.display = 'none';
    addBtn.style.display = '';
    inp.value = '';
  });

  function resolveRelationshipTarget(raw) {
    var firstToken = raw.split(/[\s:]+/)[0];
    var candidates = (state.items || []).filter(function (it) { return it && it.id !== item.id; });
    var exactId = candidates.find(function (it) { return it.id === raw || it.id === firstToken; });
    if (exactId) return { id: exactId.id };

    var lower = raw.toLowerCase();
    var exactTitle = candidates.find(function (it) { return (it.title || '').trim().toLowerCase() === lower; });
    if (exactTitle) return { id: exactTitle.id };

    var partialTitles = candidates.filter(function (it) {
      return (it.title || '').toLowerCase().indexOf(lower) !== -1;
    });
    if (partialTitles.length === 1) return { id: partialTitles[0].id };
    if (partialTitles.length > 1) {
      return { error: 'Multiple items match "' + raw + '". Pick one from the list or enter its ID.' };
    }
    return { id: firstToken };
  }

  function submitRelationship() {
    var raw = inp.value.trim();
    if (!raw) {
      showToast('Enter an item ID or title to link', true);
      return;
    }
    var resolved = resolveRelationshipTarget(raw);
    if (resolved.error) {
      showToast(resolved.error, true);
      return;
    }
    var targetId = resolved.id;
    if (targetId === item.id) {
      showToast('Cannot link item to itself', true);
      return;
    }
    var actionType = sel.value;
    confirmBtn.disabled = true;
    sendAction(item.id, { action: actionType, target: targetId, expect_revision: item.revision })
      .then(ensureActionOk)
      .then(function () {
        showToast('Relationship added', false);
        callbacks.refreshData();
      })
      .catch(function (err) {
        confirmBtn.disabled = false;
        showToast('Action failed: ' + err.message, true);
      });
  }

  confirmBtn.addEventListener('click', submitRelationship);
  inp.addEventListener('keydown', function (e) {
    if (e.key === 'Enter') {
      e.preventDefault();
      submitRelationship();
    }
  });

  var dl = document.getElementById('dep-items-datalist');
  if (!dl && document.body) {
    dl = document.createElement('datalist');
    dl.id = 'dep-items-datalist';
    document.body.appendChild(dl);
  }
  if (dl) {
    clear(dl);
    (state.items || []).forEach(function (other) {
      if (other.id !== item.id) {
        var opt = document.createElement('option');
        opt.value = other.id;
        opt.label = other.title ? other.title.slice(0, 50) : other.id;
        dl.appendChild(opt);
      }
    });
  }

  els.dependenciesCard.appendChild(addBtn);
  els.dependenciesCard.appendChild(form);
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
      btn('Start idea mode', true, function () {
        callbacks.sendRunItem(item.id, false, 'Failed to start idea mode: ', 'idea');
      });
      btn('Convert to spec', false, function () {
        callbacks.convertIdeaToSpec(item);
      });
    } else {
      btn('Discuss in idea mode', false, function () {
        callbacks.sendRunItem(item.id, false, 'Failed to start idea mode: ', 'idea');
      });
    }
    btn('Move to planned', item.type !== 'idea', function () {
      sendAction(item.id, { action: 'transition', to: 'planned', expect_revision: rev })
        .then(ensureActionOk)
        .then(function () {
          showToast('Moved to planned', false);
          callbacks.refreshData();
        })
        .catch(function (err) { showToast('Action failed: ' + err.message, true); });
    });
  }

  if (st === 'planned') {
    if (item.type === 'idea') {
      btn('Start idea mode', true, function () {
        callbacks.sendRunItem(item.id, false, 'Failed to start idea mode: ', 'idea');
      });
      btn('Convert to spec', false, function () {
        callbacks.convertIdeaToSpec(item);
      });
    } else {
      btn('Start agent', true, function () {
        callbacks.openWorktreeModal(item, null);
      });
      btn('Discuss in idea mode', false, function () {
        callbacks.sendRunItem(item.id, false, 'Failed to start idea mode: ', 'idea');
      });
    }
    btn('Mark blocked', false, function () {
      callbacks.openBlockedModal(item);
    });
  }

  if (st === 'in_progress') {
    if (state.activeRuns.has(item.id)) {
      var waitingInProgress = isRunWaiting(item.id);
      if (waitingInProgress) {
        btn('Reply to Agent', true, function () {
          callbacks.openReplyModal(item.id);
        });
      }
      btn('View agent run', !waitingInProgress, function () {
        callbacks.openAgentDrawer(item.id);
      });
    } else {
      if (item.type === 'idea') {
        btn('Resume idea mode', true, function () {
          callbacks.sendRunItem(item.id, false, 'Failed to resume idea mode: ', 'idea');
        });
        btn('Convert to spec', false, function () {
          callbacks.convertIdeaToSpec(item);
        });
      } else {
        btn('Resume agent', true, function () {
          callbacks.openWorktreeModal(item, null);
        });
      }
    }
    btn('Complete with evidence', false, function () {
      callbacks.openCompleteModal(item);
    });
    btn('Mark blocked', false, function () {
      callbacks.openBlockedModal(item);
    });
  }

  if (st === 'blocked') {
    btn('Unblock to planned', true, function () {
      sendAction(item.id, { action: 'transition', to: 'planned', expect_revision: rev })
        .then(ensureActionOk)
        .then(function () {
          showToast('Moved to planned', false);
          callbacks.refreshData();
        })
        .catch(function (err) { showToast('Action failed: ' + err.message, true); });
    });
  }

  if (st === 'done') {
    if (state.activeRuns.has(item.id)) {
      btn('View agent run', true, function () {
        callbacks.openAgentDrawer(item.id);
      });
    } else if (item.git_status === 'merged') {
      var mergedInd = node('span', 'indicator-merged', 'Merged into main');
      mergedInd.title = 'Merged into main branch';
      els.actions.appendChild(mergedInd);
    } else if (item.git_status === 'pr_open' || (item.pr_url && item.pr_url.length > 0)) {
      var prLabel = item.pr_number ? 'View PR #' + item.pr_number + ' ↗' : 'View PR ↗';
      var prActionLink = node('a', 'action-btn primary', prLabel);
      prActionLink.href = item.pr_url || '#';
      prActionLink.setAttribute('href', item.pr_url || '#');
      prActionLink.target = '_blank';
      prActionLink.rel = 'noopener noreferrer';
      els.actions.appendChild(prActionLink);
    } else {
      btn('Commit + PR', true, function () {
        callbacks.sendCommitPrItem(item.id);
      });
    }
    btn('Close item', false, function () {
      callbacks.openCloseModal(item);
    });
    if (!item.archived_at) {
      btn('Archive item', false, function () {
        sendAction(item.id, { action: 'archive', expect_revision: rev })
          .then(ensureActionOk)
          .then(function () {
            showToast('Archived ' + (item.title || item.id), false);
            callbacks.refreshData();
          })
          .catch(function (err) { showToast('Action failed: ' + err.message, true); });
      });
    }
    btn('Reopen to planned', false, function () {
      sendAction(item.id, { action: 'transition', to: 'planned', expect_revision: rev })
        .then(ensureActionOk)
        .then(function () {
          showToast('Reopened to planned', false);
          callbacks.refreshData();
        })
        .catch(function (err) { showToast('Action failed: ' + err.message, true); });
    });
  }

  if (st !== 'closed' && st !== 'cancelled' && st !== 'done') {
    btn('Close item', false, function () {
      callbacks.openCloseModal(item);
    });
  }
  if (st === 'closed' && !item.archived_at) {
    btn('Archive item', false, function () {
      sendAction(item.id, { action: 'archive', expect_revision: rev })
        .then(ensureActionOk)
        .then(function () {
          showToast('Archived ' + (item.title || item.id), false);
          callbacks.refreshData();
        })
        .catch(function (err) { showToast('Action failed: ' + err.message, true); });
    });
  }

  if (item.archived_at) {
    btn('Unarchive item', false, function () {
      sendAction(item.id, { action: 'unarchive', expect_revision: rev })
        .then(ensureActionOk)
        .then(function () {
          showToast('Unarchived ' + (item.title || item.id), false);
          callbacks.refreshData();
        })
        .catch(function (err) { showToast('Action failed: ' + err.message, true); });
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
    addField('blocker', 'Blocker', 'Blocked', 'handoff-pill-blocker', h.blocker, false);
    addField('verification', 'Verification', 'Verified', 'handoff-pill-verification', h.verification, false);
    addField('progress', 'Progress', null, null, h.progress, false);
    addField('next', 'Next steps', null, null, h.next, false);
  } else {
    addField('verification', 'Verification', 'Verified', 'handoff-pill-verification', h.verification, false);
    addField('progress', 'Progress', null, null, h.progress, false);
    addField('next', 'Next steps', null, null, h.next, false);
    addField('blocker', 'Blocker', 'Blocked', 'handoff-pill-blocker', h.blocker, false);
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
  var isTraj = tab === 'trajectory';
  if (els.tabAgentTrajectory) {
    els.tabAgentTrajectory.classList.toggle('active', isTraj);
    if (els.tabAgentTrajectory.setAttribute) {
      els.tabAgentTrajectory.setAttribute('aria-selected', String(isTraj));
      els.tabAgentTrajectory.setAttribute('tabindex', isTraj ? '0' : '-1');
    }
  }
  if (els.tabTaskSpec) {
    els.tabTaskSpec.classList.toggle('active', !isTraj);
    if (els.tabTaskSpec.setAttribute) {
      els.tabTaskSpec.setAttribute('aria-selected', String(!isTraj));
      els.tabTaskSpec.setAttribute('tabindex', isTraj ? '-1' : '0');
    }
  }
  if (isTraj) {
    if (els.proposal) els.proposal.hidden = true;
    if (els.agentDrawer) els.agentDrawer.hidden = false;

    if (state.currentStreamingId === state.selected) return;
    if (state.selected) callbacks.openAgentDrawer(state.selected);
  } else {
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
    if (callbacks.closeAgentDrawer) callbacks.closeAgentDrawer();
    if (els.readerTabs) els.readerTabs.hidden = true;
    if (els.proposal) els.proposal.hidden = true;
    if (els.agentDrawer) els.agentDrawer.hidden = true;
    if (els.detailEmpty) {
      clear(els.detailEmpty);
      var wrap = node('div', 'empty-stack');
      wrap.appendChild(node('strong', '', state.selected ? 'Item not found' : 'Select an item'));
      wrap.appendChild(node('p', '', state.selected
        ? 'This item is no longer in the vault. Pick another item from the list or return to the board.'
        : 'Choose an item from the list to inspect its details, relationships, and activity.'));
      els.detailEmpty.appendChild(wrap);
    }
    return;
  }

  if (els.readerTabs) els.readerTabs.hidden = false;
  if (els.readerTrajectoryBadge) {
    var activeNow = state.activeRuns.has(item.id);
    var waitingNow = activeNow && isRunWaiting(item.id);
    els.readerTrajectoryBadge.hidden = !activeNow;
    text(els.readerTrajectoryBadge, waitingNow ? 'Needs reply' : 'Running');
    if (els.readerTrajectoryBadge.classList) {
      els.readerTrajectoryBadge.classList.toggle('badge-waiting', waitingNow);
    }
  }
  setReaderTab(state.readerTab === 'trajectory' ? 'trajectory' : 'spec');
  var context = [];
  if (item.project) context.push(item.project);
  context.push(itemType(item).toUpperCase());
  context.push(kind(item));
  if (els.context) {
    clear(els.context);
    var ctxText = context.join(' · ');
    if (ctxText) {
      els.context.appendChild(node('span', 'context-meta', ctxText));
      els.context.appendChild(document.createTextNode(' · '));
    }
    var idBadge = node('code', 'context-id-badge', item.id.slice(0, 8));
    idBadge.title = item.id;
    els.context.appendChild(idBadge);

    var copyBtn = node('button', 'btn-copy-id', 'Copy ID');
    copyBtn.type = 'button';
    copyBtn.title = 'Copy item ID to clipboard (' + item.id + ')';
    copyBtn.setAttribute('aria-label', 'Copy ID ' + item.id);
    copyBtn.addEventListener('click', function (e) {
      if (e && typeof e.stopPropagation === 'function') e.stopPropagation();
      try {
        if (typeof navigator !== 'undefined' && navigator.clipboard && navigator.clipboard.writeText) {
          navigator.clipboard.writeText(item.id).then(function () {
            text(copyBtn, 'Copied!');
            setTimeout(function () { text(copyBtn, 'Copy ID'); }, 1500);
          }).catch(function () {
            showToast('Failed to copy ID', true);
          });
        }
      } catch (_) {}
    });
    els.context.appendChild(copyBtn);
  }
  text(els.title, item.title || '(untitled)');

  clear(els.summary);
  addSummary(formatStatusLabel(itemStatus(item)), 'status-badge status-' + itemStatus(item));
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
