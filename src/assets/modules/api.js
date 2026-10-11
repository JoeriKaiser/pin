import { BASE } from './config.js';

export function fetchData(etag) {
  var headers = {};
  if (etag) headers['If-None-Match'] = etag;
  return fetch(BASE + 'data.json', {
    credentials: 'same-origin',
    cache: 'no-cache',
    headers: headers
  });
}

export function fetchRuns() {
  return fetch(BASE + 'runs');
}

export function fetchItemLog(id) {
  return fetch(BASE + 'items/' + encodeURIComponent(id) + '/log');
}

export function sendAction(id, payload) {
  return fetch(BASE + 'items/' + encodeURIComponent(id) + '/action', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'X-Pin-Action': 'true'
    },
    body: JSON.stringify(payload)
  });
}

export function sendRun(id, payload) {
  return fetch(BASE + 'items/' + encodeURIComponent(id) + '/run', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'X-Pin-Action': 'true'
    },
    body: JSON.stringify(payload || {})
  });
}

export function sendCommitPr(id) {
  return fetch(BASE + 'items/' + encodeURIComponent(id) + '/commit-pr', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'X-Pin-Action': 'true'
    }
  });
}

export function sendMessage(id, message) {
  return fetch(BASE + 'items/' + encodeURIComponent(id) + '/message', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'X-Pin-Action': 'true'
    },
    body: JSON.stringify({ message: message })
  });
}

export function sendFinish(id) {
  return fetch(BASE + 'items/' + encodeURIComponent(id) + '/finish', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'X-Pin-Action': 'true'
    }
  });
}

export function sendCancel(id) {
  return fetch(BASE + 'items/' + encodeURIComponent(id) + '/cancel', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'X-Pin-Action': 'true'
    }
  });
}

export function createItem(payload) {
  return fetch(BASE + 'items', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'X-Pin-Action': 'true'
    },
    body: JSON.stringify(payload)
  });
}

export function uploadScreenshot(file) {
  return fetch(BASE + 'screenshots', {
    method: 'POST',
    headers: {
      'Content-Type': (file && file.type) || 'image/png',
      'X-Pin-Action': 'true'
    },
    body: file
  });
}

export function fetchUpdate(force) {
  return fetch(BASE + 'update' + (force ? '?force=true' : ''));
}

export function sendUpdate(payload) {
  return fetch(BASE + 'update', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'X-Pin-Action': 'true'
    },
    body: JSON.stringify(payload || {})
  });
}
