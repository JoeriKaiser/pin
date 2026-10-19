import { els } from './dom.js';

export function text(node, value) {
  if (node) node.textContent = value == null ? '' : String(value);
}

export function clear(node) {
  if (!node) return;
  if (typeof node.replaceChildren === 'function') {
    node.replaceChildren();
  } else {
    node.textContent = '';
  }
}

export function node(tag, className, value) {
  var n = document.createElement(tag);
  if (className) n.className = className;
  if (value != null) text(n, value);
  return n;
}

export function date(value, long) {
  var n = Number(value);
  if (!isFinite(n)) return '';
  if (n < 1e12) n *= 1000;
  var opts = long
    ? { year: 'numeric', month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }
    : { month: 'short', day: 'numeric' };
  try {
    return new Date(n).toLocaleDateString(undefined, opts);
  } catch (_) {
    return new Date(n).toISOString();
  }
}

export function timeAgo(value) {
  var n = Number(value);
  if (!isFinite(n)) return '';
  if (n < 1e12) n *= 1000;
  var diffSec = Math.floor((Date.now() - n) / 1000);
  if (diffSec < 60) return 'just now';
  if (diffSec < 3600) return Math.floor(diffSec / 60) + 'm ago';
  if (diffSec < 86400) return Math.floor(diffSec / 3600) + 'h ago';
  if (diffSec < 86400 * 30) return Math.floor(diffSec / 86400) + 'd ago';
  return date(value, false);
}

export function escapeHtml(str) {
  return String(str == null ? '' : str)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

export function isNarrow() {
  return (typeof window !== 'undefined' && window && window.matchMedia)
    ? window.matchMedia('(max-width: 799.98px)').matches
    : false;
}

var toastTimer = null;
export function showToast(message, isError) {
  if (!els.toast) return;
  clearTimeout(toastTimer);
  text(els.toast, message);
  els.toast.hidden = false;
  if (els.toast.classList) {
    els.toast.classList.toggle('toast-error', !!isError);
    els.toast.classList.toggle('toast-success', !isError);
  }
  toastTimer = setTimeout(function () {
    els.toast.hidden = true;
  }, 3500);
}

export function showDialogError(message) {
  if (!els.dialogError) return;
  if (message) {
    text(els.dialogError, message);
    els.dialogError.hidden = false;
  } else {
    text(els.dialogError, '');
    els.dialogError.hidden = true;
  }
}

export function stripAnsi(str) {
  if (!str) return '';
  return String(str).replace(/\x1b\[[0-9;]*[a-zA-Z]|\u001b\[[0-9;]*[a-zA-Z]/g, '');
}

export function cleanThoughtText(thoughtText) {
  if (!thoughtText) return '';
  var str = typeof thoughtText === 'string' ? thoughtText : String(thoughtText);
  return stripAnsi(str)
    .replace(/<(?:\/)?(?:think|thought|thinking)(?:\s+[^>]*)?>/gi, '')
    .replace(/\r\n?/g, '\n')
    .replace(/[ \t]+$/gm, '')
    .replace(/\n{3,}/g, '\n\n');
}
