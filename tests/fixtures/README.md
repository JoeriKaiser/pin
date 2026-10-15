# Test fixtures

Recorded agent streams for the behavior suite.

- `transcripts/` — sanitized JSONL recordings from real runs, one file per
  scenario. Each line is an ACP notification exactly as the agent sent it.
- `export-transcript.js` — exports one session from a vault transcript
  (`<vault>/runs/<id>.events.jsonl`) into a sanitized fixture. It keeps the
  session init noise, starts from the recorded final message so the outcome
  survives the trim, and strips home paths and common secret shapes:

  ```
  node tests/fixtures/export-transcript.js ~/.pin_vault/runs/<id>.events.jsonl \
    --head 15 --from-final-message --truncate-text 800 \
    > tests/fixtures/transcripts/<name>.jsonl
  ```

- `replay-agent.js` — serves a fixture as a live agent: answers the handshake
  with the recorded session ID, emits the recorded notifications when prompted,
  then ends the turn. `--die-after N` dies mid-stream after N notifications;
  `--hang` stays silent until the client cancels.

`tests/cli.sh` drives the replay agent through the full manager lifecycle, so a
new ACP fix should land with a fixture recorded from the run that exposed it.
