#!/usr/bin/env node
'use strict';

// Exports one run from a pin transcript into a sanitized replay fixture.
//
// Usage: node export-transcript.js <events.jsonl> [options] > fixture.jsonl
//   --session <id>        keep this session only (default: the last one seen)
//   --head <n>            keep the first n events (session init noise)
//   --from-final-message  start at the first assistant chunk carrying a
//                         Summary or Verification marker, minus a small margin,
//                         so the recorded outcome survives the trim
//   --truncate-text <n>   clamp string values to n characters
//
// The input is <vault>/runs/<id>.events.jsonl as written by pin itself. Only
// agent-to-client notifications are exported; manager lifecycle markers and
// responses are skipped, because the replay agent synthesizes those.

const fs = require('fs');
const os = require('os');

const SUMMARY_MARKERS = ['## summary', '### summary', 'summary:'];
const EVIDENCE_MARKERS = [
  '## verification',
  '### verification',
  '## tests',
  '### tests',
  '## evidence',
  '### evidence',
  'verification evidence:',
  'test evidence:',
  'verification:',
  'evidence:',
];

const SECRET_PATTERNS = [
  /gh[pousr]_[A-Za-z0-9]{20,}/g,
  /sk-[A-Za-z0-9_-]{20,}/g,
  /AKIA[0-9A-Z]{16}/g,
  /xox[baprs]-[A-Za-z0-9-]{10,}/g,
  /Bearer\s+[A-Za-z0-9._~+/=-]{20,}/g,
  /-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----/g,
];

function parseArgs(argv) {
  const options = { session: null, head: 0, fromFinalMessage: false, truncateText: 0 };
  const positional = [];
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--session') options.session = argv[++i];
    else if (arg === '--head') options.head = Number(argv[++i]);
    else if (arg === '--from-final-message') options.fromFinalMessage = true;
    else if (arg === '--truncate-text') options.truncateText = Number(argv[++i]);
    else positional.push(arg);
  }
  return { options, inputPath: positional[0] };
}

function assistantText(update) {
  if (!update || typeof update !== 'object') return '';
  if (update.sessionUpdate !== 'agent_message_chunk') return '';
  const content = update.content;
  if (content && content.type === 'text') return content.text || '';
  return '';
}

function hasMarker(text) {
  const lower = text.toLowerCase();
  return SUMMARY_MARKERS.concat(EVIDENCE_MARKERS).some(marker => lower.includes(marker));
}

function sanitizeString(value, truncateText) {
  let out = value;
  try {
    const username = os.userInfo().username;
    out = out.split(`/home/${username}`).join('/home/user');
    out = out.split(`/Users/${username}`).join('/Users/user');
  } catch (_) {
    // os.userInfo can fail in minimal environments; path scrubbing is best effort.
  }
  for (const pattern of SECRET_PATTERNS) {
    out = out.replace(pattern, '[redacted]');
  }
  if (truncateText > 0 && out.length > truncateText) {
    out = out.slice(0, truncateText) + '...[truncated]';
  }
  return out;
}

function sanitizeValue(value, truncateText) {
  if (typeof value === 'string') return sanitizeString(value, truncateText);
  if (Array.isArray(value)) return value.map(item => sanitizeValue(item, truncateText));
  if (value && typeof value === 'object') {
    const out = {};
    for (const key of Object.keys(value)) {
      out[key] = sanitizeValue(value[key], truncateText);
    }
    return out;
  }
  return value;
}

function main() {
  const { options, inputPath } = parseArgs(process.argv.slice(2));
  if (!inputPath) {
    console.error('export-transcript: missing transcript path');
    console.error('usage: node export-transcript.js <events.jsonl> [--session <id>] [--head N] [--from-final-message] [--truncate-text N]');
    process.exit(2);
  }

  const raw = fs.readFileSync(inputPath, 'utf8');
  let events = [];
  let lastName = null;
  for (const line of raw.split('\n')) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    let value;
    try {
      value = JSON.parse(trimmed);
    } catch (_) {
      continue;
    }
    if (!value.method) continue;
    const sessionId = value.params && value.params.sessionId;
    if (sessionId) lastName = sessionId;
    events.push({ value, sessionId });
  }

  const session = options.session || lastName;
  if (session) {
    events = events.filter(event => !event.sessionId || event.sessionId === session);
  }
  if (events.length === 0) {
    console.error(`export-transcript: no notifications found for session ${session}`);
    process.exit(1);
  }

  let start = 0;
  if (options.fromFinalMessage) {
    const index = events.findIndex(
      event => hasMarker(assistantText(event.value.params && event.value.params.update))
    );
    if (index < 0) {
      console.error('export-transcript: no summary or verification marker found in assistant output');
      process.exit(1);
    }
    start = Math.max(0, index - 40);
  }

  let selected;
  if (start > 0) {
    selected = events.slice(0, options.head).concat(events.slice(start));
  } else {
    selected = events.slice(0, Math.max(options.head, events.length));
  }

  for (const event of selected) {
    const sanitized = sanitizeValue(event.value, options.truncateText);
    process.stdout.write(JSON.stringify(sanitized) + '\n');
  }
  console.error(
    `export-transcript: session ${session}, ${selected.length} of ${events.length} notifications, start ${start}`
  );
}

main();
