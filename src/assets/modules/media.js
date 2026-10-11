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

      uploadScreenshot(img)
        .then(function (res) {
          var md = (res && res.markdown) || ('![screenshot](' + (res && res.path ? res.path : '') + ')');
          if (targetTa.value.indexOf(placeholder) !== -1) {
            targetTa.value = targetTa.value.replace(placeholder, md);
          } else {
            insertSnippetIntoTextarea(targetTa, md);
          }
          showToast('Screenshot pasted', false);
        })
        .catch(function (err) {
          if (targetTa.value.indexOf(placeholder) !== -1) {
            targetTa.value = targetTa.value.replace(placeholder, '');
          }
          showToast(err.message || 'Failed to paste screenshot', true);
        });
    })(images[i], i);
  }
  return true;
}
