#!/usr/bin/env node
'use strict';

/**
 * tests/viewer_trajectory.js
 *
 * Verification test for pin view's DSH trajectory interface and ACP streaming events.
 * 1. Verifies backward-compatible string contracts required by tests/cli.sh.
 * 2. Unit tests extracted functions: stripAnsi, cleanThoughtText, and inline tool formatting.
 * 3. Simulates ACP event stream processing in a lightweight mock DOM:
 *    agent_thought_chunk, tool_call, tool_call_update, finished.
 * 4. Validates metrics (duration, turns, calls), swimlane SVG rendering, role badges,
 *    and tool inspector interactions.
 */

const fs = require('fs');
const path = require('path');
const assert = require('assert');
const vm = require('vm');

const appJsPath = path.resolve(__dirname, '../src/assets/app.js');
if (!fs.existsSync(appJsPath)) {
  console.error(`Error: app.js not found at ${appJsPath}`);
  process.exit(1);
}
const appJs = fs.readFileSync(appJsPath, 'utf8');

console.log('--- 1. Verifying String Contracts ---');
const requiredStrings = [
  'cleanThoughtText',
  'stripAnsi',
  'card.open = false;',
  'EventSource',
  'agent-trajectory-stream',
  'agent-swimlane-svg',
  'agent-tool-inspector'
];

for (const s of requiredStrings) {
  if (!appJs.includes(s)) {
    console.error(`FAIL: Missing required string contract in app.js: "${s}"`);
    process.exit(1);
  }
}
console.log('PASS: All string contracts present in app.js');

console.log('\n--- 2. Testing Extracted Functions ---');

// Extract stripAnsi
const stripAnsiMatch = appJs.match(/function stripAnsi\(str\) \{([\s\S]*?)\n  \}/);
if (!stripAnsiMatch) {
  console.error('FAIL: function stripAnsi(str) definition not found');
  process.exit(1);
}
const stripAnsi = new Function('str', stripAnsiMatch[1]);

// Test stripAnsi
assert.strictEqual(stripAnsi('\x1b[32mChecking code\x1b[0m'), 'Checking code');
assert.strictEqual(stripAnsi('\u001b[1;31mError\u001b[0m: \x1b[33mWarning\x1b[0m'), 'Error: Warning');
assert.strictEqual(stripAnsi('plain string'), 'plain string');
assert.strictEqual(stripAnsi(''), '');
assert.strictEqual(stripAnsi(null), '');
assert.strictEqual(stripAnsi(undefined), '');
console.log('PASS: stripAnsi verified on ANSI codes and edge cases');

// Extract cleanThoughtText
const cleanThoughtMatch = appJs.match(/function cleanThoughtText\(thoughtText\) \{([\s\S]*?)\n  \}/);
if (!cleanThoughtMatch) {
  console.error('FAIL: function cleanThoughtText(thoughtText) definition not found');
  process.exit(1);
}
const cleanThoughtText = new Function('thoughtText', 'stripAnsi', cleanThoughtMatch[1]);
const cleanThought = (t) => cleanThoughtText(t, stripAnsi);

// Test cleanThoughtText
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
console.log('PASS: cleanThoughtText verified on think/thought tags, ANSI codes, and newlines');

console.log('\n--- 3. Setting Up Mock DOM Environment ---');

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
  readyState: 'loading', // Prevents automatic boot()
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

// Create sandbox
const sandbox = {
  window: null,
  document: documentMock,
  DOMPurify: { sanitize: (s) => s },
  marked: { parse: (s) => s },
  EventSource: function(url) { this.url = url; this.addEventListener = () => {}; this.close = () => {}; },
  fetch: () => Promise.resolve({ ok: true, json: () => Promise.resolve([]) }),
  navigator: { clipboard: { writeText: () => Promise.resolve() } },
  requestAnimationFrame: (cb) => setTimeout(cb, 0),
  cancelAnimationFrame: (id) => clearTimeout(id),
  setInterval: () => 123,
  clearInterval: () => {},
  setTimeout: () => 456,
  clearTimeout: () => {},
  addEventListener: () => {},
  removeEventListener: () => {},
  console: console
};
sandbox.window = sandbox;
sandbox.global = sandbox;

const context = vm.createContext(sandbox);
vm.runInContext(appJs, context);

const win = context.window;
const testExports = win._test || {};
if (typeof testExports.bind === 'function') {
  testExports.bind();
}
console.log('PASS: Mock DOM and app.js loaded in sandbox');

console.log('\n--- 4. Testing Inline Tool Formatting Logic ---');

const formatToolCallInline = win.formatToolCallInline || testExports.formatToolCallInline;
if (typeof formatToolCallInline !== 'function') {
  console.error('FAIL: formatToolCallInline is not exposed on window or window._test');
  process.exit(1);
}

// 1. Command extraction from tool args
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
assert.strictEqual(formattedBash.status, 'running', 'Expected status to be "running"');

// 2. Arguments truncation
const longArgCall = {
  name: 'grep',
  args: {
    pattern: 'A'.repeat(200),
    path: 'B'.repeat(200)
  },
  status: 'running'
};
const formattedLong = formatToolCallInline(longArgCall);
assert.ok(
  (formattedLong.name && formattedLong.name.includes('grep')) || formattedLong.argsStr,
  'grep call formatted'
);
assert.ok(
  (formattedLong.name && (formattedLong.name.length <= 150 || formattedLong.name.includes('...'))) ||
  (formattedLong.argsStr && (formattedLong.argsStr.length <= 150 || formattedLong.argsStr.includes('...'))),
  'Long arguments/invocation should be capped or truncated in inline view'
);

// 3. Status handling: completed, failed
const completedCall = {
  name: 'read',
  args: { path: 'file.txt' },
  status: 'completed',
  result: 'file contents'
};
const formattedCompleted = formatToolCallInline(completedCall);
assert.strictEqual(formattedCompleted.status, 'completed');

const failedCall = {
  name: 'eval',
  args: { code: '1/0' },
  status: 'failed',
  error: 'ZeroDivisionError'
};
const formattedFailed = formatToolCallInline(failedCall);
assert.strictEqual(formattedFailed.status, 'failed');

console.log('PASS: formatToolCallInline correctly handles commands, truncation, and statuses');

console.log('\n--- 5. Simulating ACP Event Stream Processing ---');

const openAgentDrawer = win.openAgentDrawer;
const handleAgentStreamEvent = win.handleAgentStreamEvent;
const closeAgentDrawer = win.closeAgentDrawer;

if (typeof openAgentDrawer !== 'function' || typeof handleAgentStreamEvent !== 'function') {
  console.error('FAIL: openAgentDrawer or handleAgentStreamEvent not exposed');
  process.exit(1);
}

const streamEl = documentMock.getElementById('agent-trajectory-stream');
const turnsMetricEl = documentMock.getElementById('agent-metric-turns');
const callsMetricEl = documentMock.getElementById('agent-metric-calls');
const durationMetricEl = documentMock.getElementById('agent-metric-duration');
const swimlaneSvgEl = documentMock.getElementById('agent-swimlane-svg');
const inspectorEl = documentMock.getElementById('agent-tool-inspector');
const inspectorTitleEl = documentMock.getElementById('inspector-title');
const inspectorArgsEl = documentMock.getElementById('inspector-args');
const inspectorOutputEl = documentMock.getElementById('inspector-output');
const statusPillEl = documentMock.getElementById('agent-status-pill');

// 5.1 Initialize drawer for item
openAgentDrawer('item-trajectory-test');

// Verify initial state
assert.strictEqual(inspectorEl.hidden, true, 'Inspector should be hidden initially');
assert.ok(
  streamEl.children.length >= 1,
  'Trajectory stream should contain initial CONTEXT start row'
);

const initialContextRow = streamEl.children[0];
const initialBadge = findDescendant(initialContextRow, '.badge-context') ||
  findDescendant(initialContextRow, '.trajectory-badge');
assert.ok(initialBadge, 'Initial row has badge');
assert.strictEqual(
  initialBadge.textContent.trim().toUpperCase(),
  'CONTEXT',
  'Initial row role badge is CONTEXT'
);
assert.ok(
  initialContextRow.textContent.includes('item-trajectory-test') ||
  initialContextRow.textContent.includes('Agent run started'),
  'Initial row describes run start'
);

// Metrics reset
assert.ok(
  turnsMetricEl.textContent.includes('0'),
  `Turns should start at 0, got "${turnsMetricEl.textContent}"`
);
assert.ok(
  callsMetricEl.textContent.includes('0'),
  `Calls should start at 0, got "${callsMetricEl.textContent}"`
);

console.log('PASS: openAgentDrawer resets state, metrics, and logs CONTEXT start row');

// 5.2 Simulate agent_thought_chunk (Turn 1 begins)
handleAgentStreamEvent({
  type: 'agent_thought_chunk',
  update: 'Analyzing the repository structure and workflow files.\n'
});

// Verify ASSISTANT row created
assert.ok(
  turnsMetricEl.textContent.includes('1'),
  `Turns metric should be 1, got "${turnsMetricEl.textContent}"`
);

let assistantRows = findAllDescendants(streamEl, '.trajectory-row-assistant');
if (assistantRows.length === 0) {
  // Check rows by badge if class is directly on child
  assistantRows = streamEl.children.filter(r => {
    const badge = findDescendant(r, '.badge-assistant');
    return !!badge;
  });
}
assert.strictEqual(assistantRows.length, 1, 'Exactly one ASSISTANT row created');
const assistantBadge = findDescendant(assistantRows[0], '.badge-assistant');
assert.ok(assistantBadge, 'ASSISTANT row has badge-assistant');
assert.strictEqual(assistantBadge.textContent.trim().toUpperCase(), 'ASSISTANT');
assert.ok(
  assistantRows[0].textContent.includes('Analyzing the repository structure'),
  'ASSISTANT row contains thought text'
);

// 5.3 Consecutive agent_thought_chunk should accumulate into same turn
handleAgentStreamEvent({
  type: 'agent_thought_chunk',
  update: 'Found relevant configuration in .github/workflows.\n'
});

assert.ok(
  turnsMetricEl.textContent.includes('1'),
  `Turns metric should remain 1 on consecutive chunk, got "${turnsMetricEl.textContent}"`
);
assert.ok(
  assistantRows[0].textContent.includes('Found relevant configuration'),
  'Thought text accumulated into same ASSISTANT row'
);
console.log('PASS: agent_thought_chunk accumulated into single ASSISTANT turn');

// 5.4 Simulate tool_call
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

assert.ok(
  callsMetricEl.textContent.includes('1'),
  `Calls metric should increment to 1, got "${callsMetricEl.textContent}"`
);

let toolRows = findAllDescendants(streamEl, '.trajectory-row-tool');
if (toolRows.length === 0) {
  toolRows = streamEl.children.filter(r => {
    const badge = findDescendant(r, '.badge-tool');
    return !!badge;
  });
}
assert.strictEqual(toolRows.length, 1, 'Exactly one TOOL row created');
const toolBadge = findDescendant(toolRows[0], '.badge-tool');
assert.ok(toolBadge, 'TOOL row has badge-tool');
assert.strictEqual(toolBadge.textContent.trim().toUpperCase(), 'TOOL');
assert.ok(
  toolRows[0].textContent.includes('bash') || toolRows[0].textContent.includes('git status'),
  'TOOL row contains tool name or command'
);
console.log('PASS: tool_call ended assistant turn, incremented calls, added TOOL row');

// 5.5 Simulate tool_call_update (completion)
handleAgentStreamEvent({
  type: 'tool_call_update',
  update: {
    toolCallId: 'call_bash_1',
    status: 'completed',
    output: 'M src/assets/app.js\n?? tests/viewer_trajectory.js\n'
  }
});

// Verify status badge update or preview update
const statusBadge = findDescendant(toolRows[0], '.status-completed') ||
  findDescendant(toolRows[0], '.tool-status-badge');
if (statusBadge) {
  assert.ok(
    statusBadge.textContent.toLowerCase().includes('completed') ||
    statusBadge.classList.contains('status-completed') ||
    statusBadge.className.includes('completed'),
    'Tool status badge reflects completion'
  );
}
console.log('PASS: tool_call_update updated tool execution status');

// 5.6 Simulate Tool Inspector interaction
// Clicking the tool line or calling openToolInspector
const toolLine = findDescendant(toolRows[0], '.trajectory-tool-line') || toolRows[0];
toolLine.click();

assert.strictEqual(inspectorEl.hidden, false, 'Clicking tool row opens inspector');
assert.ok(
  inspectorArgsEl.textContent.includes('git status'),
  `Inspector args should contain tool args, got "${inspectorArgsEl.textContent}"`
);
assert.ok(
  inspectorOutputEl.textContent.includes('src/assets/app.js'),
  `Inspector output should contain tool output, got "${inspectorOutputEl.textContent}"`
);

const closeInspectorBtn = documentMock.getElementById('btn-close-inspector');
if (closeInspectorBtn) {
  closeInspectorBtn.click();
  assert.strictEqual(inspectorEl.hidden, true, 'Close inspector button hides inspector');
}
console.log('PASS: Tool inspector opens on row click and displays args & output');

// 5.7 Simulate second thought chunk (Turn 2 begins)
handleAgentStreamEvent({
  type: 'agent_thought_chunk',
  update: 'Git status verified. Now closing the run.\n'
});

assert.ok(
  turnsMetricEl.textContent.includes('2'),
  `Turns metric should increment to 2, got "${turnsMetricEl.textContent}"`
);
console.log('PASS: New turn started after tool execution');

// 5.8 Simulate finished event
handleAgentStreamEvent({
  type: 'finished',
  update: { success: true }
});

const lastRow = streamEl.children[streamEl.children.length - 1];
const finalBadge = findDescendant(lastRow, '.badge-context') ||
  findDescendant(lastRow, '.trajectory-badge');
assert.ok(finalBadge, 'Finished row has badge');
assert.strictEqual(
  finalBadge.textContent.trim().toUpperCase(),
  'CONTEXT',
  'Finished row role badge is CONTEXT'
);
assert.ok(
  lastRow.textContent.includes('Agent run completed') ||
  lastRow.textContent.includes('completed'),
  'Finished row notes completion'
);
console.log('PASS: finished event logged CONTEXT completion row');

// 5.9 Verify Swimlane SVG rendered lanes and bars
const swimlaneContent = swimlaneSvgEl.innerHTML ||
  (swimlaneSvgEl.children && swimlaneSvgEl.children.map(c => c.innerHTML || c.textContent).join('')) ||
  '';
const swimlaneText = swimlaneSvgEl.textContent || swimlaneContent;

assert.ok(
  swimlaneContent.includes('Input') || swimlaneText.includes('Input'),
  'Swimlane includes Input lane'
);
assert.ok(
  swimlaneContent.includes('Model') || swimlaneText.includes('Model'),
  'Swimlane includes Model lane'
);
assert.ok(
  swimlaneContent.includes('Tools') || swimlaneText.includes('Tools'),
  'Swimlane includes Tools lane'
);
console.log('PASS: Swimlane SVG rendered Input, Model, and Tools lanes');

// 5.10 Verify search filtering on trajectory stream
const searchInput = documentMock.getElementById('agent-trajectory-search');
if (searchInput) {
  searchInput.value = 'git status';
  searchInput.dispatchEvent({ type: 'input', target: searchInput });

  // Rows that do not match should be hidden
  const nonMatchingAssistant = assistantRows[0];
  assert.strictEqual(
    nonMatchingAssistant.hidden,
    true,
    'Non-matching assistant row should be hidden on search filter'
  );

  const matchingTool = toolRows[0];
  assert.strictEqual(
    matchingTool.hidden,
    false,
    'Matching tool row should remain visible on search filter'
  );

  // Clear search
  searchInput.value = '';
  searchInput.dispatchEvent({ type: 'input', target: searchInput });
  assert.strictEqual(
    nonMatchingAssistant.hidden,
    false,
    'Clearing search unhides rows'
  );
  console.log('PASS: Trajectory search correctly filters rows');
}

// 5.11 Close drawer
if (typeof closeAgentDrawer === 'function') {
  closeAgentDrawer();
}

console.log('\n--- 6. Testing Spec Ticket Editor Logic ---');

const openSpecModal = win.openSpecModal || testExports.openSpecModal;
const closeSpecModal = win.closeSpecModal || testExports.closeSpecModal;
const setSpecModalTab = win.setSpecModalTab || testExports.setSpecModalTab;
const expandQuickAdd = win.expandQuickAdd || testExports.expandQuickAdd;
const collapseQuickAdd = win.collapseQuickAdd || testExports.collapseQuickAdd;
const SPEC_SNIPPETS = win.SPEC_SNIPPETS || testExports.SPEC_SNIPPETS;

assert.ok(typeof openSpecModal === 'function', 'openSpecModal is exported');
assert.ok(typeof closeSpecModal === 'function', 'closeSpecModal is exported');
assert.ok(typeof setSpecModalTab === 'function', 'setSpecModalTab is exported');
assert.ok(typeof expandQuickAdd === 'function', 'expandQuickAdd is exported');
assert.ok(typeof collapseQuickAdd === 'function', 'collapseQuickAdd is exported');
assert.ok(SPEC_SNIPPETS && SPEC_SNIPPETS.ac && SPEC_SNIPPETS.ac.includes('Acceptance Criteria'), 'SPEC_SNIPPETS.ac defined');
assert.ok(SPEC_SNIPPETS && SPEC_SNIPPETS.template && SPEC_SNIPPETS.template.includes('Acceptance Criteria'), 'SPEC_SNIPPETS.template defined');

// 6.1 Test quick add expand / collapse
const quickAddTrigger = documentMock.getElementById('quick-add-trigger');
const quickAddForm = documentMock.getElementById('quick-add-form');

expandQuickAdd();
assert.strictEqual(quickAddTrigger.hidden, true, 'Trigger hidden when quick add expanded');
assert.strictEqual(quickAddForm.hidden, false, 'Form visible when quick add expanded');

collapseQuickAdd();
assert.strictEqual(quickAddTrigger.hidden, false, 'Trigger visible when quick add collapsed');
assert.strictEqual(quickAddForm.hidden, true, 'Form hidden when quick add collapsed');
console.log('PASS: expandQuickAdd and collapseQuickAdd toggle element visibility');

// 6.2 Test openSpecModal and closeSpecModal
const specModal = documentMock.getElementById('spec-modal');
const specModalTitle = documentMock.getElementById('spec-modal-title');
const specModalBody = documentMock.getElementById('spec-modal-body');
const specModalType = documentMock.getElementById('spec-modal-type');
const specModalKind = documentMock.getElementById('spec-modal-kind');
const specModalPriority = documentMock.getElementById('spec-modal-priority');
const specModalTags = documentMock.getElementById('spec-modal-tags');

openSpecModal({
  title: 'Spec Test Title',
  body: '## Description\nDetailed spec description\n\n## Acceptance Criteria\n- [ ] Done',
  type: 'bug',
  kind: 'technical',
  priority: 'high',
  tags: 'test,spec'
});

assert.strictEqual(specModalTitle.value, 'Spec Test Title', 'Modal populated title');
assert.ok(specModalBody.value.includes('Acceptance Criteria'), 'Modal populated body with criteria');
assert.strictEqual(specModalType.value, 'bug', 'Modal populated type');
assert.strictEqual(specModalKind.value, 'technical', 'Modal populated kind');
assert.strictEqual(specModalPriority.value, 'high', 'Modal populated priority');
assert.strictEqual(specModalTags.value, 'test,spec', 'Modal populated tags');
assert.ok(specModal.open || specModal.hasAttribute('open'), 'Modal opened');

closeSpecModal();
assert.ok(!specModal.open && !specModal.hasAttribute('open'), 'Modal closed');
console.log('PASS: openSpecModal initializes form fields and opens/closes modal');

// 6.3 Test tab switching in spec modal
const specTabWrite = documentMock.getElementById('spec-tab-write');
const specTabPreview = documentMock.getElementById('spec-tab-preview');
const specModalPreview = documentMock.getElementById('spec-modal-preview');

openSpecModal({ title: 'Preview Test', body: '### Test Header\n- Item 1' });
setSpecModalTab('preview');
assert.strictEqual(specModalBody.hidden, true, 'Body textarea hidden in preview tab');
assert.strictEqual(specModalPreview.hidden, false, 'Preview pane visible in preview tab');
assert.ok(specTabPreview.classList.contains('active'), 'Preview tab is active');

setSpecModalTab('write');
assert.strictEqual(specModalBody.hidden, false, 'Body textarea visible in write tab');
assert.strictEqual(specModalPreview.hidden, true, 'Preview pane hidden in write tab');
assert.ok(specTabWrite.classList.contains('active'), 'Write tab is active');
console.log('PASS: setSpecModalTab switches write/preview panes');

closeSpecModal();


// 6.4 Test screenshot pasting in ticket description editors
(async () => {
  const getClipboardImages = win.getClipboardImages || testExports.getClipboardImages;
  const handleScreenshotPaste = win.handleScreenshotPaste || testExports.handleScreenshotPaste;
  const uploadScreenshot = win.uploadScreenshot || testExports.uploadScreenshot;

  assert.ok(typeof getClipboardImages === 'function', 'getClipboardImages is exported');
  assert.ok(typeof handleScreenshotPaste === 'function', 'handleScreenshotPaste is exported');
  assert.ok(typeof uploadScreenshot === 'function', 'uploadScreenshot is exported');

  // Test extracting images from clipboard items
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
  assert.strictEqual(extractedImages.length, 1, 'Extracted one image from clipboard items');
  assert.strictEqual(extractedImages[0], fakeImageFile, 'Extracted image matches fake file');

  // Test clipboard with no images (plain text only)
  const mockClipboardTextOnly = {
    clipboardData: {
      items: [
        { type: 'text/plain', getAsFile: () => null }
      ]
    }
  };
  assert.strictEqual(getClipboardImages(mockClipboardTextOnly).length, 0, 'No images from text-only clipboard');

  // Mock fetch for upload
  const originalFetch = win.fetch;
  win.fetch = function(url, options) {
    if (url.includes('screenshots')) {
      return Promise.resolve({
        ok: true,
        json: () => Promise.resolve({
          path: '/tmp/pin-screenshots/screenshot-123456.png',
          filename: 'screenshot-123456.png',
          markdown: '![screenshot](/tmp/pin-screenshots/screenshot-123456.png)'
        })
      });
    }
    return originalFetch ? originalFetch(url, options) : Promise.reject(new Error('unmocked fetch'));
  };

  // Test handleScreenshotPaste on quick-add-body
  const quickAddBodyEl = documentMock.getElementById('quick-add-body');
  quickAddBodyEl.value = 'Initial spec description';
  let pastePreventDefaultCalled = false;
  const pasteEventOnQuickAdd = {
    type: 'paste',
    preventDefault() { pastePreventDefaultCalled = true; },
    clipboardData: mockClipboardWithImage.clipboardData,
    currentTarget: quickAddBodyEl
  };

  const handled = handleScreenshotPaste(pasteEventOnQuickAdd, quickAddBodyEl);
  assert.strictEqual(handled, true, 'handleScreenshotPaste handled image paste');
  assert.strictEqual(pastePreventDefaultCalled, true, 'e.preventDefault() called on image paste');
  assert.ok(quickAddBodyEl.value.includes('![Uploading screenshot...]()'), 'Placeholder inserted immediately');

  // Wait for upload promise to resolve
  await new Promise(r => setTimeout(r, 20));
  assert.ok(quickAddBodyEl.value.includes('![screenshot](/tmp/pin-screenshots/screenshot-123456.png)'), 'Placeholder replaced with screenshot markdown');
  assert.ok(!quickAddBodyEl.value.includes('Uploading screenshot'), 'Uploading placeholder removed');

  // Test handleScreenshotPaste on spec-modal-body
  openSpecModal({ title: 'Spec Modal Paste Test', body: 'Spec details' });
  const specModalBodyEl = documentMock.getElementById('spec-modal-body');
  let modalPastePreventDefault = false;
  const pasteEventOnSpecModal = {
    type: 'paste',
    preventDefault() { modalPastePreventDefault = true; },
    clipboardData: mockClipboardWithImage.clipboardData,
    currentTarget: specModalBodyEl
  };

  handleScreenshotPaste(pasteEventOnSpecModal, specModalBodyEl);
  assert.strictEqual(modalPastePreventDefault, true, 'e.preventDefault() called on spec modal paste');
  await new Promise(r => setTimeout(r, 20));
  assert.ok(specModalBodyEl.value.includes('![screenshot](/tmp/pin-screenshots/screenshot-123456.png)'), 'Spec modal body has screenshot markdown');
  closeSpecModal();

  // Test text-only paste does NOT prevent default and does not insert placeholder
  let textPastePreventDefault = false;
  const pasteEventTextOnly = {
    type: 'paste',
    preventDefault() { textPastePreventDefault = true; },
    clipboardData: mockClipboardTextOnly.clipboardData,
    currentTarget: quickAddBodyEl
  };
  const textHandled = handleScreenshotPaste(pasteEventTextOnly, quickAddBodyEl);
  assert.strictEqual(textHandled, false, 'Text-only paste returns false');
  assert.strictEqual(textPastePreventDefault, false, 'e.preventDefault() NOT called on text-only paste');

  win.fetch = originalFetch;
  console.log('PASS: Screenshot paste correctly uploads, inserts markdown reference, and ignores plain text');

  console.log('\n--- 7. Testing Worktree Modal Prompt Logic ---');
  const openWorktreeModal = win.openWorktreeModal || testExports.openWorktreeModal;
  const closeWorktreeModal = win.closeWorktreeModal || testExports.closeWorktreeModal;
  assert.ok(typeof openWorktreeModal === 'function', 'openWorktreeModal is exported');
  assert.ok(typeof closeWorktreeModal === 'function', 'closeWorktreeModal is exported');

  const testItem = { id: 'item-test-123' };
  const modalTitle = win.document.getElementById('worktree-modal-title');
  const busyMsg = win.document.getElementById('worktree-busy-msg');
  const promptMsg = win.document.getElementById('worktree-prompt-msg');
  const primaryBtn = win.document.getElementById('btn-worktree-primary');

  // Manual prompt mode: busyId is null
  openWorktreeModal(testItem, null);
  assert.strictEqual(modalTitle.textContent, 'Start Agent Run', 'Modal title for manual start');
  assert.strictEqual(busyMsg.hidden, true, 'Busy message hidden on manual start');
  assert.strictEqual(promptMsg.hidden, false, 'Prompt message shown on manual start');
  assert.strictEqual(primaryBtn.hidden, false, 'Primary checkout button shown on manual start');

  // Busy mode: busyId is provided
  openWorktreeModal(testItem, 'busy-item-456');
  assert.strictEqual(modalTitle.textContent, 'Primary Working Tree Busy', 'Modal title when busy');
  assert.strictEqual(busyMsg.hidden, false, 'Busy message shown when busy');
  assert.strictEqual(promptMsg.hidden, true, 'Prompt message hidden when busy');
  assert.strictEqual(primaryBtn.hidden, true, 'Primary checkout button hidden when busy');

  closeWorktreeModal();
  console.log('PASS: Worktree modal correctly toggles between manual start prompt and busy state');

  console.log('\n--- 8. Testing Timeline Performance with High Volume of Entries ---');
  openAgentDrawer('perf-test-item');
  const perfEvents = [];
  for (let i = 0; i < 200; i++) {
    perfEvents.push({
      params: {
        update: {
          sessionUpdate: 'agent_thought_chunk',
          content: { type: 'text', text: 'Step ' + i + ' analysis.\n' }
        }
      }
    });
    perfEvents.push({
      params: {
        update: {
          sessionUpdate: 'tool_call',
          id: 'perf_call_' + i,
          title: 'bash: check ' + i,
          rawInput: 'check ' + i
        }
      }
    });
    perfEvents.push({
      params: {
        update: {
          sessionUpdate: 'tool_call_update',
          id: 'perf_call_' + i,
          status: 'completed',
          rawOutput: 'ok ' + i
        }
      }
    });
  }

  const perfStart = Date.now();
  for (const ev of perfEvents) {
    handleAgentStreamEvent(ev);
  }
  const streamElapsed = Date.now() - perfStart;
  assert.ok(streamElapsed < 500, `600 stream events should process in <500ms, took ${streamElapsed}ms`);
  assert.ok(callsMetricEl.textContent.includes('200'), `Expected 200 calls, got "${callsMetricEl.textContent}"`);
  console.log(`PASS: 600 timeline entries processed seamlessly in ${streamElapsed}ms`);

  // Simulate finish and test instantaneous cached open
  handleAgentStreamEvent({ type: 'finished', stopReason: 'completed' });
  closeAgentDrawer();

  const reopenStart = Date.now();
  openAgentDrawer('perf-test-item');
  const reopenElapsed = Date.now() - reopenStart;
  assert.ok(reopenElapsed < 50, `Reopening cached run should be instant (<50ms), took ${reopenElapsed}ms`);
  assert.ok(callsMetricEl.textContent.includes('200'), `Cached drawer preserved 200 calls, got "${callsMetricEl.textContent}"`);
  closeAgentDrawer();
  console.log(`PASS: Reopening completed agent run is seamless and cached (${reopenElapsed}ms)`);
  // --- 9. Testing Commit + PR Quick Action Rendering ---
  console.log('\n--- 9. Testing Commit + PR Quick Action Rendering ---');
  const buildBoardCard = win.buildBoardCard || testExports.buildBoardCard;
  const renderActions = win.renderActions || testExports.renderActions;
  const actionsEl = testExports.els && testExports.els.actions;

  assert.ok(typeof buildBoardCard === 'function', 'buildBoardCard is exposed');
  assert.ok(typeof renderActions === 'function', 'renderActions is exposed');

  // 9.1 Review status board card
  const reviewItem = { id: 'rev123', title: 'Review Task', status: 'review', type: 'task', timestamp: Date.now() };
  const cardReview = buildBoardCard(reviewItem);
  const btnReview = cardReview.querySelector('.btn-commit-pr');
  assert.ok(btnReview, 'Board card in review status must render .btn-commit-pr button');
  assert.strictEqual(btnReview.textContent, 'Commit + PR');

  // 9.2 Done status board card
  const doneItem = { id: 'done123', title: 'Done Task', status: 'done', type: 'task', timestamp: Date.now() };
  const cardDone = buildBoardCard(doneItem);
  const btnDone = cardDone.querySelector('.btn-commit-pr');
  assert.ok(btnDone, 'Board card in done status must render .btn-commit-pr button');
  assert.strictEqual(btnDone.textContent, 'Commit + PR');

  // 9.3 Planned status board card does NOT have commit+pr
  const plannedItem = { id: 'plan123', title: 'Planned Task', status: 'planned', type: 'task', timestamp: Date.now() };
  const cardPlanned = buildBoardCard(plannedItem);
  assert.strictEqual(cardPlanned.querySelector('.btn-commit-pr'), null, 'Planned card must not render .btn-commit-pr');

  // 9.4 Active run replaces commit+pr with running indicator
  testExports.state.activeRuns.add('rev123');
  const cardRunningReview = buildBoardCard(reviewItem);
  assert.strictEqual(cardRunningReview.querySelector('.btn-commit-pr'), null, 'Running card must not render .btn-commit-pr');
  assert.ok(cardRunningReview.querySelector('.card-running-indicator'), 'Running card renders indicator');
  testExports.state.activeRuns.delete('rev123');

  // 9.5 renderActions renders Commit + PR button for review and done
  if (actionsEl) {
    renderActions(reviewItem);
    const actionBtnsReview = Array.from(actionsEl.querySelectorAll('button'));
    assert.ok(actionBtnsReview.some(b => b.textContent === 'Commit + PR'), 'Review actions must include Commit + PR');

    renderActions(doneItem);
    const actionBtnsDone = Array.from(actionsEl.querySelectorAll('button'));
    assert.ok(actionBtnsDone.some(b => b.textContent === 'Commit + PR'), 'Done actions must include Commit + PR');

    renderActions(plannedItem);
    const actionBtnsPlanned = Array.from(actionsEl.querySelectorAll('button'));
    assert.ok(!actionBtnsPlanned.some(b => b.textContent === 'Commit + PR'), 'Planned actions must not include Commit + PR');
  }

  console.log('PASS: Commit + PR buttons correctly rendered on cards and actions panel');

  console.log('\n--- 10. Testing Handoff Card and Actions Priority & Layout ---');
  // 10.1 Verify HTML layout order: proposal-actions precedes handoff-card in inspector-rail
  const indexHtmlPath = path.resolve(__dirname, '../src/assets/index.html');
  const indexHtml = fs.readFileSync(indexHtmlPath, 'utf8');
  const actionsIdx = indexHtml.indexOf('id="proposal-actions"');
  const handoffIdx = indexHtml.indexOf('id="handoff-card"');
  const workerIdx = indexHtml.indexOf('id="worker-card"');
  assert.ok(workerIdx !== -1 && actionsIdx !== -1 && handoffIdx !== -1, 'Required rail IDs present in index.html');
  assert.ok(actionsIdx < handoffIdx, 'Actions panel must precede handoff card in index.html so actions are visible without scrolling');
  assert.ok(workerIdx < actionsIdx, 'Worker card must precede actions panel');

  // 10.2 Verify renderHandoff prioritizes verification evidence for completed/review items
  const renderHandoff = win.renderHandoff || testExports.renderHandoff;
  assert.ok(typeof renderHandoff === 'function', 'renderHandoff is exported');

  const handoffCardEl = documentMock.getElementById('handoff-card');
  const completedReviewItem = {
    id: 'rev-item-1',
    status: 'review',
    handoff: {
      progress: '1. Added feature XYZ\n2. Refactored module',
      verification: 'Cargo test passed 51/51 unit tests'
    }
  };

  renderHandoff(completedReviewItem);
  assert.strictEqual(handoffCardEl.hidden, false, 'Handoff card must be visible for completed item with handoff');

  // Find rendered fields
  const renderedFields = handoffCardEl.children.filter(c => c.classList && c.classList.contains('handoff-field'));
  assert.strictEqual(renderedFields.length, 2, 'Must render both verification and progress fields');

  // Priority check: first field MUST be verification evidence
  const firstField = renderedFields[0];
  assert.ok(firstField.classList.contains('handoff-field-verification'), 'First block must be verification evidence for review item');
  const evidencePre = findDescendant(firstField, '.handoff-evidence');
  assert.ok(evidencePre, 'First block must contain .handoff-evidence pre tag');
  assert.ok(evidencePre.textContent.includes('51/51'), 'Evidence block contains verification text');
  const evidencePill = findDescendant(firstField, '.handoff-pill-verification');
  assert.ok(evidencePill, 'Evidence block contains verification pill');

  // Second field MUST be progress
  const secondField = renderedFields[1];
  assert.ok(secondField.classList.contains('handoff-field-progress'), 'Second block must be progress');
  const progressText = findDescendant(secondField, '.handoff-text');
  assert.ok(progressText, 'Progress block must contain .handoff-text div');

  // 10.3 Verify blocked item prioritizes blocker block
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
  assert.strictEqual(blockedFields.length, 3, 'Must render all 3 fields');
  assert.ok(blockedFields[0].classList.contains('handoff-field-blocker'), 'Blocker block must be first for blocked status');
  assert.ok(blockedFields[1].classList.contains('handoff-field-verification'), 'Verification block must be second');
  assert.ok(blockedFields[2].classList.contains('handoff-field-progress'), 'Progress block must be third');

  // 10.4 Empty handoff hides card
  const emptyHandoffItem = { id: 'empty-1', status: 'done', handoff: {} };
  renderHandoff(emptyHandoffItem);
  assert.strictEqual(handoffCardEl.hidden, true, 'Handoff card must be hidden when handoff is empty');

  console.log('PASS: Handoff blocks correctly prioritized, structured, and ordered above actions');
  console.log('\n========================================');
  console.log('ALL VIEWER TRAJECTORY TESTS PASSED (0)');
  console.log('========================================');
  process.exit(0);
})().catch(err => {
  console.error('FAIL in async tests:', err);
  process.exit(1);
});
