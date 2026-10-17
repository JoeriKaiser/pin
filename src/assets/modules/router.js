import { state } from './state.js';
import { els } from './dom.js';
import { isNarrow } from './formatters.js';
import { setViewMode } from './board.js';
var onRouteChanged = null;

export function setOnRouteChanged(fn) {
  onRouteChanged = fn;
}

export function route() {
  var hash = '';
  try {
    if (typeof location !== 'undefined' && location && location.hash) {
      hash = location.hash;
    }
  } catch (_) {}
  if (!hash) return state ? state.selected : null;
  if (hash === '#/agents') return null;
  var match = hash.match(/^#\/(?:idea|item)\/(.+)$/);
  if (!match) return null;
  try {
    return decodeURIComponent(match[1]);
  } catch (_) {
    return null;
  }
}

export function applyRoute() {
  var hash = '';
  try {
    if (typeof location !== 'undefined' && location && location.hash) {
      hash = location.hash;
    }
  } catch (_) {}

  if (hash === '#/agents') {
    state.selected = null;
    setViewMode('agents');
  } else {
    state.selected = route();
    if (state.selected) {
      if (state.viewMode !== 'detail') {
        setViewMode('detail');
      }
    } else if (state.viewMode === 'agents') {
      setViewMode('board');
    }
  }
  if (onRouteChanged) {
    onRouteChanged(state.selected);
  }
}

export function setRoute(id) {
  var next;
  if (id === 'agents') {
    state.selected = null;
    next = '#/agents';
  } else if (id) {
    state.selected = id;
    next = '#/item/' + encodeURIComponent(id);
  } else {
    state.selected = null;
    next = '#/';
  }
  try {
    if (typeof location !== 'undefined' && location) {
      if (location.hash !== next) {
        location.hash = next;
      } else {
        applyRoute();
      }
      return;
    }
  } catch (_) {}
  applyRoute();
}
