#!/usr/bin/env node
'use strict';

/**
 * tests/viewer_trajectory.js
 *
 * Verification harness for pin view's frontend modules, trajectory drawer,
 * ACP streaming events, card drag and drop, and run dispatching.
 */

const fs = require('fs');
const path = require('path');
const assert = require('assert');
const vm = require('vm');

const rootDir = path.resolve(__dirname, '..');
const appJsPath = path.resolve(rootDir, 'src/assets/app.js');
const testHooksPath = path.resolve(rootDir, 'src/assets/modules/test-hooks.js');

if (!fs.existsSync(appJsPath)) {
  console.error(`Error: app.js not found at ${appJsPath}`);
  process.exit(1);
}

// 1. Set up lightweight Mock DOM environment
function createMockElement(id, tagName = 'div') {
  const el = {
    id: id || '',
    tagName: tagName.toUpperCase(),
    className: '',
    _classes: new Set(),
    get classList() {
      const self = this;
      return {
        add(...cls) {
          cls.forEach(c => {
            if (c) {
              self._classes.add(c);
              self.className = Array.from(self._classes).join(' ');
            }
          });
        },
        remove(...cls) {
          cls.forEach(c => {
            self._classes.delete(c);
            self.className = Array.from(self._classes).join(' ');
          });
        },
        contains(c) {
          return self._classes.has(c) || (self.className || '').split(/\s+/).includes(c);
        },
        toggle(c, force) {
          if (force === undefined) {
            if (this.contains(c)) { this.remove(c); return false; }
            else { this.add(c); return true; }
          }
          if (force) { this.add(c); return true; }
          this.remove(c); return false;
        }
      };
    },
    dataset: {},
    children: [],
    childNodes: [],
    style: {},
    hidden: false,
    open: false,
    disabled: false,
    value: '',
    scrollTop: 0,
    scrollHeight: 0,
    attributes: {},
    setAttribute(k, v) { this.attributes[k] = String(v); },
    getAttribute(k) { return this.attributes[k] || null; },
    removeAttribute(k) { delete this.attributes[k]; },
    hasAttribute(k) { return k in this.attributes; },
    showModal() { this.open = true; },
    close() { this.open = false; },
    appendChild(child) {
      if (!child) return child;
      this.children.push(child);
      this.childNodes.push(child);
      child.parentNode = this;
      return child;
    },
    removeChild(child) {
      const i = this.children.indexOf(child);
      if (i !== -1) this.children.splice(i, 1);
      const j = this.childNodes.indexOf(child);
      if (j !== -1) this.childNodes.splice(j, 1);
      child.parentNode = null;
      return child;
    },
    replaceChildren(...nodes) {
      this.children = [];
      this.childNodes = [];
      nodes.forEach(n => this.appendChild(n));
    },
    querySelector(sel) {
      return findDescendant(this, sel);
    },
    querySelectorAll(sel) {
      return findAllDescendants(this, sel);
    },
    addEventListener(event, handler) {
      if (!this._listeners) this._listeners = {};
      if (!this._listeners[event]) this._listeners[event] = [];
      this._listeners[event].push(handler);
    },
    removeEventListener(event, handler) {
      if (!this._listeners || !this._listeners[event]) return;
      const idx = this._listeners[event].indexOf(handler);
      if (idx !== -1) this._listeners[event].splice(idx, 1);
    },
    dispatchEvent(event) {
      const evType = (event && event.type) || event;
      const evObj = typeof event === 'object' ? event : { type: evType, target: this };
      if (this._listeners && this._listeners[evType]) {
        this._listeners[evType].forEach(h => h.call(this, evObj));
      }
      return true;
    },
    click() {
      this.dispatchEvent({
        type: 'click',
        target: this,
        stopPropagation() {},
        preventDefault() {}
      });
    },
    focus() {},
    blur() {}
  };

  let _textContent = '';
  Object.defineProperty(el, 'textContent', {
    get() {
      if (this.childNodes.length > 0) {
        return this.childNodes.map(c => c.textContent || '').join('');
      }
      return _textContent;
    },
    set(val) {
      _textContent = String(val);
      this.children = [];
      this.childNodes = [];
    }
  });

  let _innerHTML = '';
  Object.defineProperty(el, 'innerHTML', {
    get() { return _innerHTML; },
    set(val) {
      _innerHTML = String(val);
      if (val === '') {
        this.children = [];
        this.childNodes = [];
      }
    }
  });

  return el;
}

function matchesSelector(el, sel) {
  if (!el || !el.tagName) return false;
  if (sel.includes('[') && sel.includes(']')) {
    const m = sel.match(/\[([a-zA-Z0-9_-]+)(?:="([^"]*)")?\]/);
    if (m) {
      const attr = m[1];
      const expectedVal = m[2];
      const baseSel = sel.split('[')[0];
      const baseOk = !baseSel || matchesSelector(el, baseSel);
      let actualVal = (el.attributes && el.attributes[attr]) || (el.getAttribute ? el.getAttribute(attr) : null);
      if (attr.startsWith('data-')) {
        const camel = attr.slice(5).replace(/-([a-z])/g, (_, l) => l.toUpperCase());
        if (el.dataset && el.dataset[camel] !== undefined) {
          actualVal = el.dataset[camel];
        }
      }
      const attrOk = expectedVal !== undefined ? actualVal === expectedVal : actualVal != null;
      return baseOk && attrOk;
    }
  }
  if (sel.startsWith('.')) {
    const cls = sel.slice(1);
    const classes = (el.className || '').split(/\s+/).filter(Boolean);
    return classes.includes(cls) || (el._classes && el._classes.has(cls));
  }
  if (sel.startsWith('#')) {
    return el.id === sel.slice(1);
  }
  return el.tagName.toLowerCase() === sel.toLowerCase();
}

function findDescendant(root, sel) {
  for (const child of root.children || []) {
    if (matchesSelector(child, sel)) return child;
    const found = findDescendant(child, sel);
    if (found) return found;
  }
  return null;
}

function findAllDescendants(root, sel) {
  const result = [];
  for (const child of root.children || []) {
    if (matchesSelector(child, sel)) result.push(child);
    result.push(...findAllDescendants(child, sel));
  }
  return result;
}

const elementsById = new Map();
function getOrCreateElement(id) {
  if (!elementsById.has(id)) {
    elementsById.set(id, createMockElement(id));
  }
  return elementsById.get(id);
}
const elementsBySelector = new Map();
function getOrCreateBySelector(sel) {
  if (!elementsBySelector.has(sel)) {
    elementsBySelector.set(sel, createMockElement('', 'div'));
  }
  return elementsBySelector.get(sel);
}

const documentMock = {
  readyState: 'loading',
  documentElement: {
    dataset: { base: '' }
  },
  body: createMockElement('body', 'body'),
  getElementById(id) {
    return getOrCreateElement(id);
  },
  createElement(tag) {
    return createMockElement('', tag);
  },
  createElementNS(ns, tag) {
    return createMockElement('', tag);
  },
  createTextNode(text) {
    return {
      nodeType: 3,
      textContent: String(text),
      parentNode: null
    };
  },
  querySelector(sel) {
    if (sel.startsWith('#')) return getOrCreateElement(sel.slice(1));
    return getOrCreateBySelector(sel);
  },
  querySelectorAll(sel) {
    if (sel.startsWith('#')) return [getOrCreateElement(sel.slice(1))];
    return [getOrCreateBySelector(sel)];
  },
  addEventListener() {},
  removeEventListener() {}
};

const localStorageMock = {
  _data: {},
  getItem(k) { return this._data[k] || null; },
  setItem(k, v) { this._data[k] = String(v); },
  removeItem(k) { delete this._data[k]; },
  clear() { this._data = {}; }
};

let fetchStub = function () {
  return Promise.resolve({
    ok: true,
    status: 200,
    headers: { get: () => null },
    json: () => Promise.resolve([])
  });
};

const mockWindow = {
  document: documentMock,
  DOMPurify: { sanitize: (s) => s },
  marked: { parse: (s) => s, Renderer: function () {} },
  EventSource: function (url) {
    this.url = url;
    this.addEventListener = () => {};
    this.close = () => {};
  },
  fetch: function (...args) {
    return fetchStub.apply(this, args);
  },
  navigator: { clipboard: { writeText: () => Promise.resolve() } },
  requestAnimationFrame: (cb) => setTimeout(cb, 0),
  cancelAnimationFrame: (id) => clearTimeout(id),
  setInterval: () => 123,
  clearInterval: () => {},
  setTimeout: () => 456,
  clearTimeout: () => {},
  addEventListener: () => {},
  removeEventListener: () => {},
  matchMedia: () => ({ matches: false, addEventListener: () => {}, removeEventListener: () => {} }),
  localStorage: localStorageMock,
  location: { hash: '', pathname: '/' },
  history: { replaceState: () => {} },
  console: console
};
mockWindow.window = mockWindow;
mockWindow.global = mockWindow;

// Expose globals onto global for ESM or VM usage
global.window = mockWindow;
global.document = documentMock;
global.DOMPurify = mockWindow.DOMPurify;
global.marked = mockWindow.marked;
global.EventSource = mockWindow.EventSource;
global.localStorage = localStorageMock;
global.fetch = mockWindow.fetch;
global.matchMedia = mockWindow.matchMedia;
global.location = mockWindow.location;
global.history = mockWindow.history;
global.requestAnimationFrame = mockWindow.requestAnimationFrame;
global.cancelAnimationFrame = mockWindow.cancelAnimationFrame;
try {
  Object.defineProperty(global, 'navigator', {
    value: mockWindow.navigator,
    configurable: true,
    writable: true
  });
} catch (_) {
  Object.assign(global.navigator, mockWindow.navigator);
}

async function runHarness() {
  let testExports;
  if (fs.existsSync(testHooksPath)) {
    // Native ES module architecture
    await import(testHooksPath);
    testExports = mockWindow._test || {};
  } else {
    // Pre-split monolithic IIFE architecture
    const appJs = fs.readFileSync(appJsPath, 'utf8');
    const context = vm.createContext(mockWindow);
    vm.runInContext(appJs, context);
    testExports = mockWindow._test || {};
  }

  const win = mockWindow;
  if (typeof testExports.bind === 'function') {
    testExports.bind();
  }

  console.log('--- 1. Testing Thought Cleaning & ANSI Stripping ---');
  const stripAnsi = testExports.stripAnsi || win.stripAnsi;
  const cleanThoughtText = testExports.cleanThoughtText || win.cleanThoughtText;
  assert.ok(typeof stripAnsi === 'function', 'stripAnsi is exported');
  assert.ok(typeof cleanThoughtText === 'function', 'cleanThoughtText is exported');

  assert.strictEqual(stripAnsi('\x1b[32mChecking code\x1b[0m'), 'Checking code');
  assert.strictEqual(stripAnsi('\u001b[1;31mError\u001b[0m: \x1b[33mWarning\x1b[0m'), 'Error: Warning');
  assert.strictEqual(stripAnsi('plain string'), 'plain string');
  assert.strictEqual(stripAnsi(''), '');
  assert.strictEqual(stripAnsi(null), '');
  assert.strictEqual(stripAnsi(undefined), '');

  const cleanThought = (t) => cleanThoughtText(t, stripAnsi);
  assert.strictEqual(cleanThought('\x1b[32mChecking code\x1b[0m'), 'Checking code');
  assert.strictEqual(
    cleanThought('<think>Thinking...</think>\n<thought>Done</thought>'),
    'Thinking...\nDone'
  );
  assert.strictEqual(cleanThought('<thinking>Pondering options</thinking>'), 'Pondering options');
  assert.strictEqual(cleanThought('A\n\n\n\nB'), 'A\n\nB');
  assert.strictEqual(cleanThought('Line with trailing space   \nNext'), 'Line with trailing space\nNext');
  assert.strictEqual(cleanThought(''), '');
  assert.strictEqual(cleanThought(null), '');
  console.log('PASS: cleanThoughtText and stripAnsi verified');

  console.log('\n--- 2. Testing Inline Tool Formatting Logic ---');
  const formatToolCallInline = testExports.formatToolCallInline || win.formatToolCallInline;
  assert.ok(typeof formatToolCallInline === 'function', 'formatToolCallInline is exported');

  const bashCall = {
    name: 'bash',
    title: 'bash: git status',
    args: { command: 'git status -s' },
    status: 'running'
  };
  const formattedBash = formatToolCallInline(bashCall);
  assert.ok(formattedBash, 'formatToolCallInline returned non-null');
  assert.ok(
    formattedBash.name.includes('git status') || formattedBash.name.includes('bash'),
    `Expected tool name to include "git status" or "bash", got "${formattedBash.name}"`
  );
  assert.strictEqual(formattedBash.status, 'running');

  const longArgCall = {
    name: 'grep',
    args: { pattern: 'A'.repeat(200), path: 'B'.repeat(200) },
    status: 'running'
  };
  const formattedLong = formatToolCallInline(longArgCall);
  assert.ok(
    (formattedLong.name && (formattedLong.name.length <= 150 || formattedLong.name.includes('...'))) ||
    (formattedLong.argsStr && (formattedLong.argsStr.length <= 150 || formattedLong.argsStr.includes('...'))),
    'Long arguments capped in inline view'
  );

  const completedCall = {
    name: 'read',
    args: { path: 'file.txt' },
    status: 'completed',
    result: 'file contents'
  };
  assert.strictEqual(formatToolCallInline(completedCall).status, 'completed');

  const failedCall = {
    name: 'eval',
    args: { code: '1/0' },
    status: 'failed',
    error: 'ZeroDivisionError'
  };
  assert.strictEqual(formatToolCallInline(failedCall).status, 'failed');
  console.log('PASS: formatToolCallInline handles commands, truncation, and statuses');

  console.log('\n--- 3. Simulating ACP Event Stream Processing ---');
  const openAgentDrawer = testExports.openAgentDrawer || win.openAgentDrawer;
  const handleAgentStreamEvent = testExports.handleAgentStreamEvent || win.handleAgentStreamEvent;
  const closeAgentDrawer = testExports.closeAgentDrawer || win.closeAgentDrawer;
  assert.ok(typeof openAgentDrawer === 'function', 'openAgentDrawer is exported');
  assert.ok(typeof handleAgentStreamEvent === 'function', 'handleAgentStreamEvent is exported');

  const streamEl = documentMock.getElementById('agent-trajectory-stream');
  const turnsMetricEl = documentMock.getElementById('agent-metric-turns');
  const callsMetricEl = documentMock.getElementById('agent-metric-calls');
  const swimlaneSvgEl = documentMock.getElementById('agent-swimlane-svg');
  const inspectorEl = documentMock.getElementById('agent-tool-inspector');
  const inspectorArgsEl = documentMock.getElementById('inspector-args');
  const inspectorOutputEl = documentMock.getElementById('inspector-output');

  openAgentDrawer('item-trajectory-test');
  assert.strictEqual(inspectorEl.hidden, true, 'Inspector hidden initially');
  assert.ok(streamEl.children.length >= 1, 'Trajectory stream contains initial CONTEXT row');

  const initialContextRow = streamEl.children[0];
  const initialBadge = findDescendant(initialContextRow, '.badge-context') ||
    findDescendant(initialContextRow, '.trajectory-badge');
  assert.ok(initialBadge, 'Initial row has badge');
  assert.strictEqual(initialBadge.textContent.trim().toUpperCase(), 'CONTEXT');

  // Turn 1 thought chunk
  handleAgentStreamEvent({
    type: 'agent_thought_chunk',
    update: 'Analyzing the repository structure and workflow files.\n'
  });
  assert.ok(turnsMetricEl.textContent.includes('1'), 'Turns metric is 1');

  let assistantRows = findAllDescendants(streamEl, '.trajectory-row-assistant');
  if (assistantRows.length === 0) {
    assistantRows = streamEl.children.filter(r => !!findDescendant(r, '.badge-assistant'));
  }
  assert.strictEqual(assistantRows.length, 1, 'One ASSISTANT row created');
  assert.ok(assistantRows[0].textContent.includes('Analyzing the repository structure'));

  // Consecutive thought chunk accumulates
  handleAgentStreamEvent({
    type: 'agent_thought_chunk',
    update: 'Found relevant configuration in .github/workflows.\n'
  });
  assert.ok(turnsMetricEl.textContent.includes('1'), 'Turns metric remains 1');
  assert.ok(assistantRows[0].textContent.includes('Found relevant configuration'));

  // Tool call closes turn
  handleAgentStreamEvent({
    type: 'tool_call',
    update: {
      toolCallId: 'call_bash_1',
      name: 'bash',
      title: 'bash: git status',
      args: { command: 'git status -s' },
      status: 'running'
    }
  });
  assert.ok(callsMetricEl.textContent.includes('1'), 'Calls metric incremented to 1');

  let toolRows = findAllDescendants(streamEl, '.trajectory-row-tool');
  if (toolRows.length === 0) {
    toolRows = streamEl.children.filter(r => !!findDescendant(r, '.badge-tool'));
  }
  assert.strictEqual(toolRows.length, 1, 'One TOOL row created');

  // Tool call update
  handleAgentStreamEvent({
    type: 'tool_call_update',
    update: {
      toolCallId: 'call_bash_1',
      status: 'completed',
      output: 'M src/assets/app.js\n?? tests/viewer_trajectory.js\n'
    }
  });

  // Tool inspector
  const toolLine = findDescendant(toolRows[0], '.trajectory-tool-line') || toolRows[0];
  toolLine.click();
  assert.strictEqual(inspectorEl.hidden, false, 'Inspector opened on tool row click');
  assert.ok(inspectorArgsEl.textContent.includes('git status'), 'Inspector shows args');
  assert.ok(inspectorOutputEl.textContent.includes('src/assets/app.js'), 'Inspector shows output');

  const closeInspectorBtn = documentMock.getElementById('btn-close-inspector');
  if (closeInspectorBtn) {
    closeInspectorBtn.click();
    assert.strictEqual(inspectorEl.hidden, true, 'Inspector closed');
  }

  // Finished event
  handleAgentStreamEvent({
    type: 'finished',
    update: { success: true }
  });
  const lastRow = streamEl.children[streamEl.children.length - 1];
  const finalBadge = findDescendant(lastRow, '.badge-context') ||
    findDescendant(lastRow, '.trajectory-badge');
  assert.ok(finalBadge, 'Finished row has CONTEXT badge');

  // Swimlane SVG
  const swimlaneContent = swimlaneSvgEl.innerHTML ||
    (swimlaneSvgEl.children && swimlaneSvgEl.children.map(c => c.innerHTML || c.textContent).join('')) ||
    swimlaneSvgEl.textContent;
  assert.ok(swimlaneContent.includes('Input') || swimlaneContent.includes('Model'), 'Swimlane rendered lanes');

  // Trajectory search
  const searchInput = documentMock.getElementById('agent-trajectory-search');
  if (searchInput) {
    searchInput.value = 'git status';
    searchInput.dispatchEvent({ type: 'input', target: searchInput });
    assert.strictEqual(assistantRows[0].hidden, true, 'Non-matching row hidden');
    assert.strictEqual(toolRows[0].hidden, false, 'Matching row visible');

    searchInput.value = '';
    searchInput.dispatchEvent({ type: 'input', target: searchInput });
    assert.strictEqual(assistantRows[0].hidden, false, 'Rows unhidden on clear search');
  }
  if (typeof closeAgentDrawer === 'function') closeAgentDrawer();
  console.log('PASS: Trajectory stream, inspector, swimlane, and search verified');

  console.log('\n--- 4. Testing Spec Ticket Editor Logic ---');
  const openSpecModal = testExports.openSpecModal || win.openSpecModal;
  const closeSpecModal = testExports.closeSpecModal || win.closeSpecModal;
  const setSpecModalTab = testExports.setSpecModalTab || win.setSpecModalTab;
  const expandQuickAdd = testExports.expandQuickAdd || win.expandQuickAdd;
  const collapseQuickAdd = testExports.collapseQuickAdd || win.collapseQuickAdd;
  const SPEC_SNIPPETS = testExports.SPEC_SNIPPETS || win.SPEC_SNIPPETS;

  assert.ok(typeof openSpecModal === 'function', 'openSpecModal is exported');
  assert.ok(typeof closeSpecModal === 'function', 'closeSpecModal is exported');
  assert.ok(typeof setSpecModalTab === 'function', 'setSpecModalTab is exported');
  assert.ok(typeof expandQuickAdd === 'function', 'expandQuickAdd is exported');
  assert.ok(typeof collapseQuickAdd === 'function', 'collapseQuickAdd is exported');
  assert.ok(SPEC_SNIPPETS && SPEC_SNIPPETS.ac, 'SPEC_SNIPPETS defined');

  const quickAddTrigger = documentMock.getElementById('quick-add-trigger');
  const quickIdeaTrigger = documentMock.getElementById('quick-idea-trigger');
  const quickAddForm = documentMock.getElementById('quick-add-form');
  const quickAddType = documentMock.getElementById('quick-add-type');
  expandQuickAdd();
  assert.strictEqual(quickAddTrigger.hidden, true, 'Trigger hidden when expanded');
  assert.strictEqual(quickIdeaTrigger.hidden, true, 'Idea trigger hidden when expanded');
  assert.strictEqual(quickAddForm.hidden, false, 'Form visible when expanded');
  collapseQuickAdd();
  assert.strictEqual(quickAddTrigger.hidden, false, 'Trigger visible when collapsed');
  assert.strictEqual(quickIdeaTrigger.hidden, false, 'Idea trigger visible when collapsed');
  assert.strictEqual(quickAddForm.hidden, true, 'Form hidden when collapsed');

  expandQuickAdd('title', 'idea');
  assert.strictEqual(quickAddTrigger.hidden, true, 'Trigger hidden when expanded for idea');
  assert.strictEqual(quickIdeaTrigger.hidden, true, 'Idea trigger hidden when expanded for idea');
  assert.strictEqual(quickAddType.value, 'idea', 'Default type set to idea');
  collapseQuickAdd();

  const specModal = documentMock.getElementById('spec-modal');
  const specModalTitle = documentMock.getElementById('spec-modal-title');
  const specModalBody = documentMock.getElementById('spec-modal-body');
  const specModalType = documentMock.getElementById('spec-modal-type');
  const specModalKind = documentMock.getElementById('spec-modal-kind');
  const specModalPriority = documentMock.getElementById('spec-modal-priority');
  const specModalTags = documentMock.getElementById('spec-modal-tags');

  openSpecModal({
    title: 'Spec Test Title',
    body: '## Description\nDetailed spec\n\n## Acceptance Criteria\n- [ ] Done',
    type: 'bug',
    kind: 'technical',
    priority: 'high',
    tags: 'test,spec'
  });
  assert.strictEqual(specModalTitle.value, 'Spec Test Title');
  assert.ok(specModalBody.value.includes('Acceptance Criteria'));
  assert.strictEqual(specModalType.value, 'bug');
  assert.strictEqual(specModalKind.value, 'technical');
  assert.strictEqual(specModalPriority.value, 'high');
  assert.strictEqual(specModalTags.value, 'test,spec');
  assert.ok(specModal.open || specModal.hasAttribute('open'));

  const specTabWrite = documentMock.getElementById('spec-tab-write');
  const specTabPreview = documentMock.getElementById('spec-tab-preview');
  const specModalPreview = documentMock.getElementById('spec-modal-preview');

  setSpecModalTab('preview');
  assert.strictEqual(specModalBody.hidden, true);
  assert.strictEqual(specModalPreview.hidden, false);
  assert.ok(specTabPreview.classList.contains('active'));

  setSpecModalTab('write');
  assert.strictEqual(specModalBody.hidden, false);
  assert.strictEqual(specModalPreview.hidden, true);
  assert.ok(specTabWrite.classList.contains('active'));

  closeSpecModal();
  assert.ok(!specModal.open && !specModal.hasAttribute('open'));
  console.log('PASS: Spec editor and quick-add toggle verified');

  console.log('\n--- 5. Testing Screenshot Pasting Logic ---');
  const getClipboardImages = testExports.getClipboardImages || win.getClipboardImages;
  const handleScreenshotPaste = testExports.handleScreenshotPaste || win.handleScreenshotPaste;
  const uploadScreenshot = testExports.uploadScreenshot || win.uploadScreenshot;

  assert.ok(typeof getClipboardImages === 'function', 'getClipboardImages exported');
  assert.ok(typeof handleScreenshotPaste === 'function', 'handleScreenshotPaste exported');
  assert.ok(typeof uploadScreenshot === 'function', 'uploadScreenshot exported');

  const fakeImageFile = { name: 'screenshot.png', type: 'image/png', size: 1024 };
  const mockClipboardWithImage = {
    clipboardData: {
      items: [
        { type: 'text/plain', getAsFile: () => null },
        { type: 'image/png', getAsFile: () => fakeImageFile }
      ]
    }
  };
  const extractedImages = getClipboardImages(mockClipboardWithImage);
  assert.strictEqual(extractedImages.length, 1);
  assert.strictEqual(extractedImages[0], fakeImageFile);

  const prevFetch = fetchStub;
  fetchStub = function (url) {
    if (url.includes('screenshots')) {
      return Promise.resolve({
        ok: true,
        status: 200,
        headers: { get: () => null },
        json: () => Promise.resolve({
          path: '/tmp/pin-screenshots/ss.png',
          filename: 'ss.png',
          markdown: '![screenshot](/tmp/pin-screenshots/ss.png)'
        })
      });
    }
    return Promise.resolve({ ok: true, status: 200, headers: { get: () => null }, json: () => Promise.resolve([]) });
  };

  const quickAddBodyEl = documentMock.getElementById('quick-add-body');
  quickAddBodyEl.value = 'Initial spec description';
  let pastePrevented = false;
  handleScreenshotPaste({
    type: 'paste',
    preventDefault() { pastePrevented = true; },
    clipboardData: mockClipboardWithImage.clipboardData,
    currentTarget: quickAddBodyEl
  }, quickAddBodyEl);
  assert.strictEqual(pastePrevented, true);
  assert.ok(quickAddBodyEl.value.includes('![Uploading screenshot...]()'));

  await new Promise(r => setTimeout(r, 20));
  assert.ok(quickAddBodyEl.value.includes('![screenshot](/tmp/pin-screenshots/ss.png)'));
  fetchStub = prevFetch;
  console.log('PASS: Screenshot paste handling verified');

  console.log('\n--- 6. Testing Worktree Modal Prompt Logic ---');
  const openWorktreeModal = testExports.openWorktreeModal || win.openWorktreeModal;
  const closeWorktreeModal = testExports.closeWorktreeModal || win.closeWorktreeModal;
  assert.ok(typeof openWorktreeModal === 'function', 'openWorktreeModal exported');
  assert.ok(typeof closeWorktreeModal === 'function', 'closeWorktreeModal exported');

  const testItem = { id: 'item-wt-test' };
  const modalTitle = documentMock.getElementById('worktree-modal-title');
  const busyMsg = documentMock.getElementById('worktree-busy-msg');
  const promptMsg = documentMock.getElementById('worktree-prompt-msg');
  const primaryBtn = documentMock.getElementById('btn-worktree-primary');

  openWorktreeModal(testItem, null);
  assert.strictEqual(modalTitle.textContent, 'Start Agent Run');
  assert.strictEqual(busyMsg.hidden, true);
  assert.strictEqual(promptMsg.hidden, false);
  assert.strictEqual(primaryBtn.hidden, false);

  openWorktreeModal(testItem, 'busy-123');
  assert.strictEqual(modalTitle.textContent, 'Primary Working Tree Busy');
  assert.strictEqual(busyMsg.hidden, false);
  assert.strictEqual(promptMsg.hidden, true);
  assert.strictEqual(primaryBtn.hidden, true);

  closeWorktreeModal();
  console.log('PASS: Worktree modal manual vs busy toggle verified');

  console.log('\n--- 7. Testing Board Card Drag and Drop ---');
  const handleCardDrop = testExports.handleCardDrop || win.handleCardDrop;
  assert.ok(typeof handleCardDrop === 'function', 'handleCardDrop is exported');

  // Populate state with items
  testExports.state.items = [
    { id: 'item-drop-1', title: 'Planned Item', status: 'planned', type: 'task', revision: 1 },
    { id: 'item-drop-2', title: 'Created Item', status: 'created', type: 'task', revision: 1 }
  ];

  let actionPayload = null;
  const originalFetchAction = fetchStub;
  fetchStub = function (url, opts) {
    if (url.includes('/action')) {
      actionPayload = JSON.parse(opts.body);
      return Promise.resolve({
        ok: true,
        status: 200,
        headers: { get: () => null },
        json: () => Promise.resolve({ item: testExports.state.items[1] })
      });
    }
    return originalFetchAction(url, opts);
  };

  // Dropping created card on planned calls transition endpoint
  handleCardDrop('item-drop-2', 'planned');
  assert.ok(actionPayload, 'sendAction was called for transition');
  assert.strictEqual(actionPayload.action, 'transition');
  assert.strictEqual(actionPayload.to, 'planned');
  assert.strictEqual(actionPayload.expect_revision, 1);

  // Dropping created card on done calls transition endpoint with to: done
  handleCardDrop('item-drop-2', 'done');
  assert.ok(actionPayload, 'sendAction was called for transition to done');
  assert.strictEqual(actionPayload.action, 'transition');
  assert.strictEqual(actionPayload.to, 'done');
  assert.strictEqual(actionPayload.expect_revision, 1);

   // Dropping planned card on in_progress prompts worktree modal
  handleCardDrop('item-drop-1', 'in_progress');
  assert.strictEqual(testExports.state.pendingWorktreeItem.id, 'item-drop-1');
  assert.strictEqual(modalTitle.textContent, 'Start Agent Run');
  closeWorktreeModal();

  fetchStub = originalFetchAction;
  console.log('PASS: Board card drop transition verified');

  console.log('\n--- 8. Testing Unified Run Dispatcher (sendRunItem) ---');
  const sendRunItem = testExports.sendRunItem || win.sendRunItem;
  assert.ok(typeof sendRunItem === 'function', 'sendRunItem is exported');

  // Test Case 8.1: Success opens trajectory drawer and sets trajectory tab
  let recordedCalls = [];
  fetchStub = function (url, opts) {
    recordedCalls.push({ url, opts });
    if (url.includes('data.json')) {
      return Promise.resolve({
        ok: true,
        status: 200,
        headers: { get: () => null },
        json: () => Promise.resolve({ items: testExports.state.items })
      });
    }
    if (url.includes('runs')) {
      return Promise.resolve({
        ok: true,
        status: 200,
        headers: { get: () => null },
        json: () => Promise.resolve({ running: [], primary_busy: null })
      });
    }
    return Promise.resolve({
      ok: true,
      status: 200,
      headers: { get: () => null },
      json: () => Promise.resolve({ status: 'started' })
    });
  };

  openAgentDrawer('other-item');
  await sendRunItem('item-drop-1', false);
  const runCall = recordedCalls.find(c => c.url.includes('items/item-drop-1/run'));
  assert.ok(runCall, 'Run URL called: ' + JSON.stringify(recordedCalls.map(c => c.url)));
  assert.strictEqual(runCall.opts.body, '{}', 'Default run must not request worktree');
  assert.ok(
    testExports.state.agentDrawerItem === 'item-drop-1' ||
    documentMock.getElementById('agent-drawer').classList.contains('open') ||
    documentMock.getElementById('agent-drawer').hidden === false,
    'Success opens trajectory drawer'
  );
  console.log('PASS: sendRunItem success opens trajectory drawer');

  // Test Case 8.2: primary_busy opens worktree modal without drawer
  fetchStub = function (url, opts) {
    recordedCalls.push({ url, opts });
    if (url.includes('data.json')) {
      return Promise.resolve({
        ok: true,
        status: 200,
        headers: { get: () => null },
        json: () => Promise.resolve({ items: testExports.state.items })
      });
    }
    return Promise.resolve({
      ok: false,
      status: 409,
      headers: { get: () => null },
      json: () => Promise.resolve({ status: 'primary_busy', active_id: 'item-busy-xyz' })
    });
  };

  closeWorktreeModal();
  testExports.state.agentDrawerItem = null;
  await sendRunItem('item-drop-1', false);
  assert.strictEqual(testExports.state.pendingWorktreeItem.id, 'item-drop-1');
  assert.strictEqual(modalTitle.textContent, 'Primary Working Tree Busy');
  assert.strictEqual(busyMsg.hidden, false);
  assert.strictEqual(testExports.state.agentDrawerItem, null, 'primary_busy does not open drawer');
  closeWorktreeModal();
  console.log('PASS: sendRunItem primary_busy opens worktree modal');

  // Test Case 8.3: failure toasts error and does not open drawer
  let lastOpts = null;
  fetchStub = function (url, opts) {
    lastOpts = opts;
    return Promise.resolve({
      ok: false,
      status: 500,
      headers: { get: () => null },
      json: () => Promise.resolve({ error: 'git exploded' })
    });
  };

  testExports.state.agentDrawerItem = null;
  await sendRunItem('item-drop-1', true, 'Worktree start failed: ');
  assert.strictEqual(lastOpts.body, JSON.stringify({ use_worktree: true }), 'worktree run sent use_worktree');
  const toastEl = documentMock.getElementById('toast');
  assert.ok(
    toastEl.textContent.includes('git exploded') || toastEl.textContent.includes('Worktree start failed:'),
    `Toast contains error message: "${toastEl.textContent}"`
  );
  assert.strictEqual(testExports.state.agentDrawerItem, null, 'failed run does not open drawer');
  console.log('PASS: sendRunItem error toasts failure and suppresses drawer');

  // Test Case 8.4: Running in board view stays in board view without opening drawer
  testExports.setViewMode('board');
  fetchStub = function (url) {
    if (url.includes('runs')) {
      return Promise.resolve({ ok: true, status: 200, json: () => Promise.resolve({ running: ['item-board-run'] }) });
    }
    return Promise.resolve({ ok: true, status: 200, json: () => Promise.resolve({ status: 'started' }) });
  };
  await sendRunItem('item-board-run', false);
  assert.strictEqual(testExports.state.viewMode, 'board', 'sendRunItem in board view keeps viewMode as board');
  console.log('PASS: sendRunItem in board view preserves board view');

  // Test Case 8.5: moving a ticket from backlog to in progress updates visual status on board and calls refreshData
  let refreshDataCount = 0;
  testExports.bind({
    refreshData: function () {
      refreshDataCount++;
      return Promise.resolve();
    }
  });

  testExports.state.items = [
    { id: 'backlog-item-1', title: 'Backlog Item', status: 'created', type: 'task', revision: 1 }
  ];
  testExports.setViewMode('board');
  if (testExports.renderList) testExports.renderList();
  else {
    testExports.state.shown = testExports.state.items.slice();
    testExports.renderBoard();
  }
  const createdColBefore = documentMock.querySelector('.board-col-cards[data-drop-status="created"]');
  const inProgressColBefore = documentMock.querySelector('.board-col-cards[data-drop-status="in_progress"]');
  assert.ok(createdColBefore.querySelector('.board-card[data-id="backlog-item-1"]'), 'Backlog item initially in created column');
  assert.strictEqual(inProgressColBefore.querySelector('.board-card[data-id="backlog-item-1"]'), null, 'Backlog item not in in_progress column');

  fetchStub = function (url) {
    if (url.includes('runs')) {
      return Promise.resolve({
        ok: true,
        status: 200,
        headers: { get: () => null },
        json: () => Promise.resolve({ running: ['backlog-item-1'], timings: {} })
      });
    }
    return Promise.resolve({
      ok: true,
      status: 200,
      headers: { get: () => null },
      json: () => Promise.resolve({ status: 'started' })
    });
  };

  await sendRunItem('backlog-item-1', false);
  const backlogItemAfter = testExports.state.items.find(i => i.id === 'backlog-item-1');
  assert.strictEqual(backlogItemAfter.status, 'in_progress', 'Ticket status updated to in_progress');
  assert.ok(refreshDataCount > 0, 'refreshData was triggered after starting item');

  const createdColAfter = documentMock.querySelector('.board-col-cards[data-drop-status="created"]');
  const inProgressColAfter = documentMock.querySelector('.board-col-cards[data-drop-status="in_progress"]');
  assert.strictEqual(createdColAfter.querySelector('.board-card[data-id="backlog-item-1"]'), null, 'Ticket no longer in created/backlog column');
  assert.ok(inProgressColAfter.querySelector('.board-card[data-id="backlog-item-1"]'), 'Ticket visually moved to in_progress column');
  console.log('PASS: moving ticket from backlog to in_progress updates board visually without refresh');

  console.log('\n--- 9. Testing Handoff Card Layout and Priority ---');
  const renderHandoff = testExports.renderHandoff || win.renderHandoff;
  assert.ok(typeof renderHandoff === 'function', 'renderHandoff is exported');
  const handoffCardEl = documentMock.getElementById('handoff-card');

  const completedItem = {
    id: 'rev-item-1',
    status: 'done',
    handoff: {
      progress: '1. Added feature XYZ\n2. Refactored module',
      verification: 'Cargo test passed 51/51 unit tests'
    }
  };
  renderHandoff(completedItem);
  assert.strictEqual(handoffCardEl.hidden, false);
  const fields = handoffCardEl.children.filter(c => c.classList && c.classList.contains('handoff-field'));
  assert.strictEqual(fields.length, 2);
  assert.ok(fields[0].classList.contains('handoff-field-verification'), 'First block is verification for completed item');
  assert.ok(fields[1].classList.contains('handoff-field-progress'), 'Second block is progress');
  assert.ok(fields[0].children.some(c => c.classList && c.classList.contains('handoff-text')), 'Verification renders as rich handoff-text');
  assert.ok(fields[1].children.some(c => c.classList && c.classList.contains('handoff-text')), 'Progress renders as rich handoff-text');

  const blockedItem = {
    id: 'blocked-item-1',
    status: 'blocked',
    handoff: {
      blocker: 'Missing credentials for production deployment',
      verification: 'Build compiles successfully',
      progress: 'Setup partially complete'
    }
  };
  renderHandoff(blockedItem);
  const blockedFields = handoffCardEl.children.filter(c => c.classList && c.classList.contains('handoff-field'));
  assert.strictEqual(blockedFields.length, 3);
  assert.ok(blockedFields[0].classList.contains('handoff-field-blocker'), 'First block is blocker for blocked item');
  assert.ok(blockedFields[1].classList.contains('handoff-field-verification'), 'Second block is verification');
  assert.ok(blockedFields[2].classList.contains('handoff-field-progress'), 'Third block is progress');
  console.log('PASS: Handoff card layout and priority verified');

  console.log('\n--- 10. Testing Commit + PR Quick Action Buttons ---');
  const buildBoardCard = testExports.buildBoardCard || win.buildBoardCard;
  assert.ok(typeof buildBoardCard === 'function', 'buildBoardCard is exported');

  const doneCard = buildBoardCard({ id: 'd1', title: 'Done', status: 'done', type: 'task', timestamp: Date.now() });
  assert.ok(doneCard.querySelector('.btn-commit-pr'), 'Done card has Commit + PR');

  const plannedCard = buildBoardCard({ id: 'p1', title: 'Planned', status: 'planned', type: 'task', timestamp: Date.now() });
  assert.strictEqual(plannedCard.querySelector('.btn-commit-pr'), null, 'Planned card lacks Commit + PR');

  // Merged card suppresses Commit + PR and shows indicator
  const mergedCard = buildBoardCard({ id: 'rm1', title: 'Merged', status: 'done', type: 'task', git_status: 'merged', timestamp: Date.now() });
  assert.strictEqual(mergedCard.querySelector('.btn-commit-pr'), null, 'Merged card suppresses Commit + PR');
  assert.ok(mergedCard.querySelector('.indicator-merged'), 'Merged card has indicator');
  assert.strictEqual(mergedCard.querySelector('.indicator-merged').textContent, 'Merged into main');

  // Open PR card suppresses Commit + PR and shows PR link
  const prCard = buildBoardCard({ id: 'rp1', title: 'Open PR', status: 'done', type: 'task', git_status: 'pr_open', pr_url: 'https://github.com/JoeriKaiser/pin/pull/42', pr_number: 42, timestamp: Date.now() });
  assert.strictEqual(prCard.querySelector('.btn-commit-pr'), null, 'Open PR card suppresses Commit + PR');
  const prLink = prCard.querySelector('.btn-pr-link');
  assert.ok(prLink, 'Open PR card has PR link button');
  assert.strictEqual(prLink.getAttribute('href'), 'https://github.com/JoeriKaiser/pin/pull/42');
  assert.ok(prLink.textContent.includes('PR #42'), 'PR link button displays PR number');

  // Merged done card suppresses Commit + PR
  const doneMerged = buildBoardCard({ id: 'dm1', title: 'Done Merged', status: 'done', type: 'task', git_status: 'merged', timestamp: Date.now() });
  assert.strictEqual(doneMerged.querySelector('.btn-commit-pr'), null, 'Merged done card suppresses Commit + PR');
  assert.ok(doneMerged.querySelector('.indicator-merged'), 'Merged done card has indicator');

  // Open PR done card suppresses Commit + PR and shows PR link
  const donePr = buildBoardCard({ id: 'dp1', title: 'Done PR', status: 'done', type: 'task', git_status: 'pr_open', pr_url: 'https://github.com/JoeriKaiser/pin/pull/99', pr_number: 99, timestamp: Date.now() });
  assert.strictEqual(donePr.querySelector('.btn-commit-pr'), null, 'Open PR done card suppresses Commit + PR');
  assert.ok(donePr.querySelector('.btn-pr-link'), 'Open PR done card has PR link');

  // Done card while agent is running shows Running Agent and View Run, suppresses Commit + PR
  testExports.state.activeRuns = new Set(['done-run-1']);
  const doneRunningCard = buildBoardCard({ id: 'done-run-1', title: 'Running Done Task', status: 'done', type: 'task', timestamp: Date.now() });
  assert.strictEqual(doneRunningCard.querySelector('.btn-commit-pr'), null, 'Running done card suppresses Commit + PR');
  assert.ok(doneRunningCard.querySelector('.card-running-indicator'), 'Running done card has running indicator');
  assert.ok(doneRunningCard.querySelector('.btn-view-run'), 'Running done card has view run button');

  // When agent run stream completes, Done card immediately gets Commit + PR created
  testExports.state.items = [
    { id: 'done-run-1', title: 'Running Done Task', status: 'done', type: 'task', timestamp: Date.now() }
  ];
  testExports.state.currentStreamingId = 'done-run-1';
  testExports.state.viewMode = 'board';
  testExports.handleAgentStreamEvent({ type: 'finished', update: { success: true } });
  assert.strictEqual(testExports.state.activeRuns.has('done-run-1'), false, 'activeRuns removed completed task');
  const doneColCards = documentMock.querySelector('.board-col-cards[data-drop-status="done"]');
  const reRenderedCard = doneColCards.querySelector('.board-card[data-id="done-run-1"]');
  assert.ok(reRenderedCard, 'Done column contains completed card');
  assert.ok(reRenderedCard.querySelector('.btn-commit-pr'), 'Completed card in Done column immediately got Commit + PR button');
  assert.strictEqual(reRenderedCard.querySelector('.card-running-indicator'), null, 'Completed card no longer has running indicator');

  // Switching viewMode to board calls renderBoard and keeps cards fresh
  testExports.setViewMode('detail');
  testExports.state.items[0].title = 'Updated Title in Board';
  testExports.setViewMode('board');
  const boardCardTitle = doneColCards.querySelector('.board-card-title');
  assert.ok(boardCardTitle.textContent.includes('Updated Title in Board'), 'setViewMode(board) immediately re-renders board cards');

  // Quick archive button on Done cards
  assert.ok(doneCard.querySelector('.btn-archive'), 'Done card has quick archive button');
  assert.strictEqual(plannedCard.querySelector('.btn-archive'), null, 'Planned card lacks archive button');
  assert.ok(mergedCard.querySelector('.btn-archive'), 'Merged done card has quick archive button');
  assert.ok(prCard.querySelector('.btn-archive'), 'Open PR done card has quick archive button');
  assert.ok(doneMerged.querySelector('.btn-archive'), 'Merged done card has quick archive button');
  assert.ok(donePr.querySelector('.btn-archive'), 'Open PR done card has quick archive button');
  assert.strictEqual(doneRunningCard.querySelector('.btn-archive'), null, 'Running done card suppresses quick archive button');

  const alreadyArchivedCard = buildBoardCard({ id: 'arch-1', title: 'Archived Card', status: 'done', type: 'task', archived_at: 1700000000, timestamp: Date.now() });
  assert.strictEqual(alreadyArchivedCard.querySelector('.btn-archive'), null, 'Already archived card lacks archive button');

  // Clicking quick archive button sends archive action
  let archiveActionPayload = null;
  let archiveActionUrl = null;
  fetchStub = function (url, opts) {
    if (url.includes('/action')) {
      archiveActionUrl = url;
      archiveActionPayload = JSON.parse(opts.body);
      return Promise.resolve({
        ok: true,
        status: 200,
        headers: { get: () => null },
        json: () => Promise.resolve({ id: 'd1', status: 'done', archived_at: Date.now() })
      });
    }
    return Promise.resolve({ ok: true, status: 200, json: () => Promise.resolve({}) });
  };

  const archiveBtn = doneCard.querySelector('.btn-archive');
  assert.ok(archiveBtn, 'Done card has archive button');
  archiveBtn.click();
  assert.ok(archiveActionUrl && archiveActionUrl.includes('items/d1/action'), 'Archive button calls item action endpoint');
  assert.strictEqual(archiveActionPayload.action, 'archive', 'Archive action payload sent');

   // Git Sync button handling
   const handleGitSync = testExports.handleGitSync;
   assert.ok(typeof handleGitSync === 'function', 'handleGitSync is exported');
   let syncCalled = false;
   fetchStub = function (url) {
     if (url.includes('sync')) {
       syncCalled = true;
       return Promise.resolve({
         ok: true,
         status: 200,
         json: () => Promise.resolve({ status: 'ok', synced: [{ id: 's1' }] })
       });
     }
     return Promise.resolve({ ok: true, status: 200, json: () => Promise.resolve({}) });
   };
   await handleGitSync();
   assert.ok(syncCalled, 'handleGitSync called /sync endpoint');

   console.log('PASS: Commit + PR quick action buttons and Git sync verified');
  console.log('\n--- 11. Testing Auto Update Check & Update Button ---');
  const checkUpdate = testExports.checkUpdate;
  const handleUpdateInstall = testExports.handleUpdateInstall;
  assert.ok(typeof checkUpdate === 'function', 'checkUpdate is exported');
  assert.ok(typeof handleUpdateInstall === 'function', 'handleUpdateInstall is exported');

  const btnUpdateEl = documentMock.getElementById('btn-update');
  const updateVersionEl = documentMock.getElementById('update-version');
  const updateFooterEl = documentMock.getElementById('update-footer');
  const updateToastEl = documentMock.getElementById('toast');

  // Test 1: Update available
  fetchStub = function (url) {
    if (url.includes('update')) {
      return Promise.resolve({
        ok: true,
        status: 200,
        json: () => Promise.resolve({
          current_version: '2.2.0',
          latest_version: '2.3.0',
          update_available: true
        })
      });
    }
    return Promise.resolve({ ok: true, status: 200, json: () => Promise.resolve([]) });
  };

  await checkUpdate();
  assert.strictEqual(btnUpdateEl.hidden, false, 'Update button visible when update available');
  assert.ok(updateVersionEl.textContent.includes('2.3.0'), 'Update version text contains 2.3.0');
  assert.ok(updateFooterEl.textContent.includes('2.3.0'), 'Footer contains update version');

  // Test 2: Update install success
  fetchStub = function (url, opts) {
    if (url.includes('update') && opts && opts.method === 'POST') {
      return Promise.resolve({
        ok: true,
        status: 200,
        json: () => Promise.resolve({
          success: true,
          previous_version: '2.2.0',
          updated_version: '2.3.0',
          message: 'pin successfully updated to version 2.3.0'
        })
      });
    }
    return Promise.resolve({ ok: true, status: 200, json: () => Promise.resolve([]) });
  };

  await handleUpdateInstall();
  assert.ok(btnUpdateEl.classList.contains('updated'), 'Button has updated class after install');
  assert.ok(btnUpdateEl.textContent.includes('2.3.0'), 'Button text shows updated version');
  assert.ok(updateToastEl.textContent.includes('pin updated successfully'), 'Toast confirms update success');

  // Test 3: Up to date (no update available)
  fetchStub = function (url) {
    if (url.includes('update')) {
      return Promise.resolve({
        ok: true,
        status: 200,
        json: () => Promise.resolve({
          current_version: '2.3.0',
          latest_version: '2.3.0',
          update_available: false
        })
      });
    }
    return Promise.resolve({ ok: true, status: 200, json: () => Promise.resolve([]) });
  };

  await checkUpdate();
  assert.strictEqual(btnUpdateEl.hidden, true, 'Update button hidden when up to date');
  assert.ok(updateFooterEl.textContent.includes('2.3.0'), 'Footer reflects current version');

  console.log('PASS: Auto update check and update button verified');

  console.log('\n--- 12. Testing Reactive Ingest, First Paint, and Keyed DOM Reconciliation ---');
  const ingest = testExports.ingest;
  const renderBoard = testExports.renderBoard;
  assert.ok(typeof ingest === 'function', 'ingest is exported');

  // Ingest initializes state.shown and populates board cards without 0 counts
  ingest({
    items: [
      { id: 'item-reconcile-1', title: 'Reconcile Task 1', status: 'created', type: 'task', timestamp: Date.now() },
      { id: 'item-reconcile-2', title: 'Reconcile Task 2', status: 'planned', type: 'task', timestamp: Date.now() }
    ]
  });
  assert.strictEqual(testExports.state.shown.length, 2, 'state.shown immediately populated on ingest');
  assert.strictEqual(testExports.els.countCreated.textContent, '1', 'Created column count updated');
  assert.strictEqual(testExports.els.countPlanned.textContent, '1', 'Planned column count updated');

  const createdCol = documentMock.querySelector('.board-col-cards[data-drop-status="created"]');
  const initialCard1 = createdCol.querySelector('.board-card[data-id="item-reconcile-1"]');
  assert.ok(initialCard1, 'Initial card 1 present in created column');

  // Keyed DOM reconciliation: re-running renderBoard preserves exact card node when renderKey unchanged
  renderBoard();
  const preservedCard1 = createdCol.querySelector('.board-card[data-id="item-reconcile-1"]');
  assert.strictEqual(preservedCard1, initialCard1, 'Existing card element identity preserved across render cycles');

  // Title change triggers in-place card replacement with updated content
  testExports.state.items[0].title = 'Reconcile Task 1 Renamed';
  renderBoard();
  const updatedCard1 = createdCol.querySelector('.board-card[data-id="item-reconcile-1"]');
  assert.ok(updatedCard1, 'Updated card 1 exists');
  const titleSpan = updatedCard1.querySelector('.board-card-title');
  assert.ok(titleSpan && titleSpan.textContent.includes('Reconcile Task 1 Renamed'), 'Card title reflects updated state');

  // Route synchronization in openAgentDrawer: updates location.hash to target item
  mockWindow.location.hash = '#/item/item-reconcile-1';
  testExports.openAgentDrawer('item-reconcile-2');
  assert.ok(mockWindow.location.hash.includes('item-reconcile-2'), 'openAgentDrawer updates location.hash to target item');
  assert.strictEqual(testExports.state.currentStreamingId, 'item-reconcile-2', 'currentStreamingId matches target item');
  testExports.closeAgentDrawer();

  console.log('PASS: Reactive ingest, first paint, and keyed reconciliation verified');

  console.log('\n--- 13. Testing Unified SSE Event Bus and Synchronous Routing ---');
  const setRoute = testExports.setRoute;
  assert.ok(typeof setRoute === 'function', 'setRoute is exported');

  // Test synchronous route selection
  setRoute('item-target-123');
  assert.strictEqual(testExports.state.selected, 'item-target-123', 'state.selected updated synchronously by setRoute');

  // Test connectEvents and SSE message handling
  let sseInstance = null;
  global.EventSource = function (url) {
    this.url = url;
    this.onmessage = null;
    this.onerror = null;
    this.close = () => {};
    sseInstance = this;
  };

  const connectEvents = testExports.connectEvents;
  const sseSource = connectEvents();
  assert.ok(sseSource, 'connectEvents returns EventSource instance');
  assert.ok(sseSource.url.endsWith('events'), 'EventSource connects to /events');

  let refreshedWithForce = false;
  fetchStub = function (url, opts) {
    if (url.includes('data.json')) {
      if (!opts || !opts.headers || !opts.headers['If-None-Match']) {
        refreshedWithForce = true;
      }
      return Promise.resolve({
        ok: true,
        status: 200,
        json: () => Promise.resolve({ items: [] })
      });
    }
    return Promise.resolve({ ok: true, status: 200, json: () => Promise.resolve([]) });
  };

  // Simulate vault_changed event arriving over SSE
  assert.ok(typeof sseSource.onmessage === 'function', 'onmessage handler attached');
  sseSource.onmessage({ data: JSON.stringify({ type: 'vault_changed' }) });
  assert.strictEqual(refreshedWithForce, true, 'vault_changed event triggers refreshData(true) without If-None-Match header');

  // Test PR renderKey expansion in board cards
  const itemWithPr = {
    id: 'item-pr-1',
    title: 'PR Task',
    status: 'done',
    type: 'task',
    revision: 1,
    pr_number: null,
    pr_url: null,
    timestamp: Date.now()
  };
  const keyNoPr = testExports.computeRenderKey(itemWithPr);

  itemWithPr.pr_number = 42;
  itemWithPr.pr_url = 'https://github.com/org/repo/pull/42';
  const keyWithPr = testExports.computeRenderKey(itemWithPr);
  assert.notStrictEqual(keyNoPr, keyWithPr, 'renderKey changes when PR status/number is added');
  assert.ok(keyWithPr.includes('42'), 'renderKey includes pr_number');

  console.log('PASS: Unified SSE event bus and synchronous routing verified');

  console.log('\n--- 14. Testing Running Agents Table View & Live Duration Ticker ---');
  // Settle any pending SSE refresh from prior test
  await new Promise(r => setTimeout(r, 20));


  const viewAgentsBtn = documentMock.getElementById('view-agents-btn');
  const viewBoardBtn = documentMock.getElementById('view-board-btn');
  const viewDetailBtn = documentMock.getElementById('view-detail-btn');
  const agentsCountBadge = documentMock.getElementById('agents-count-badge');
  const agentsSection = documentMock.getElementById('agents');
  const boardSection = documentMock.getElementById('board');
  const workspaceEl = documentMock.getElementById('workspace');
  const agentsTableBody = documentMock.getElementById('agents-table-body');
  const agentsEmpty = documentMock.getElementById('agents-empty');
  const agentsTableWrap = documentMock.getElementById('agents-table-wrap');

  // Verify DOM elements exist
  assert.ok(viewAgentsBtn, 'view-agents-btn exists in topbar');
  assert.ok(agentsCountBadge, 'agents-count-badge exists');
  assert.ok(agentsSection, 'agents view section exists');
  assert.ok(agentsTableBody, 'agents-table-body exists');
  assert.ok(agentsEmpty, 'agents-empty exists');

  // Test 1: Switching to agents view via setViewMode('agents')
  testExports.setViewMode('agents');
  assert.strictEqual(testExports.state.viewMode, 'agents', 'viewMode set to agents');
  assert.ok(workspaceEl.classList.contains('mode-agents'), 'workspace has mode-agents class');
  assert.strictEqual(viewAgentsBtn.getAttribute('aria-pressed'), 'true', 'Agents button aria-pressed true');
  assert.strictEqual(viewBoardBtn.getAttribute('aria-pressed'), 'false', 'Board button aria-pressed false');
  assert.strictEqual(viewDetailBtn.getAttribute('aria-pressed'), 'false', 'Detail button aria-pressed false');
  assert.strictEqual(agentsSection.hidden, false, 'agents section visible');
  assert.strictEqual(boardSection.hidden, true, 'board section hidden');

  // Test 2: Navigating via route #/agents
  win.location.hash = '#/agents';
  testExports.setRoute('agents');
  assert.strictEqual(testExports.state.viewMode, 'agents', 'setRoute("agents") sets viewMode to agents');
  assert.strictEqual(testExports.state.selected, null, 'setRoute("agents") leaves state.selected null');
  assert.strictEqual(win.location.hash, '#/agents', 'location.hash is #/agents');

  // Test 3: Empty state when no agents are running
  testExports.state.activeRunsInfo = [];
  testExports.state.activeRuns = new Set();
  testExports.renderAgentsTable();
  assert.strictEqual(agentsEmpty.hidden, false, 'agents-empty shown when no runs active');
  assert.ok(agentsEmpty.textContent.includes('No agents currently running'), 'empty state message text correct');
  assert.strictEqual(agentsTableWrap.hidden, true, 'agents-table-wrap hidden when no runs');
  assert.strictEqual(agentsTableBody.children.length, 0, 'no rows in table body');

  // Test 4: Duration ticker formatting
  const nowSec = Math.floor(Date.now() / 1000);
  assert.strictEqual(testExports.formatDuration(nowSec), '00:00', 'formatDuration 0 elapsed');
  assert.strictEqual(testExports.formatDuration(nowSec - 65), '01:05', 'formatDuration 65s elapsed');
  assert.strictEqual(testExports.formatDuration(nowSec - 3661), '61:01', 'formatDuration >1hr elapsed');

  // Test 5: Populating active runs and rendering table rows
  const testItems = [
    { id: 'item-agent-1', title: 'Implement agent feature', claimed_by: '@agent:omp', type: 'task' },
    { id: 'item-agent-2', title: 'Explore architecture idea', claimed_by: 'alice', type: 'idea' }
  ];
  testExports.ingest({ items: testItems });

  const nowMs = Date.now();
  const mockRuns = [
    {
      item_id: 'item-agent-1',
      mode: 'implement',
      is_primary_worktree: false,
      worktree_path: '/path/to/.pin_worktrees/item-agent-1',
      started_at: Math.floor(nowMs / 1000) - 120
    },
    {
      item_id: 'item-agent-2',
      mode: 'idea',
      is_primary_worktree: true,
      worktree_path: null,
      started_at: Math.floor(nowMs / 1000) - 45
    }
  ];

  fetchStub = function (url) {
    if (url.includes('runs')) {
      return Promise.resolve({
        ok: true,
        status: 200,
        json: () => Promise.resolve({
          running: ['item-agent-1', 'item-agent-2'],
          timings: { 'item-agent-1': mockRuns[0].started_at, 'item-agent-2': mockRuns[1].started_at },
          runs: mockRuns
        })
      });
    }
    if (url.includes('data.json')) {
      return Promise.resolve({ ok: true, status: 200, json: () => Promise.resolve({ items: testItems }) });
    }
    return Promise.resolve({ ok: true, status: 200, json: () => Promise.resolve([]) });
  };

  await testExports.updateActiveRuns();
  assert.strictEqual(agentsCountBadge.hidden, false, 'agents-count-badge visible when runs active');
  assert.strictEqual(agentsCountBadge.textContent, '2', 'agents-count-badge displays 2');
  assert.strictEqual(agentsEmpty.hidden, true, 'agents-empty hidden when runs active');
  assert.strictEqual(agentsTableWrap.hidden, false, 'table wrap visible when runs active');
  assert.strictEqual(agentsTableBody.children.length, 2, '2 rows rendered in agents table');

  // Verify Row 1 contents
  const row1 = agentsTableBody.children[0];
  const itemBtn1 = row1.querySelector('.agent-item-link');
  assert.ok(itemBtn1, 'Row 1 item link exists');
  assert.ok(itemBtn1.textContent.includes('item-age'), 'Row 1 has short id');
  assert.ok(itemBtn1.textContent.includes('Implement agent feature'), 'Row 1 has title');
  const modeBadge1 = row1.querySelector('.badge-mode-implement');
  assert.ok(modeBadge1, 'Row 1 has implement mode badge');
  assert.strictEqual(modeBadge1.textContent, 'Implement', 'Row 1 mode badge text');
  const wt1 = row1.querySelector('.badge-wt-isolated');
  assert.ok(wt1, 'Row 1 worktree target is isolated');
  assert.ok(wt1.title.includes('item-agent-1'), 'Row 1 worktree path in title');
  const actor1 = row1.querySelector('.agent-actor');
  assert.strictEqual(actor1.textContent, '@agent:omp', 'Row 1 actor is @agent:omp');
  const ticker1 = row1.querySelector('.agent-duration-ticker');
  assert.ok(ticker1, 'Row 1 has duration ticker');
  assert.strictEqual(ticker1.textContent, '02:00', 'Row 1 duration ticker 02:00');

  // Verify Row 2 contents (Idea mode)
  const row2 = agentsTableBody.children[1];
  const modeBadge2 = row2.querySelector('.badge-mode-idea');
  assert.ok(modeBadge2, 'Row 2 has idea mode badge');
  assert.strictEqual(modeBadge2.textContent, 'Idea', 'Row 2 mode badge text');
  const wt2 = row2.querySelector('.badge-wt-primary');
  assert.ok(wt2, 'Row 2 worktree target is primary');
  assert.strictEqual(wt2.textContent, 'Primary', 'Row 2 primary target text');
  const actor2 = row2.querySelector('.agent-actor');
  assert.strictEqual(actor2.textContent, 'alice', 'Row 2 actor is alice');
  const openChatBtn2 = row2.querySelector('.btn-open-chat');
  assert.ok(openChatBtn2, 'Row 2 has Open Chat button for Idea mode');

  // Test 6: "View Run" button opens trajectory drawer
  const viewRunBtn1 = row1.querySelector('.btn-view-run');
  assert.ok(viewRunBtn1, 'View Run button exists on row 1');
  viewRunBtn1.click();
  assert.strictEqual(testExports.state.viewMode, 'detail', 'Clicking View Run switches to detail view');
  assert.strictEqual(testExports.state.readerTab, 'trajectory', 'Clicking View Run activates trajectory tab');
  assert.strictEqual(testExports.state.currentStreamingId, 'item-agent-1', 'Streaming ID set to item-agent-1');

  // Return to agents view
  testExports.setViewMode('agents');
  assert.strictEqual(testExports.state.viewMode, 'agents');

  // Test 7: "Cancel" button sends POST /items/:id/cancel
  let cancelCalledUrl = null;
  let cancelMethod = null;
  fetchStub = function (url, opts) {
    if (url.includes('cancel')) {
      cancelCalledUrl = url;
      cancelMethod = opts && opts.method;
      return Promise.resolve({
        ok: true,
        status: 200,
        json: () => Promise.resolve({ status: 'cancelled' })
      });
    }
    if (url.includes('runs')) {
      return Promise.resolve({
        ok: true,
        status: 200,
        json: () => Promise.resolve({
          running: ['item-agent-2'],
          timings: { 'item-agent-2': mockRuns[1].started_at },
          runs: [mockRuns[1]]
        })
      });
    }
    return Promise.resolve({ ok: true, status: 200, json: () => Promise.resolve([]) });
  };

  const cancelBtn1 = row1.querySelector('.btn-cancel-run');
  assert.ok(cancelBtn1, 'Cancel button exists on row 1');
  cancelBtn1.click();

  // Wait a tick for promise resolution
  await new Promise(r => setTimeout(r, 10));
  assert.ok(cancelCalledUrl && cancelCalledUrl.includes('items/item-agent-1/cancel'), 'Cancel button called POST /items/:id/cancel');
  assert.strictEqual(cancelMethod, 'POST', 'Cancel request method was POST');
  assert.strictEqual(agentsTableBody.children.length, 1, 'Table updated after cancellation, now 1 row');
  assert.strictEqual(agentsCountBadge.textContent, '1', 'Badge updated after cancellation to 1');

  // Test 8: Live Duration Ticker updates textContent without replacing elements
  testExports.updateAgentDurations();
  const tickerRemaining = agentsTableBody.children[0].querySelector('.agent-duration-ticker');
  assert.ok(tickerRemaining, 'Ticker element still present');

  // Test 9: Reactive SSE run_finished event completes last agent
  fetchStub = function (url) {
    if (url.includes('runs')) {
      return Promise.resolve({
        ok: true,
        status: 200,
        json: () => Promise.resolve({ running: [], timings: {}, runs: [] })
      });
    }
    return Promise.resolve({ ok: true, status: 200, json: () => Promise.resolve({ items: [] }) });
  };

  sseSource.onmessage({ data: JSON.stringify({ type: 'run_finished', item_id: 'item-agent-2' }) });
  await new Promise(r => setTimeout(r, 10));

  assert.strictEqual(agentsEmpty.hidden, false, 'agents-empty visible after SSE run_finished');
  assert.strictEqual(agentsCountBadge.hidden, true, 'agentsCountBadge hidden when 0 runs');
  assert.strictEqual(agentsTableBody.children.length, 0, 'table body empty after last run finished');

  console.log('PASS: Running agents table view, live ticker, drawer opening, cancel dispatch, and SSE reactivity verified');
  console.log('\n========================================');
  console.log('ALL VIEWER TRAJECTORY TESTS PASSED (0)');
  console.log('========================================');
}

runHarness()
  .then(() => {
    process.exit(0);
  })
  .catch(err => {
    console.error('FAIL in viewer trajectory harness:', err);
    process.exit(1);
  });
