import { state } from './state.js';
import { els } from './dom.js';
import { isNarrow } from './formatters.js';

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
  var match = hash.match(/^#\/(?:idea|item)\/(.+)$/);
  if (!match) return null;
  try {
    return decodeURIComponent(match[1]);
  } catch (_) {
    return null;
  }
}

export function applyRoute() {
  state.selected = route();
  if (onRouteChanged) {
    onRouteChanged(state.selected);
  }
}

export function setRoute(id) {
  state.selected = id;
  var next = id ? '#/item/' + encodeURIComponent(id) : '#/';
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
