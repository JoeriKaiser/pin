import { state, find } from './state.js';
import { els } from './dom.js';
import { text, showToast } from './formatters.js';

var SOUND_KEY = 'pin-alerts-sound';
var DESKTOP_KEY = 'pin-alerts-desktop';

var notifiedKeys = new Set();
var initialSyncDone = false;
var callbacks = {
  openReplyForItem: function () {},
  openAgentDrawer: function () {},
  onWaitingUpdated: function () {},
  renderBoard: function () {},
  renderDetail: function () {},
  renderAgentsTable: function () {}
};

export function setAlertCallbacks(cbs) {
  if (cbs) Object.assign(callbacks, cbs);
}

function readStorage(key, fallback) {
  try {
    if (typeof localStorage !== 'undefined' && localStorage && typeof localStorage.getItem === 'function') {
      var v = localStorage.getItem(key);
      if (v != null) return v;
    }
  } catch (_) {}
  return fallback;
}

function writeStorage(key, val) {
  try {
    if (typeof localStorage !== 'undefined' && localStorage && typeof localStorage.setItem === 'function') {
      localStorage.setItem(key, String(val));
    }
  } catch (_) {}
}

export function getAlertPrefs() {
  var sound = readStorage(SOUND_KEY, 'true') !== 'false';
  var desktop = readStorage(DESKTOP_KEY, 'true') !== 'false';
  return { sound: sound, desktop: desktop };
}

export function setAlertPrefs(prefs) {
  if (prefs && typeof prefs.sound === 'boolean') {
    writeStorage(SOUND_KEY, prefs.sound ? 'true' : 'false');
  }
  if (prefs && typeof prefs.desktop === 'boolean') {
    writeStorage(DESKTOP_KEY, prefs.desktop ? 'true' : 'false');
  }
  renderAlertsButton();
  renderAlertsDialog();
}

export function getWaitingRuns() {
  var list = (Array.isArray(state.activeRunsInfo) ? state.activeRunsInfo : []).filter(function (run) {
    return !!(run && run.waiting_for_user);
  });
  list.sort(function (a, b) {
    var wa = a.waiting_since || a.started_at || 0;
    var wb = b.waiting_since || b.started_at || 0;
    if (wa !== wb) return wa - wb;
    return String(a.item_id || '').localeCompare(String(b.item_id || ''));
  });
  return list;
}

export function summarizeQuestionPreview(raw, maxLen) {
  var limit = maxLen || 120;
  if (!raw) return 'Waiting for your reply';
  var compact = String(raw)
    .replace(/```[\s\S]*?```/g, ' [code] ')
    .replace(/`([^`]+)`/g, '$1')
    .replace(/[#*_>~]+/g, '')
    .replace(/\s+/g, ' ')
    .trim();
  if (!compact) return 'Waiting for your reply';
  if (compact.length <= limit) return compact;
  return compact.slice(0, limit - 1).trim() + '…';
}

function buildFaviconHref(waitingCount) {
  var svg = waitingCount > 0
    ? '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><rect width="32" height="32" rx="8" fill="#d97706"/><circle cx="16" cy="16" r="7" fill="#ffffff"/></svg>'
    : '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><rect width="32" height="32" rx="8" fill="#4f46e5"/><circle cx="16" cy="16" r="6" fill="#ffffff"/></svg>';
  return 'data:image/svg+xml,' + encodeURIComponent(svg);
}

export function updateDocumentAttention(waitingCount) {
  if (typeof document === 'undefined') return;
  document.title = waitingCount > 0
    ? '(' + waitingCount + ') Needs reply - pin'
    : 'pin';
  var iconEl = document.querySelector && document.querySelector('link[rel="icon"]');
  if (iconEl && typeof iconEl.setAttribute === 'function') {
    iconEl.setAttribute('href', buildFaviconHref(waitingCount));
  }
}

export function playAlertChime() {
  try {
    if (typeof navigator !== 'undefined' && navigator.userActivation && !navigator.userActivation.hasBeenActive) {
      return false;
    }
    var AudioCtx = (typeof window !== 'undefined' && (window.AudioContext || window.webkitAudioContext)) || null;
    if (!AudioCtx) return false;
    var ctx = new AudioCtx();
    var now = ctx.currentTime;
    var notes = [
      { freq: 659.25, start: 0, dur: 0.14 },
      { freq: 880.0, start: 0.12, dur: 0.22 }
    ];
    notes.forEach(function (n) {
      var osc = ctx.createOscillator();
      var gain = ctx.createGain();
      osc.type = 'sine';
      osc.frequency.setValueAtTime(n.freq, now + n.start);
      gain.gain.setValueAtTime(0.001, now + n.start);
      gain.gain.linearRampToValueAtTime(0.08, now + n.start + 0.02);
      gain.gain.exponentialRampToValueAtTime(0.001, now + n.start + n.dur);
      osc.connect(gain);
      gain.connect(ctx.destination);
      osc.start(now + n.start);
      osc.stop(now + n.start + n.dur + 0.02);
    });
    if (typeof ctx.close === 'function') {
      setTimeout(function () { try { ctx.close(); } catch (_) {} }, 600);
    }
    return true;
  } catch (_) {
    return false;
  }
}

function getNotificationApi() {
  if (typeof window !== 'undefined' && typeof window.Notification === 'function') {
    return window.Notification;
  }
  if (typeof Notification === 'function') {
    return Notification;
  }
  return null;
}

export function sendDesktopNotification(itemId, title, message) {
  var Notif = getNotificationApi();
  if (!Notif || Notif.permission !== 'granted') return null;
  var body = summarizeQuestionPreview(message, 160);
  try {
    var n = new Notif('Needs reply: ' + (title || itemId), {
      body: body,
      tag: 'pin-waiting-' + itemId,
      renotify: true
    });
    n.onclick = function () {
      try {
        if (typeof window !== 'undefined' && typeof window.focus === 'function') {
          window.focus();
        }
      } catch (_) {}
      try { if (typeof n.close === 'function') n.close(); } catch (_) {}
      callbacks.openReplyForItem(itemId);
    };
    return n;
  } catch (_) {
    return null;
  }
}

export function notifyRunWaiting(run, fallbackTitle) {
  if (!run || !run.item_id) return false;
  var key = run.item_id + ':' + (run.waiting_since || 0);
  if (notifiedKeys.has(key)) return false;
  notifiedKeys.add(key);

  var prefs = getAlertPrefs();
  var itm = find(run.item_id);
  var title = (itm && itm.title) || fallbackTitle || run.item_id.slice(0, 8);
  var isViewingThisDrawer = state.viewMode === 'detail' &&
    state.readerTab === 'trajectory' &&
    state.currentStreamingId === run.item_id &&
    typeof document !== 'undefined' && !document.hidden;

  if (prefs.sound && !isViewingThisDrawer) {
    playAlertChime();
  }
  if (prefs.desktop && (!isViewingThisDrawer || (typeof document !== 'undefined' && document.hidden))) {
    sendDesktopNotification(run.item_id, title, run.last_message);
  }
  if (els.alertAnnouncer) {
    text(els.alertAnnouncer, title + ' is waiting for your reply.');
  }
  return true;
}

export function renderAlertsButton() {
  if (!els.btnAlertsToggle) return;
  var prefs = getAlertPrefs();
  var Notif = getNotificationApi();
  var desktopReady = !!(prefs.desktop && Notif && Notif.permission === 'granted');
  var anyEnabled = prefs.sound || desktopReady;
  els.btnAlertsToggle.classList.toggle('alerts-active', anyEnabled);
  els.btnAlertsToggle.setAttribute(
    'aria-label',
    anyEnabled ? 'Alerts (enabled)' : 'Alerts (muted)'
  );
  els.btnAlertsToggle.title = desktopReady
    ? 'Alerts: desktop notifications + ' + (prefs.sound ? 'sound' : 'silent')
    : (prefs.sound ? 'Alerts: sound on (click to enable desktop notifications)' : 'Alerts muted');
}

export function renderAlertsDialog() {
  var prefs = getAlertPrefs();
  var Notif = getNotificationApi();
  if (els.alertsSoundCheckbox) {
    els.alertsSoundCheckbox.checked = prefs.sound;
  }
  if (els.alertsDesktopCheckbox) {
    els.alertsDesktopCheckbox.checked = prefs.desktop;
  }
  if (els.alertsDesktopStatus) {
    if (!Notif) {
      text(els.alertsDesktopStatus, 'Browser desktop notifications are unavailable in this environment.');
    } else if (Notif.permission === 'granted') {
      text(els.alertsDesktopStatus, 'Permission granted. Alerts appear even when pin is in a background tab.');
    } else if (Notif.permission === 'denied') {
      text(els.alertsDesktopStatus, 'Blocked by browser settings. Allow notifications for localhost to enable.');
    } else {
      text(els.alertsDesktopStatus, 'Click "Enable Desktop Alerts" to grant browser notification permission.');
    }
  }
  if (els.btnAlertsRequestPerm) {
    els.btnAlertsRequestPerm.hidden = !Notif || Notif.permission === 'granted';
  }
}

export function openAlertsDialog() {
  renderAlertsDialog();
  if (els.alertsDialog) {
    if (els.alertsDialog.showModal) els.alertsDialog.showModal();
    else els.alertsDialog.setAttribute('open', '');
  }
}

export function closeAlertsDialog() {
  if (els.alertsDialog) {
    if (els.alertsDialog.close) els.alertsDialog.close();
    else els.alertsDialog.removeAttribute('open');
  }
}

export function requestDesktopPermission() {
  var Notif = getNotificationApi();
  if (!Notif || typeof Notif.requestPermission !== 'function') {
    showToast('Desktop notifications are not supported by this browser', true);
    return Promise.resolve('unsupported');
  }
  return Promise.resolve(Notif.requestPermission()).then(function (perm) {
    if (perm === 'granted') {
      setAlertPrefs({ desktop: true });
      showToast('Desktop alerts enabled', false);
    } else {
      renderAlertsDialog();
      renderAlertsButton();
      showToast('Desktop notification permission was not granted', true);
    }
    return perm;
  }).catch(function () {
    return 'denied';
  });
}

export function renderNeedsReplyControls(waitingRuns) {
  var count = waitingRuns.length;
  if (els.btnNeedsReply) {
    els.btnNeedsReply.hidden = count === 0;
    if (els.needsReplyCount) {
      text(els.needsReplyCount, count === 1 ? '1 needs reply' : (count + ' need reply'));
    }
  }
  if (els.agentsCountBadge) {
    els.agentsCountBadge.classList.toggle('badge-waiting', count > 0);
  }
  if (!els.needsReplyBanner) return;

  var first = waitingRuns[0] || null;
  var viewingOnlyWaiting = count === 1 &&
    first &&
    state.viewMode === 'detail' &&
    state.readerTab === 'trajectory' &&
    state.currentStreamingId === first.item_id;

  if (!first || viewingOnlyWaiting) {
    els.needsReplyBanner.hidden = true;
    return;
  }

  els.needsReplyBanner.hidden = false;
  els.needsReplyBanner.dataset.itemId = first.item_id;
  var itm = find(first.item_id);
  var title = (itm && itm.title) || first.item_id.slice(0, 8);
  if (els.needsReplyBannerTitle) text(els.needsReplyBannerTitle, title);
  if (els.needsReplyBannerPreview) {
    text(els.needsReplyBannerPreview, summarizeQuestionPreview(first.last_message, 130));
  }
  if (els.needsReplyBannerMore) {
    if (count > 1) {
      els.needsReplyBannerMore.hidden = false;
      text(els.needsReplyBannerMore, '+' + (count - 1) + ' more');
    } else {
      els.needsReplyBannerMore.hidden = true;
      text(els.needsReplyBannerMore, '');
    }
  }
}

export function syncWaitingAlerts(options) {
  var opts = options || {};
  var waitingRuns = getWaitingRuns();
  state.waitingRuns = new Set(waitingRuns.map(function (r) { return r.item_id; }));

  // Prune notified keys for items no longer waiting so a future turn re-alerts.
  var activeKeys = new Set(waitingRuns.map(function (r) {
    return r.item_id + ':' + (r.waiting_since || 0);
  }));
  Array.from(notifiedKeys).forEach(function (k) {
    if (!activeKeys.has(k)) notifiedKeys.delete(k);
  });

  if (opts.notify) {
    if (!initialSyncDone) {
      waitingRuns.forEach(function (run) {
        notifiedKeys.add(run.item_id + ':' + (run.waiting_since || 0));
      });
    } else {
      waitingRuns.forEach(function (run) {
        notifyRunWaiting(run);
      });
    }
  }
  initialSyncDone = true;

  updateDocumentAttention(waitingRuns.length);
  renderNeedsReplyControls(waitingRuns);
  renderAlertsButton();
  if (callbacks.onWaitingUpdated) callbacks.onWaitingUpdated(waitingRuns);
  return waitingRuns;
}

export function handleGlobalWaitingEvent(msg) {
  if (!msg || !msg.id) return;
  var runs = Array.isArray(state.activeRunsInfo) ? state.activeRunsInfo : [];
  var existing = null;
  for (var i = 0; i < runs.length; i++) {
    if (runs[i].item_id === msg.id) {
      existing = runs[i];
      break;
    }
  }
  if (msg.type === 'run_waiting') {
    if (existing) {
      existing.waiting_for_user = true;
      existing.interactive = true;
      existing.waiting_since = msg.waiting_since || Math.floor(Date.now() / 1000);
      if (msg.message) existing.last_message = msg.message;
      existing.last_activity = 'Waiting for reply';
    } else {
      existing = {
        item_id: msg.id,
        mode: msg.mode || 'idea',
        interactive: true,
        waiting_for_user: true,
        waiting_since: msg.waiting_since || Math.floor(Date.now() / 1000),
        last_message: msg.message || null,
        last_activity: 'Waiting for reply',
        is_primary_worktree: false,
        worktree_path: null,
        started_at: Math.floor(Date.now() / 1000)
      };
      runs.push(existing);
      state.activeRunsInfo = runs;
    }
    if (state.activeRuns) state.activeRuns.add(msg.id);
    notifyRunWaiting(existing, msg.title);
    syncWaitingAlerts({ notify: false });
    callbacks.renderBoard();
    callbacks.renderDetail();
    if (state.viewMode === 'agents') callbacks.renderAgentsTable();
  } else if (msg.type === 'run_resumed') {
    if (existing) {
      existing.waiting_for_user = false;
      existing.waiting_since = null;
      existing.last_activity = 'Running';
    }
    syncWaitingAlerts({ notify: false });
    callbacks.renderBoard();
    callbacks.renderDetail();
    if (state.viewMode === 'agents') callbacks.renderAgentsTable();
  }
}

export function bindAlertsUi() {
  renderAlertsButton();
  if (els.btnNeedsReply) {
    els.btnNeedsReply.addEventListener('click', function () {
      var waiting = getWaitingRuns();
      if (waiting.length > 0) {
        callbacks.openReplyForItem(waiting[0].item_id);
      }
    });
  }
  if (els.btnNeedsReplyBannerAction) {
    els.btnNeedsReplyBannerAction.addEventListener('click', function () {
      var id = els.needsReplyBanner && els.needsReplyBanner.dataset && els.needsReplyBanner.dataset.itemId;
      var waiting = getWaitingRuns();
      var targetId = id || (waiting[0] && waiting[0].item_id);
      if (targetId) callbacks.openReplyForItem(targetId);
    });
  }
  if (els.btnNeedsReplyBannerView) {
    els.btnNeedsReplyBannerView.addEventListener('click', function () {
      var id = els.needsReplyBanner && els.needsReplyBanner.dataset && els.needsReplyBanner.dataset.itemId;
      var waiting = getWaitingRuns();
      var targetId = id || (waiting[0] && waiting[0].item_id);
      if (targetId) callbacks.openAgentDrawer(targetId);
    });
  }
  if (els.btnAlertsToggle) {
    els.btnAlertsToggle.addEventListener('click', openAlertsDialog);
  }
  if (els.btnAlertsClose) {
    els.btnAlertsClose.addEventListener('click', closeAlertsDialog);
  }
  if (els.btnAlertsDone) {
    els.btnAlertsDone.addEventListener('click', closeAlertsDialog);
  }
  if (els.alertsSoundCheckbox) {
    els.alertsSoundCheckbox.addEventListener('change', function () {
      setAlertPrefs({ sound: !!els.alertsSoundCheckbox.checked });
    });
  }
  if (els.alertsDesktopCheckbox) {
    els.alertsDesktopCheckbox.addEventListener('change', function () {
      var want = !!els.alertsDesktopCheckbox.checked;
      setAlertPrefs({ desktop: want });
      var Notif = getNotificationApi();
      if (want && Notif && Notif.permission === 'default') {
        requestDesktopPermission();
      }
    });
  }
  if (els.btnAlertsRequestPerm) {
    els.btnAlertsRequestPerm.addEventListener('click', requestDesktopPermission);
  }
  if (els.btnAlertsTest) {
    els.btnAlertsTest.addEventListener('click', function () {
      var prefs = getAlertPrefs();
      if (prefs.sound) playAlertChime();
      if (prefs.desktop) {
        sendDesktopNotification('test', 'Interactive prompt test', 'This is how pin alerts you when an agent pauses for your reply.');
      }
      showToast('Test alert triggered', false);
    });
  }
}

export function resetAlertStateForTests() {
  notifiedKeys.clear();
  initialSyncDone = true;
}
