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
      this.dispatchEvent({ type: 'click', target: this });
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
    return createMockElement('', 'div');
  },
  querySelectorAll(sel) {
    if (sel.startsWith('#')) return [getOrCreateElement(sel.slice(1))];
    return [createMockElement('', 'div')];
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

  console.log('\n--- 9. Testing Handoff Card Layout and Priority ---');
  const renderHandoff = testExports.renderHandoff || win.renderHandoff;
  assert.ok(typeof renderHandoff === 'function', 'renderHandoff is exported');
  const handoffCardEl = documentMock.getElementById('handoff-card');

  const reviewItem = {
    id: 'rev-item-1',
    status: 'review',
    handoff: {
      progress: '1. Added feature XYZ\n2. Refactored module',
      verification: 'Cargo test passed 51/51 unit tests'
    }
  };
  renderHandoff(reviewItem);
  assert.strictEqual(handoffCardEl.hidden, false);
  const fields = handoffCardEl.children.filter(c => c.classList && c.classList.contains('handoff-field'));
  assert.strictEqual(fields.length, 2);
  assert.ok(fields[0].classList.contains('handoff-field-verification'), 'First block is verification for review item');
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

  const reviewCard = buildBoardCard({ id: 'r1', title: 'Review', status: 'review', type: 'task', timestamp: Date.now() });
  assert.ok(reviewCard.querySelector('.btn-commit-pr'), 'Review card has Commit + PR');

  const doneCard = buildBoardCard({ id: 'd1', title: 'Done', status: 'done', type: 'task', timestamp: Date.now() });
  assert.ok(doneCard.querySelector('.btn-commit-pr'), 'Done card has Commit + PR');

  const plannedCard = buildBoardCard({ id: 'p1', title: 'Planned', status: 'planned', type: 'task', timestamp: Date.now() });
  assert.strictEqual(plannedCard.querySelector('.btn-commit-pr'), null, 'Planned card lacks Commit + PR');

  // Merged review card suppresses Commit + PR and shows indicator
  const reviewMerged = buildBoardCard({ id: 'rm1', title: 'Merged', status: 'review', type: 'task', git_status: 'merged', timestamp: Date.now() });
  assert.strictEqual(reviewMerged.querySelector('.btn-commit-pr'), null, 'Merged review card suppresses Commit + PR');
  assert.ok(reviewMerged.querySelector('.indicator-merged'), 'Merged review card has indicator');
  assert.strictEqual(reviewMerged.querySelector('.indicator-merged').textContent, 'Merged into main');

  // Open PR review card suppresses Commit + PR and shows PR link
  const reviewPr = buildBoardCard({ id: 'rp1', title: 'Open PR', status: 'review', type: 'task', git_status: 'pr_open', pr_url: 'https://github.com/JoeriKaiser/pin/pull/42', pr_number: 42, timestamp: Date.now() });
  assert.strictEqual(reviewPr.querySelector('.btn-commit-pr'), null, 'Open PR card suppresses Commit + PR');
  const prLink = reviewPr.querySelector('.btn-pr-link');
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
  console.log('\n========================================');
  console.log('ALL VIEWER TRAJECTORY TESTS PASSED (0)');
  console.log('========================================');
}

runHarness().catch(err => {
  console.error('FAIL in viewer trajectory harness:', err);
  process.exit(1);
});
