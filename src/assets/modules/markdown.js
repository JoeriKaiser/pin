import { BASE } from './config.js';
import { norm } from './state.js';
import { els } from './dom.js';
import { text, escapeHtml } from './formatters.js';

export var PURIFY_CONFIG = {
  ALLOWED_TAGS: [
    'p', 'br', 'strong', 'em', 'b', 'i', 'code', 'pre', 'blockquote',
    'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'ul', 'ol', 'li', 'a', 'hr',
    'table', 'thead', 'tbody', 'tr', 'th', 'td', 'del', 'ins', 'sub', 'sup', 'input'
  ],
  ALLOWED_ATTR: ['href', 'title', 'class', 'type', 'checked', 'disabled', 'target', 'rel'],
  ALLOWED_URI_REGEXP: /^(?:(?:https?|mailto):|[^a-z]|[a-z+.\-]+(?:[^a-z+.\-:]|$))/i,
  FORBID_TAGS: [
    'script', 'iframe', 'object', 'embed', 'form', 'button', 'textarea',
    'select', 'style', 'link', 'meta', 'base', 'img', 'svg', 'math'
  ],
  FORBID_ATTR: ['style', 'src', 'srcdoc', 'onerror', 'onclick', 'onload', 'onfocus', 'onblur', 'onsubmit'],
  KEEP_CONTENT: true
};

export function bodyWithoutDuplicateTitle(body, title) {
  var source = String(body || '');
  var match = source.match(/^\s*#\s+(.+?)\s*(?:\n|$)/);
  return match && norm(match[1].replace(/[*_`]/g, '').trim()) === norm(title).trim()
    ? source.slice(match[0].length).replace(/^\s+/, '')
    : source;
}

export var markedRenderer = null;
if (typeof marked !== 'undefined' && marked && typeof marked.Renderer === 'function') {
  try {
    markedRenderer = new marked.Renderer();
    markedRenderer.listitem = function (item, task, checked) {
      var textStr = '';
      var isTask = false;
      var isChecked = false;
      if (item && typeof item === 'object') {
        isTask = !!item.task;
        isChecked = !!item.checked;
        if (item.tokens) {
          var filteredTokens = item.tokens.filter(function (t) { return t.type !== 'checkbox'; });
          textStr = this.parser ? this.parser.parse(filteredTokens) : (item.text || '');
        } else {
          textStr = item.text || '';
        }
      } else {
        textStr = String(item || '');
        isTask = !!task;
        isChecked = !!checked;
      }

      if (isTask || /^\[[ xX]\]\s*/.test(textStr)) {
        isChecked = isChecked || /^\[[xX]\]\s*/.test(textStr);
        var cleanText = textStr.replace(/^\[[ xX]\]\s*/, '');
        return '<li class="task-list-item"><input type="checkbox" disabled ' + (isChecked ? 'checked ' : '') + '/> ' + cleanText + '</li>';
      }
      return '<li>' + textStr + '</li>';
    };
    markedRenderer.image = function (token, title, text) {
      var src = (typeof token === 'object' && token) ? (token.href || '') : (token || '');
      var label = (typeof token === 'object' && token) ? (token.text || '') : (text || '');
      var filename = src.split('/').pop() || src;
      var viewUrl = BASE + 'screenshots/' + encodeURIComponent(filename);
      return '<p class="screenshot-ref">📷 <a href="' + escapeHtml(viewUrl) + '" target="_blank" rel="noopener"><code>' + escapeHtml(src || label || 'screenshot') + '</code></a></p>';
    };
    marked.use({ renderer: markedRenderer, gfm: true });
  } catch (_) {}
}

export function sanitizeHtml(dirty) {
  if (!dirty) return '';
  if (typeof DOMPurify !== 'undefined' && DOMPurify) {
    if (typeof DOMPurify.sanitize === 'function') {
      return DOMPurify.sanitize(dirty, PURIFY_CONFIG);
    }
    if (typeof DOMPurify === 'function') {
      try {
        var instance = DOMPurify(window);
        if (instance && typeof instance.sanitize === 'function') {
          return instance.sanitize(dirty, PURIFY_CONFIG);
        }
      } catch (_) {}
    }
  }
  return dirty;
}

export function renderMarkdownSnippet(raw, container) {
  if (!raw || !container) return;
  var trimmed = String(raw).trim();
  if (!trimmed) return;
  if (typeof marked !== 'undefined' && marked) {
    try {
      var dirty = marked.parse(trimmed, { renderer: markedRenderer, gfm: true, async: false });
      var clean = sanitizeHtml(dirty);
      container.innerHTML = clean;
      if (!container.textContent && !container.childNodes.length) {
        container.textContent = trimmed;
      }
      return;
    } catch (_) {}
  }
  container.textContent = trimmed;
}

export function renderMarkdown(item) {
  if (!els.body || !item) return;
  var source = bodyWithoutDuplicateTitle(item.content || item.body, item.title), dirty;
  try {
    dirty = marked.parse(source, { renderer: markedRenderer, gfm: true, async: false });
  } catch (_) {
    text(els.body, source);
    return;
  }
  try {
    els.body.innerHTML = sanitizeHtml(dirty);
  } catch (_) {
    text(els.body, source);
  }
}
