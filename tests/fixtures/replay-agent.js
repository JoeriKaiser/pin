#!/usr/bin/env node
'use strict';

// Replays a recorded ACP notification stream as a live agent.
//
// The transcript is a JSONL file of agent-to-client messages recorded by a
// real run (see export-transcript.js). The handshake is answered locally and
// the recorded stream is emitted when the prompt arrives, so tests exercise
// real framing and real payloads without spawning an agent.
//
// Usage: node replay-agent.js <transcript.jsonl> [--die-after N] [--hang]
//   --die-after N  emit N notifications, then exit 1 (mid-stream failure)
//   --hang         answer the handshake, then stay silent during the prompt
//                  until the client cancels and kills the process

const fs = require('fs');
const readline = require('readline');

const args = process.argv.slice(2);
const transcriptPath = args[0];
if (!transcriptPath) {
  console.error('replay-agent: missing transcript path');
  process.exit(2);
}
const dieAfterIndex = args.indexOf('--die-after');
const dieAfter = dieAfterIndex >= 0 ? Number(args[dieAfterIndex + 1]) : null;
const hang = args.includes('--hang');

const lines = fs.readFileSync(transcriptPath, 'utf8')
  .split('\n')
  .map(line => line.trim())
  .filter(line => line.length > 0);

function sessionIdFromTranscript() {
  for (const line of lines) {
    try {
      const value = JSON.parse(line);
      const sessionId = value && value.params && value.params.sessionId;
      if (sessionId) return sessionId;
    } catch (_) {
      // Ignore malformed lines; the export tool writes valid JSONL.
    }
  }
  return 'replay-session';
}

const sessionId = sessionIdFromTranscript();

function reply(id, result) {
  process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id, result }) + '\n');
}

const rl = readline.createInterface({ input: process.stdin });

rl.on('line', line => {
  if (!line.trim()) return;
  let request;
  try {
    request = JSON.parse(line);
  } catch (_) {
    return;
  }

  switch (request.method) {
    case 'initialize':
      reply(request.id, { protocolVersion: 1, agentCapabilities: {} });
      break;
    case 'session/new':
      reply(request.id, { sessionId });
      break;
    case 'session/prompt': {
      if (hang) return;
      const count = dieAfter === null ? lines.length : Math.min(dieAfter, lines.length);
      for (let i = 0; i < count; i++) {
        process.stdout.write(lines[i] + '\n');
      }
      if (dieAfter !== null) {
        // Exit without a prompt response, after the queued writes flush.
        process.stdout.write('', () => {
          process.exitCode = 1;
          rl.close();
          process.stdin.destroy();
        });
        return;
      }
      reply(request.id, { stopReason: 'end_turn' });
      break;
    }
    case 'session/cancel':
      process.exit(0);
      break;
    default:
      break;
  }
});
