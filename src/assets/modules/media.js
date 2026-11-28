import { els } from './dom.js';
import { uploadScreenshot as uploadScreenshotApi } from './api.js';
import { showToast } from './formatters.js';

export function insertSnippetIntoTextarea(textarea, snippet) {
  if (!textarea) return;
  var start = typeof textarea.selectionStart === 'number' ? textarea.selectionStart : textarea.value.length;
  var end = typeof textarea.selectionEnd === 'number' ? textarea.selectionEnd : start;
  var val = textarea.value;
  var before = val.substring(0, start);
  var after = val.substring(end);
  var prefix = '';
  if (before && !before.endsWith('\n\n')) {
    prefix = before.endsWith('\n') ? '\n' : '\n\n';
  }
  var inserted = prefix + snippet;
  textarea.value = before + inserted + after;
  var newPos = before.length + inserted.length;
  textarea.selectionStart = newPos;
  textarea.selectionEnd = newPos;
  if (textarea.focus) textarea.focus();
}

export function getClipboardImages(e) {
  var cd = (e && e.clipboardData) || (typeof window !== 'undefined' && window && window.clipboardData);
  if (!cd) return [];
  var images = [];
  if (cd.items && cd.items.length) {
    for (var i = 0; i < cd.items.length; i++) {
      var item = cd.items[i];
      if (item && item.type && item.type.indexOf('image') !== -1) {
        var f = typeof item.getAsFile === 'function' ? item.getAsFile() : null;
        if (f) images.push(f);
      }
    }
  }
  if (images.length === 0 && cd.files && cd.files.length) {
    for (var j = 0; j < cd.files.length; j++) {
      var file = cd.files[j];
      if (file && file.type && file.type.indexOf('image') !== -1) {
        images.push(file);
      }
    }
  }
  return images;
}

export function uploadScreenshot(file) {
  return uploadScreenshotApi(file).then(function (res) {
    if (!res.ok) {
      return res.json().then(function (err) {
        throw new Error(err.error || ('Failed to upload screenshot (' + res.status + ')'));
      }).catch(function (pErr) {
        throw new Error(pErr.message || ('Failed to upload screenshot (' + res.status + ')'));
      });
    }
    return res.json();
  });
}

function notifyInput(textarea) {
  if (!textarea || typeof textarea.dispatchEvent !== 'function') return;
  try {
    if (typeof Event === 'function') {
      textarea.dispatchEvent(new Event('input', { bubbles: true }));
    }
  } catch (_) {}
}

function replacePlaceholderPreservingSelection(textarea, placeholder, replacement) {
  var val = textarea.value;
  var idx = val.indexOf(placeholder);
  if (idx === -1) return false;
  var selStart = typeof textarea.selectionStart === 'number' ? textarea.selectionStart : val.length;
  var selEnd = typeof textarea.selectionEnd === 'number' ? textarea.selectionEnd : selStart;
  var delta = replacement.length - placeholder.length;
  var phEnd = idx + placeholder.length;
  textarea.value = val.slice(0, idx) + replacement + val.slice(phEnd);
  var nextStart = selStart >= phEnd ? selStart + delta : (selStart > idx ? idx + replacement.length : selStart);
  var nextEnd = selEnd >= phEnd ? selEnd + delta : (selEnd > idx ? idx + replacement.length : selEnd);
  textarea.selectionStart = Math.max(0, nextStart);
  textarea.selectionEnd = Math.max(0, nextEnd);
  notifyInput(textarea);
  return true;
}

export function handleScreenshotPaste(e, textareaOverride) {
  var images = getClipboardImages(e);
  if (!images.length) return false;

  if (e && typeof e.preventDefault === 'function') {
    e.preventDefault();
  }

  var targetTa = textareaOverride || (e && (e.currentTarget || e.target));
  if (!targetTa || typeof targetTa.value !== 'string') {
    var modalOpen = els.specModal && (els.specModal.open || els.specModal.hasAttribute('open'));
    targetTa = modalOpen ? els.specModalBody : els.quickAddBody;
  }
  if (!targetTa) return true;

  for (var i = 0; i < images.length; i++) {
    (function (img, idx) {
      var placeholder = '![Uploading screenshot' + (images.length > 1 ? (' ' + (idx + 1)) : '') + '...]()';
      insertSnippetIntoTextarea(targetTa, placeholder);
      notifyInput(targetTa);

      uploadScreenshot(img)
        .then(function (res) {
          var md = (res && res.markdown) || ('![screenshot](' + (res && res.path ? res.path : '') + ')');
          if (!replacePlaceholderPreservingSelection(targetTa, placeholder, md)) {
            insertSnippetIntoTextarea(targetTa, md);
            notifyInput(targetTa);
          }
          showToast('Screenshot pasted', false);
        })
        .catch(function (err) {
          replacePlaceholderPreservingSelection(targetTa, placeholder, '');
          showToast(err.message || 'Failed to paste screenshot', true);
        });
    })(images[i], i);
  }
  return true;
}
