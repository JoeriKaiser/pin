#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
TMP=$(mktemp -d)
view_pid=
cleanup() {
    if [ -n "$view_pid" ]; then
        kill "$view_pid" 2>/dev/null || true
        wait "$view_pid" 2>/dev/null || true
    fi
    rm -rf "$TMP"
}
trap cleanup EXIT INT TERM

PIN_BIN=${PIN_BIN:-"$TMP/pin"}
if [ ! -x "$PIN_BIN" ]; then
    cargo build --manifest-path "$ROOT/Cargo.toml" --bin pin
    cp "$ROOT/target/debug/pin" "$PIN_BIN"
fi

fail() {
    echo "FAIL: $*" >&2
    exit 1
}

assert_contains() {
    case "$1" in
        *"$2"*) ;;
        *) fail "expected '$2' in: $1" ;;
    esac
}

start_view() {
    # start_view <vault> <logfile> <agent command> [extra view args...]
    view_vault=$1
    view_log=$2
    view_agent=$3
    shift 3
    PIN_VAULT="$view_vault" "$PIN_BIN" view --no-open --format plain --acp-command "$view_agent" "$@" >"$view_log" 2>&1 &
    view_pid=$!
    view_url=""
    for i in 1 2 3 4 5 6 7 8 9 10; do
        sleep 0.1
        if [ -s "$view_log" ]; then
            view_url=$(tr -d '\r' <"$view_log" | head -n 1)
            break
        fi
    done
    [ -n "$view_url" ] || fail "view server did not start for $view_vault"
}

stop_view() {
    kill "$view_pid" 2>/dev/null || true
    wait "$view_pid" 2>/dev/null || true
    view_pid=
}

"$PIN_BIN" --help >/dev/null 2>&1
assert_contains "$("$PIN_BIN" --version)" "pin 2.2.0"

mkdir -p "$TMP/repo/packages/api" "$TMP/home"
cd "$TMP/repo"
git init -q
HOME="$TMP/home" "$PIN_BIN" init --local --project example --format json | grep -q '"scope":"local"'
cd packages/api

created=$(HOME="$TMP/home" "$PIN_BIN" add '# Cache invalidation

Avoid repeated work.' --kind technical --tags 'perf, agents' --priority high --format json)
assert_contains "$created" '"project":"example"'
assert_contains "$created" '"kind":"technical"'
assert_contains "$created" '"title":"Cache invalidation"'
assert_contains "$created" '"tags":["perf","agents"]'
assert_contains "$created" '"priority":"high"'
HOME="$TMP/home" "$PIN_BIN" read "$(printf '%s' "$created" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')" | grep -q -E '^schema: [12]$'

wf_item=$(HOME="$TMP/home" "$PIN_BIN" add '# Lifecycle test' --kind technical --type task --format json)
wf_id=$(printf '%s' "$wf_item" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
HOME="$TMP/home" PIN_ACTOR=agent:test "$PIN_BIN" transition "$wf_id" --to planned --format json | grep -q '"status":"planned"'
HOME="$TMP/home" PIN_ACTOR=agent:test "$PIN_BIN" claim "$wf_id" --lease 60 --format json | grep -q '"status":"in_progress"'
if HOME="$TMP/home" PIN_ACTOR=agent:other "$PIN_BIN" claim "$wf_id" --format json >/dev/null 2>&1; then
    fail "claim was stolen from another active actor"
fi
HOME="$TMP/home" PIN_ACTOR=agent:test "$PIN_BIN" handoff "$wf_id" --progress 'Started work' --next 'Finish tests' --format json | grep -q '"action":"handoff_updated"'
HOME="$TMP/home" PIN_ACTOR=agent:test "$PIN_BIN" release "$wf_id" --format json | grep -q '"status":"planned"'
if HOME="$TMP/home" "$PIN_BIN" transition "$wf_id" --to done --expect-revision 0 >/dev/null 2>&1; then
    fail "stale revision was accepted"
fi
HOME="$TMP/home" PIN_ACTOR=agent:test "$PIN_BIN" claim "$wf_id" --format json >/dev/null
HOME="$TMP/home" PIN_ACTOR=agent:test "$PIN_BIN" complete "$wf_id" --evidence 'Direct completion path' --format json | grep -q '"status":"done"'
HOME="$TMP/home" PIN_ACTOR=agent:test "$PIN_BIN" close "$wf_id" --format json | grep -q '"status":"closed"'
HOME="$TMP/home" "$PIN_BIN" rm "$wf_id" --format json >/dev/null

# A run that ends after the user moved the item must not reset the status
preserve_item=$(HOME="$TMP/home" "$PIN_BIN" add '# Status preservation' --kind technical --type task --format json)
preserve_id=$(printf '%s' "$preserve_item" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
HOME="$TMP/home" PIN_ACTOR=agent:omp "$PIN_BIN" claim "$preserve_id" --format json >/dev/null
HOME="$TMP/home" PIN_ACTOR=human:viewer "$PIN_BIN" transition "$preserve_id" --to blocked --note 'Blocked mid-run' --format json >/dev/null
HOME="$TMP/home" PIN_ACTOR=agent:omp "$PIN_BIN" release "$preserve_id" --force --format json | grep -q '"status":"blocked"'
HOME="$TMP/home" "$PIN_BIN" rm "$preserve_id" --format json >/dev/null

if HOME="$TMP/home" "$PIN_BIN" add '# Missing kind' >/dev/null 2>&1; then
    fail "add accepted a proposal without --kind"
fi

id=$(HOME="$TMP/home" "$PIN_BIN" list-project --format plain | awk 'NR == 1 { print $1 }')
[ ${#id} -eq 12 ] || fail "expected a 12-character ID, got '$id'"
prefix=$(printf '%s' "$id" | cut -c1-5)
HOME="$TMP/home" "$PIN_BIN" read "$prefix" | grep -q '# Cache invalidation'
HOME="$TMP/home" "$PIN_BIN" read "$prefix" --format json | grep -q '"content"'

if HOME="$TMP/home" "$PIN_BIN" add '# Cache invalidation' --kind technical >/dev/null 2>&1; then
    fail "duplicate title was accepted without --allow-duplicate"
fi
HOME="$TMP/home" "$PIN_BIN" add '# Cache invalidation' --kind product --allow-duplicate --format json >/dev/null

context=$(HOME="$TMP/home" "$PIN_BIN" context --limit 1 --format plain)
assert_contains "$context" 'Active proposals for example:'
assert_contains "$context" '[technical]'
assert_contains "$context" '(high)'
[ "$(printf '%s\n' "$context" | grep -c '^- \[')" -eq 1 ] || fail "context did not honor --limit or priority ordering"

grouped=$(HOME="$TMP/home" "$PIN_BIN" context --limit 10 --group kind --format plain)
assert_contains "$grouped" 'Technical:'
assert_contains "$grouped" 'Product:'

product=$(HOME="$TMP/home" "$PIN_BIN" list-project --kind product --format json)
assert_contains "$product" '"kind":"product"'
[ "$(printf '%s' "$product" | grep -o '"id"' | wc -l | tr -d ' ')" -eq 1 ] || fail "kind filter returned the wrong number of ideas"

search=$(HOME="$TMP/home" "$PIN_BIN" search 'repeated work' --kind technical --format json)
assert_contains "$search" '"title":"Cache invalidation"'

if HOME="$TMP/home" "$PIN_BIN" read "$id" extra >/dev/null 2>&1; then
    fail "read accepted an unexpected argument"
fi
if read_error=$(HOME="$TMP/home" "$PIN_BIN" read "$id" --format 2>&1); then
    fail "read accepted --format without a value"
fi
assert_contains "$read_error" '--format requires a value'
if edit_error=$(HOME="$TMP/home" "$PIN_BIN" edit "$id" --format 2>&1); then
    fail "edit accepted --format without a value"
fi
assert_contains "$edit_error" '--format requires a value'
if rm_error=$(HOME="$TMP/home" "$PIN_BIN" rm "$id" --format 2>&1); then
    fail "rm accepted --format without a value"
fi
assert_contains "$rm_error" '--format requires a value'

cat >"$TMP/editor" <<'EOF'
#!/bin/sh
[ "$1" = "--wait" ] || exit 2
printf '\nEdited in test.\n' >>"$2"
EOF
chmod +x "$TMP/editor"
edited=$(HOME="$TMP/home" EDITOR="$TMP/editor --wait" "$PIN_BIN" edit "$id" --format json)
assert_contains "$edited" '"edited"'
HOME="$TMP/home" "$PIN_BIN" read "$id" | grep -q 'Edited in test.'

cat >"$TMP/bad-editor" <<'EOF'
#!/bin/sh
printf '%s\n' 'not valid front matter' >"$1"
EOF
chmod +x "$TMP/bad-editor"
if HOME="$TMP/home" EDITOR="$TMP/bad-editor" "$PIN_BIN" edit "$id" >/dev/null 2>&1; then
    fail "edit accepted malformed front matter"
fi
HOME="$TMP/home" "$PIN_BIN" read "$id" | grep -q 'Edited in test.' || fail "invalid edit was not restored"
find "$TMP/repo/.pin_vault" -name '.*.edit-recovery.tmp' -type f | grep -q . || fail "invalid edit did not leave a recovery file"

cat >"$TMP/repo/.pin_vault/legacy.md" <<'EOF'
---
project: "example"
timestamp: 1
title: "Legacy proposal"
---
# Legacy proposal
EOF
legacy=$(HOME="$TMP/home" "$PIN_BIN" list-project --kind unspecified --format json)
assert_contains "$legacy" '"kind":"unspecified"'
assert_contains "$legacy" '"title":"Legacy proposal"'

stats=$(HOME="$TMP/home" "$PIN_BIN" stats --format json)
assert_contains "$stats" '"ideas":3'
assert_contains "$stats" '"technical":1'
assert_contains "$stats" '"product":1'
assert_contains "$stats" '"unspecified":1'
assert_contains "$stats" '"active":3'
assert_contains "$stats" '"archived":0'
if HOME="$TMP/home" "$PIN_BIN" stats unexpected >/dev/null 2>&1; then
    fail "stats accepted an unexpected argument"
fi

health=$(HOME="$TMP/home" "$PIN_BIN" doctor --format json)
assert_contains "$health" '"errors":0'
assert_contains "$health" '"missing_id"'
repaired=$(HOME="$TMP/home" "$PIN_BIN" doctor --repair --format json)
assert_contains "$repaired" '"repaired":1'
grep -q '^schema: 1$' "$TMP/repo/.pin_vault/legacy.md" || fail "doctor did not add schema"
grep -q '^id:' "$TMP/repo/.pin_vault/legacy.md" || fail "doctor did not add an ID"
second_repair=$(HOME="$TMP/home" "$PIN_BIN" doctor --repair --format json)
assert_contains "$second_repair" '"repaired":0'

HOME="$TMP/home" "$PIN_BIN" export "$TMP/export" --format json | grep -q '"operation":"export"'
exported=$(find "$TMP/export" -name '*.md' -type f | wc -l | tr -d ' ')
[ "$exported" -eq 3 ] || fail "expected three exported ideas, got $exported"
rm -f "$TMP/repo/.pin_vault"/*.md
HOME="$TMP/home" "$PIN_BIN" import "$TMP/export" --format json | grep -q '"operation":"import"'
[ "$(HOME="$TMP/home" "$PIN_BIN" list-project --format plain | wc -l | tr -d ' ')" -eq 3 ] || fail "import did not restore ideas"

mkdir -p "$TMP/mixed-import" "$TMP/import-target"
first_pin=$(find "$TMP/export" -name '*.md' -type f | head -1)
cp "$first_pin" "$TMP/mixed-import/00-valid.md"
printf '%s\n' 'not a pin file' >"$TMP/mixed-import/99-invalid.md"
if import_error=$(PIN_VAULT="$TMP/import-target" HOME="$TMP/home" "$PIN_BIN" import "$TMP/mixed-import" --format json 2>&1); then
    fail "import accepted a malformed pin file"
fi
assert_contains "$import_error" 'is not a valid pin file'
if find "$TMP/import-target" -name '*.md' -type f | grep -q .; then
    fail "failed import left the destination partially populated"
fi

archived=$(HOME="$TMP/home" "$PIN_BIN" archive "$id" --resolution implemented --note 'covered by tests' --format json)
assert_contains "$archived" '"archived"'
active_list=$(HOME="$TMP/home" "$PIN_BIN" list-project --format json)
case "$active_list" in *"$id"*) fail "archived idea remained active" ;; esac
archived_list=$(HOME="$TMP/home" "$PIN_BIN" list-project --archived --format json)
assert_contains "$archived_list" "$id"
assert_contains "$archived_list" '"resolution":"implemented"'
all_stats=$(HOME="$TMP/home" "$PIN_BIN" stats --format json)
assert_contains "$all_stats" '"archived":1'
HOME="$TMP/home" "$PIN_BIN" unarchive "$id" --format json | grep -q '"unarchived"'
HOME="$TMP/home" "$PIN_BIN" list-project --format json | grep -q "$id"

mkdir -p "$TMP/dependency-vault"
dependency=$(PIN_VAULT="$TMP/dependency-vault" PIN_PROJECT=dependencies "$PIN_BIN" add '# Dependency' --kind technical --type task --format json)
dependency_id=$(printf '%s' "$dependency" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
dependent=$(PIN_VAULT="$TMP/dependency-vault" PIN_PROJECT=dependencies "$PIN_BIN" add '# Dependent work' --kind technical --type task --format json)
dependent_id=$(printf '%s' "$dependent" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
PIN_VAULT="$TMP/dependency-vault" PIN_PROJECT=dependencies "$PIN_BIN" depend "$dependent_id" "$dependency_id" --format json | grep -q '"action":"dependency_added"'
PIN_VAULT="$TMP/dependency-vault" PIN_PROJECT=dependencies "$PIN_BIN" parent "$dependent_id" "$dependency_id" --format json | grep -q '"parent_id":"'
PIN_VAULT="$TMP/dependency-vault" PIN_PROJECT=dependencies "$PIN_BIN" relate "$dependent_id" "$dependency_id" --format json | grep -q '"related":\['
PIN_VAULT="$TMP/dependency-vault" PIN_PROJECT=dependencies "$PIN_BIN" transition "$dependency_id" --to planned --format json >/dev/null
PIN_VAULT="$TMP/dependency-vault" PIN_PROJECT=dependencies "$PIN_BIN" transition "$dependent_id" --to planned --format json >/dev/null
ready=$(PIN_VAULT="$TMP/dependency-vault" PIN_PROJECT=dependencies "$PIN_BIN" next --format json)
assert_contains "$ready" "$dependency_id"
case "$ready" in *"$dependent_id"*) fail "next returned work with unfinished dependency" ;; esac
ready_list=$(PIN_VAULT="$TMP/dependency-vault" PIN_PROJECT=dependencies "$PIN_BIN" list-project --ready --format json)
assert_contains "$ready_list" "$dependency_id"
case "$ready_list" in *"$dependent_id"*) fail "ready list returned work with unfinished dependency" ;; esac
PIN_VAULT="$TMP/dependency-vault" PIN_PROJECT=dependencies "$PIN_BIN" claim "$dependency_id" --format json >/dev/null
PIN_VAULT="$TMP/dependency-vault" PIN_PROJECT=dependencies "$PIN_BIN" complete "$dependency_id" --evidence 'Dependency completed' --format json >/dev/null
ready_after=$(PIN_VAULT="$TMP/dependency-vault" PIN_PROJECT=dependencies "$PIN_BIN" next --format json)
assert_contains "$ready_after" "$dependent_id"
if PIN_VAULT="$TMP/dependency-vault" PIN_PROJECT=dependencies "$PIN_BIN" depend "$dependency_id" "$dependent_id" --format json >/dev/null 2>&1; then
    fail "dependency cycle was accepted"
fi
cat >"$TMP/dep-remove-editor" <<'EOF'
#!/bin/sh
[ "$1" = "--wait" ] || exit 2
sed -i '/depends_on:/,+1d' "$2"
EOF
chmod +x "$TMP/dep-remove-editor"
PIN_VAULT="$TMP/dependency-vault" EDITOR="$TMP/dep-remove-editor --wait" "$PIN_BIN" edit "$dependent_id" --format json >/dev/null
edited_dep=$(PIN_VAULT="$TMP/dependency-vault" "$PIN_BIN" read "$dependent_id")
case "$edited_dep" in *"depends_on:"*) fail "edit did not remove dependency: $edited_dep" ;; esac
cross_dep=$(PIN_VAULT="$TMP/dependency-vault" PIN_PROJECT=shared "$PIN_BIN" add '# Shared dependency' --kind technical --type task --format json)
cross_dep_id=$(printf '%s' "$cross_dep" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
cross_dependent=$(PIN_VAULT="$TMP/dependency-vault" PIN_PROJECT=other "$PIN_BIN" add '# Other project dependent' --kind technical --type task --format json)
cross_dependent_id=$(printf '%s' "$cross_dependent" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
PIN_VAULT="$TMP/dependency-vault" "$PIN_BIN" depend "$cross_dependent_id" "$cross_dep_id" --format json >/dev/null
PIN_VAULT="$TMP/dependency-vault" "$PIN_BIN" transition "$cross_dep_id" --to planned --format json >/dev/null
PIN_VAULT="$TMP/dependency-vault" "$PIN_BIN" transition "$cross_dependent_id" --to planned --format json >/dev/null
cross_ready=$(PIN_VAULT="$TMP/dependency-vault" PIN_PROJECT=other "$PIN_BIN" next --format json)
case "$cross_ready" in *"$cross_dependent_id"*) fail "next returned other project work with unfinished shared dependency" ;; esac
PIN_VAULT="$TMP/dependency-vault" "$PIN_BIN" claim "$cross_dep_id" --format json >/dev/null
PIN_VAULT="$TMP/dependency-vault" "$PIN_BIN" complete "$cross_dep_id" --evidence 'Shared dependency completed' --format json >/dev/null
cross_ready_after=$(PIN_VAULT="$TMP/dependency-vault" PIN_PROJECT=other "$PIN_BIN" next --format json)
assert_contains "$cross_ready_after" "$cross_dependent_id"

mkdir -p "$TMP/upgrade-vault"
cat >"$TMP/upgrade-vault/legacy.md" <<'EOF'
---
project: example
timestamp: 1
title: Upgrade me
---
# Upgrade me
EOF
PIN_VAULT="$TMP/upgrade-vault" "$PIN_BIN" doctor --upgrade --format json | grep -q '"repaired":1'
PIN_VAULT="$TMP/upgrade-vault" "$PIN_BIN" read legacy.md | grep -q '^schema: 2$'
PIN_VAULT="$TMP/upgrade-vault" "$PIN_BIN" read legacy.md | grep -q '^status: "created"$'

contention_item=$(PIN_VAULT="$TMP/repo/.pin_vault" PIN_PROJECT=example "$PIN_BIN" add '# Lock contention test' --kind technical --type task --format json)
contention_id=$(printf '%s' "$contention_item" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
PIN_VAULT="$TMP/repo/.pin_vault" PIN_PROJECT=example "$PIN_BIN" transition "$contention_id" --to planned --format json >/dev/null

PIN_VAULT="$TMP/repo/.pin_vault" PIN_PROJECT=example PIN_ACTOR=agent:worker1 "$PIN_BIN" handoff "$contention_id" --progress 'Worker 1 progress' --format json >"$TMP/worker1.out" &
p1=$!
PIN_VAULT="$TMP/repo/.pin_vault" PIN_PROJECT=example PIN_ACTOR=agent:worker2 "$PIN_BIN" relate "$contention_id" "$prefix" --format json >"$TMP/worker2.out" &
p2=$!
wait $p1
wait $p2

read_contention=$(PIN_VAULT="$TMP/repo/.pin_vault" "$PIN_BIN" read "$contention_id" --format json)
assert_contains "$read_contention" 'Worker 1 progress'
assert_contains "$read_contention" "$prefix"
assert_contains "$read_contention" 'revision: 4'
assert_contains "$read_contention" 'agent:worker1'
assert_contains "$read_contention" 'agent:worker2'
HOME="$TMP/home" "$PIN_BIN" rm "$contention_id" --format json >/dev/null

mkdir -p "$TMP/search-vault"
PIN_VAULT="$TMP/search-vault" PIN_PROJECT=search "$PIN_BIN" add '# Incidental note

This body discusses ranked retrieval results.' --kind technical --format json >/dev/null
PIN_VAULT="$TMP/search-vault" PIN_PROJECT=search "$PIN_BIN" add '# Ranked retrieval results

Direct title match.' --kind product --format json >/dev/null
ranked=$(PIN_VAULT="$TMP/search-vault" PIN_PROJECT=search "$PIN_BIN" search 'ranked results' --format json)
first_title=$(printf '%s' "$ranked" | sed -n 's/^\[{[^}]*"title":"\([^"]*\)".*/\1/p')
[ "$first_title" = "Ranked retrieval results" ] || fail "search did not rank the title match first: $ranked"
assert_contains "$ranked" '"score":'
limited=$(PIN_VAULT="$TMP/search-vault" PIN_PROJECT=search "$PIN_BIN" search 'ranked results' --limit 1 --format json)
[ "$(printf '%s' "$limited" | grep -o '"id"' | wc -l | tr -d ' ')" -eq 1 ] || fail "search did not honor --limit"

mkdir -p "$TMP/broken-vault"
printf '%s\n' 'not a pin' >"$TMP/broken-vault/broken.md"
if PIN_VAULT="$TMP/broken-vault" "$PIN_BIN" doctor --format json >"$TMP/doctor.json"; then
    fail "doctor returned success for malformed front matter"
fi
assert_contains "$(cat "$TMP/doctor.json")" '"missing_front_matter"'

HOME="$TMP/home" "$PIN_BIN" rm "$prefix" --format json | grep -q '"removed"'

# ── view tests ──────────────────────────────────────────────────────────
# 1. Non-TTY gate refusal
if HOME="$TMP/home" "$PIN_BIN" view >/dev/null 2>&1; then
    fail "view accepted non-TTY invocation without --no-open or explicit format"
fi

# 2. View launch with --no-open, plain format, and mock ACP agent
cat >"$TMP/mock-agent.sh" <<'EOF'
#!/bin/sh
while IFS= read -r line; do
  case "$line" in
    *initialize*)
      req_id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
      printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":1,"agentCapabilities":{}}}\n' "$req_id"
      ;;
    *session/new*)
      req_id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
      printf '{"jsonrpc":"2.0","id":%s,"result":{"sessionId":"mock-session-1"}}\n' "$req_id"
      ;;
    *session/prompt*)
      req_id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
      printf '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"mock-session-1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"## Summary\\nMock agent finished\\n\\n## Verification\\nMock tests passed"}}}}\n'
      sleep 0.05
      printf '{"jsonrpc":"2.0","id":%s,"result":{"stopReason":"end_turn"}}\n' "$req_id"
      ;;
    *session/cancel*)
      exit 0
      ;;
  esac
done
EOF
chmod +x "$TMP/mock-agent.sh"

mkdir -p "$TMP/view-vault"
view_item=$(PIN_VAULT="$TMP/view-vault" PIN_PROJECT=view "$PIN_BIN" add '# View test' --kind technical --priority low --format json)
view_id=$(printf '%s' "$view_item" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')

view_out="$TMP/view.log"
view_err="$TMP/view.err"
# Launch in background with mock ACP agent
PIN_VAULT="$TMP/view-vault" "$PIN_BIN" view --no-open --format plain --acp-command "$TMP/mock-agent.sh" >"$view_out" 2>"$view_err" &
view_pid=$!
# Wait/poll for stdout readiness
url=""
for i in 1 2 3 4 5 6 7 8 9 10; do
    sleep 0.1
    if [ -s "$view_out" ]; then
        url=$(cat "$view_out" | tr -d '\r')
        break
    fi
done

[ -n "$url" ] || {
    cat "$view_err" >&2
    kill -9 $view_pid 2>/dev/null || true
    fail "view server did not start or write URL to stdout"
}

# Assert URL structure
assert_contains "$url" "http://127.0.0.1:"
assert_contains "$url" "/"

# Retrieve data.json and check headers and contents
curl_out="$TMP/curl.json"
curl_headers="$TMP/headers.txt"
curl -s -S -D "$curl_headers" "$url"data.json >"$curl_out"

# Assert response is valid JSON and contains the pin we added
assert_contains "$(cat "$curl_out")" '"title":"View test"'
assert_contains "$(cat "$curl_out")" '"kind":"technical"'
assert_contains "$(cat "$curl_out")" '"status":"created"'

# Assert security headers
assert_contains "$(cat "$curl_headers")" "Content-Security-Policy:"
assert_contains "$(cat "$curl_headers")" "default-src 'none'"
assert_contains "$(cat "$curl_headers")" "X-Content-Type-Options: nosniff"
assert_contains "$(cat "$curl_headers")" "X-Frame-Options: DENY"
assert_contains "$(cat "$curl_headers")" "Cache-Control: no-store"

# Try getting index.html
curl_html="$TMP/index.html"
curl -s -S "$url" >"$curl_html"
assert_contains "$(cat "$curl_html")" '<title>pin</title>'
assert_contains "$(cat "$curl_html")" 'id="filter-toggle"'
assert_contains "$(cat "$curl_html")" 'id="proposal-more"'
assert_contains "$(cat "$curl_html")" 'id="btn-new-ticket"'
assert_contains "$(cat "$curl_html")" 'id="spec-modal"'
assert_contains "$(cat "$curl_html")" 'id="quick-add-trigger"'
# Verify all modules in the asset table return 200 with JavaScript content type
for mod in "app.js" \
    "modules/config.js" \
    "modules/state.js" \
    "modules/dom.js" \
    "modules/api.js" \
    "modules/formatters.js" \
    "modules/markdown.js" \
    "modules/router.js" \
    "modules/media.js" \
    "modules/board.js" \
    "modules/detail.js" \
    "modules/forms.js" \
    "modules/agent.js" \
    "modules/bind.js" \
    "modules/boot.js" \
    "modules/test-hooks.js"; do
    mod_meta=$(curl -s -o /dev/null -w "%{http_code} %{content_type}" "${url}${mod}")
    [ "$mod_meta" = "200 text/javascript; charset=utf-8" ] || fail "asset $mod returned $mod_meta, expected 200 text/javascript; charset=utf-8"
done
origin=$(printf '%s' "$url" | cut -d/ -f1-3)
mutation_code=$(curl -s -o /dev/null -w "%{http_code}" -X POST -H 'Content-Type: application/json' --data '{"action":"transition","to":"planned"}' "${url}items/${view_id}/action")
[ "$mutation_code" = "403" ] || fail "viewer accepted a mutation without same-origin headers"
mutation=$(curl -s -S -X POST -H "Origin: $origin" -H 'Content-Type: application/json' -H 'X-Pin-Action: true' --data '{"action":"transition","to":"planned","actor":"human:test","expect_revision":1}' "${url}items/${view_id}/action")
assert_contains "$mutation" '"status":"planned"'
curl -s -S "${url}data.json" | grep -q '"status":"planned"' || fail "viewer data did not refresh after mutation"

# Action errors must name the real problem, not a parse failure
unknown_err=$(curl -s -S -X POST -H "Origin: $origin" -H 'Content-Type: application/json' -H 'X-Pin-Action: true' --data '{"action":"bogus"}' "${url}items/${view_id}/action")
assert_contains "$unknown_err" "Unknown action 'bogus'"
missing_target_err=$(curl -s -S -X POST -H "Origin: $origin" -H 'Content-Type: application/json' -H 'X-Pin-Action: true' --data '{"action":"transition"}' "${url}items/${view_id}/action")
assert_contains "$missing_target_err" "Missing 'to' status for transition"

# Test POST items (quick-add from viewer)
created_from_view=$(curl -s -S -X POST -H "Origin: $origin" -H 'Content-Type: application/json' -H 'X-Pin-Action: true' --data '{"title":"Created from viewer","type":"task","status":"created","project":"view"}' "${url}items")
assert_contains "$created_from_view" '"title":"Created from viewer"'
assert_contains "$created_from_view" '"status":"created"'
curl -s -S "${url}data.json" | grep -q '"title":"Created from viewer"' || fail "viewer data did not contain newly created item"

# Test POST items with detailed spec ticket payload (body, kind, priority, tags)
created_spec=$(curl -s -S -X POST -H "Origin: $origin" -H 'Content-Type: application/json' -H 'X-Pin-Action: true' --data '{"title":"Spec ticket test","body":"## Description\nDetailed spec.\n\n## Acceptance Criteria\n- [ ] Spec criteria","type":"task","kind":"technical","priority":"high","tags":"frontend,spec","status":"created","project":"view"}' "${url}items")
assert_contains "$created_spec" '"title":"Spec ticket test"'
assert_contains "$created_spec" '"priority":"high"'
assert_contains "$created_spec" '"tags":["frontend","spec"]'
spec_id=$(echo "$created_spec" | grep -o '"id":"[^"]*"' | head -n 1 | cut -d'"' -f4)
[ -n "$spec_id" ] || fail "failed to extract id from created spec item"
curl -s -S "${url}data.json" | grep -q '"title":"Spec ticket test"' || fail "viewer data did not contain spec ticket test item"
spec_file_content=$(cat "$TMP/view-vault/${spec_id}.md")
assert_contains "$spec_file_content" "## Acceptance Criteria"
assert_contains "$spec_file_content" "priority: \"high\""
assert_contains "$spec_file_content" "tags: \"frontend,spec\""


# Test POST screenshots endpoint
printf '\x89PNG\r\n\x1a\nfake-screenshot-data' >"$TMP/fake_screenshot.png"
screenshot_resp=$(curl -s -S -X POST -H "Origin: $origin" -H 'X-Pin-Action: true' -H 'Content-Type: image/png' --data-binary @"$TMP/fake_screenshot.png" "${url}screenshots")
assert_contains "$screenshot_resp" '"path":'
assert_contains "$screenshot_resp" '"filename":'
assert_contains "$screenshot_resp" '"markdown":'
screenshot_path=$(echo "$screenshot_resp" | grep -o '"path":"[^"]*"' | cut -d'"' -f4)
screenshot_file=$(echo "$screenshot_resp" | grep -o '"filename":"[^"]*"' | cut -d'"' -f4)
[ -f "$screenshot_path" ] || fail "uploaded screenshot file was not created on disk: $screenshot_path"
cmp "$TMP/fake_screenshot.png" "$screenshot_path" || fail "uploaded screenshot file content mismatch"

# Test GET screenshots/{filename}
curl -s -S "${url}screenshots/${screenshot_file}" >"$TMP/downloaded_screenshot.png"
cmp "$TMP/fake_screenshot.png" "$TMP/downloaded_screenshot.png" || fail "downloaded screenshot mismatch"

# Test path traversal rejected
bad_ss_code=$(curl --path-as-is -s -o /dev/null -w "%{http_code}" "${url}screenshots/../secret.png")
[ "$bad_ss_code" = "400" ] || fail "expected 400 for path traversal screenshot request, got $bad_ss_code"
bad_ss_code2=$(curl -s -o /dev/null -w "%{http_code}" "${url}screenshots/secret..png")
[ "$bad_ss_code2" = "400" ] || fail "expected 400 for invalid screenshot name request, got $bad_ss_code2"

# Create a ticket containing the screenshot path in its description (simulating paste + create)
ticket_with_ss=$(curl -s -S -X POST -H "Origin: $origin" -H 'Content-Type: application/json' -H 'X-Pin-Action: true' --data "{\"title\":\"Ticket with screenshot\",\"body\":\"## Description\\n![screenshot](${screenshot_path})\",\"type\":\"task\",\"status\":\"created\",\"project\":\"view\"}" "${url}items")
ss_ticket_id=$(echo "$ticket_with_ss" | grep -o '"id":"[^"]*"' | head -n 1 | cut -d'"' -f4)
ss_ticket_content=$(cat "$TMP/view-vault/${ss_ticket_id}.md")
assert_contains "$ss_ticket_content" "$screenshot_path"
# Oversized request bodies are rejected rather than allocated
head -c 2000000 /dev/zero | tr '\0' 'a' >"$TMP/oversized.json"
oversized_code=$(curl -s -o /dev/null -w "%{http_code}" -X POST -H "Origin: $origin" -H 'X-Pin-Action: true' --data-binary @"$TMP/oversized.json" "${url}items")
[ "$oversized_code" = "413" ] || {
    kill -9 $view_pid 2>/dev/null || true
    fail "expected 413 for an oversized request body, got $oversized_code"
}
curl -s -S "${url}data.json" | grep -q '"status":"created"' || {
    kill -9 $view_pid 2>/dev/null || true
    fail "viewer stopped serving after rejecting an oversized body"
}

# Try getting file with wrong token
bad_url=$(echo "$url" | sed 's/[a-f0-9]\{32\}/bad_token/')
bad_code=$(curl -s -o /dev/null -w "%{http_code}" "${bad_url}data.json")
[ "$bad_code" = "404" ] || {
    kill -9 $view_pid 2>/dev/null || true
    fail "expected 404 for bad token request, got $bad_code"
}

# Try POST method
post_code=$(curl -s -o /dev/null -w "%{http_code}" -X POST "${url}data.json")
[ "$post_code" = "405" ] || {
    kill -9 $view_pid 2>/dev/null || true
    fail "expected 405 for POST request, got $post_code"
}

# Test ACP runs endpoint
runs_json=$(curl -s -S "${url}runs")
assert_contains "$runs_json" '"running":[]'
assert_contains "$runs_json" '"primary_busy":null'

# Test ACP SSE stream endpoint
stream_headers=$(curl -s -S -I --max-time 1 "${url}items/${view_id}/stream" || true)
assert_contains "$stream_headers" "text/event-stream"

# Test UI contains agent drawer elements with collapsed tool calls and clean thoughts
assert_contains "$(cat "$curl_html")" 'id="agent-drawer"'
assert_contains "$(cat "$curl_html")" 'id="worktree-modal"'
assert_contains "$(cat "$curl_html")" 'id="btn-toggle-tools"'
assert_contains "$(cat "$curl_html")" 'id="agent-thoughts"'
assert_contains "$(cat "$curl_html")" 'id="agent-tools"'
# Human claiming is gone across all modules
curl -s -S "${url}modules/bind.js" >"$TMP/bind.js"
curl -s -S "${url}modules/detail.js" >"$TMP/detail.js"
if grep -q 'openClaimModal\|openReleaseModal\|Release Claim\|lease-countdown\|claimer-badge' "$TMP/bind.js" "$TMP/detail.js"; then
    fail "frontend modules still carry the human claim UI"
fi

# Verify JavaScript frontend behavior, trajectory events, and run dispatching
node "$ROOT/tests/viewer_trajectory.js"

# Test ACP agent execution and automatic handoff/review transition
# A human claim must not block the run: agents own execution
PIN_VAULT="$TMP/view-vault" "$PIN_BIN" claim "$view_id" --actor human:audit --lease 3600 --format json | grep -q '"status":"in_progress"'
run_res=$(curl -s -S -X POST -H "Origin: $origin" -H 'Content-Type: application/json' -H 'X-Pin-Action: true' "${url}items/${view_id}/run")
assert_contains "$run_res" '"status":"started"'

read_item=""
for i in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do
    sleep 0.1
    read_item=$(PIN_VAULT="$TMP/view-vault" "$PIN_BIN" read "$view_id" 2>/dev/null || true)
    if printf '%s' "$read_item" | grep -q 'status: "review"'; then
        break
    fi
done
assert_contains "$read_item" 'status: "review"'
assert_contains "$read_item" 'progress: "Mock agent finished"'
assert_contains "$read_item" 'verification: "Mock tests passed"'
[ -f "$TMP/view-vault/runs/${view_id}.log" ] || fail "run log file was not created"
[ -f "$TMP/view-vault/runs/${view_id}.events.jsonl" ] || fail "run events file was not created"
if printf '%s' "$read_item" | grep -q 'claimed_by:'; then
    fail "a finished run kept its claim attribution"
fi

# Test commit-pr endpoint on unready item rejects with 400
bad_commit_pr_code=$(curl -s -o /dev/null -w "%{http_code}" -X POST -H "Origin: $origin" -H 'Content-Type: application/json' -H 'X-Pin-Action: true' "${url}items/${ss_ticket_id}/commit-pr")
[ "$bad_commit_pr_code" = "400" ] || fail "expected 400 when calling commit-pr on created item, got $bad_commit_pr_code"

# Test commit-pr endpoint on review item starts successfully
commit_pr_res=$(curl -s -S -X POST -H "Origin: $origin" -H 'Content-Type: application/json' -H 'X-Pin-Action: true' "${url}items/${view_id}/commit-pr")
assert_contains "$commit_pr_res" '"status":"started"'
assert_contains "$commit_pr_res" '"mode":"commit_pr"'

# Wait for mock agent to complete commit-pr run
for i in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do
    sleep 0.1
    read_item=$(PIN_VAULT="$TMP/view-vault" "$PIN_BIN" read "$view_id" 2>/dev/null || true)
    if printf '%s' "$read_item" | grep -q 'status: "review"' && ! printf '%s' "$read_item" | grep -q 'claimed_by:'; then
        break
    fi
done
assert_contains "$read_item" 'status: "review"'
if printf '%s' "$read_item" | grep -q 'claimed_by:'; then
    fail "commit-pr finished run kept its claim attribution"
fi

kill "$view_pid" 2>/dev/null || true
wait "$view_pid" 2>/dev/null || true
view_pid=

# ── CLI filters must survive into the served snapshot ───────────────────
mkdir -p "$TMP/filter-vault"
filter_match=$(PIN_VAULT="$TMP/filter-vault" PIN_PROJECT=filter "$PIN_BIN" add '# Filter match' --kind technical --type bug --tags 'perf' --format json)
filter_match_id=$(printf '%s' "$filter_match" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
PIN_VAULT="$TMP/filter-vault" "$PIN_BIN" transition "$filter_match_id" --to planned >/dev/null
PIN_VAULT="$TMP/filter-vault" "$PIN_BIN" transition "$filter_match_id" --to blocked >/dev/null
PIN_VAULT="$TMP/filter-vault" PIN_PROJECT=filter "$PIN_BIN" add '# Filter miss' --kind technical --type bug --tags 'perf' --format json >/dev/null

start_view "$TMP/filter-vault" "$TMP/filter-view.log" "$TMP/mock-agent.sh" --status blocked --tag perf --kind technical --type bug
filter_json=$(curl -s -S "${view_url}data.json")
assert_contains "$filter_json" '"title":"Filter match"'
assert_contains "$filter_json" '"filters":{"status":"blocked","tag":"perf","kind":"technical","item_type":"bug"}'
if printf '%s' "$filter_json" | grep -q '"title":"Filter miss"'; then
    stop_view
    fail "viewer ignored the CLI filters and served a non-matching item"
fi
stop_view

# A different --status value must change what is served
start_view "$TMP/filter-vault" "$TMP/filter-view.log" "$TMP/mock-agent.sh" --status created
filter_json=$(curl -s -S "${view_url}data.json")
assert_contains "$filter_json" '"title":"Filter miss"'
if printf '%s' "$filter_json" | grep -q '"title":"Filter match"'; then
    stop_view
    fail "viewer served a blocked item for --status created"
fi
stop_view

# ── A second primary run must report primary_busy, not fail silently ────
cat >"$TMP/slow-agent.sh" <<'EOF'
#!/bin/sh
while IFS= read -r line; do
  case "$line" in
    *initialize*)
      req_id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
      printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":1,"agentCapabilities":{}}}\n' "$req_id"
      ;;
    *session/new*)
      req_id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
      printf '{"jsonrpc":"2.0","id":%s,"result":{"sessionId":"slow-session-1"}}\n' "$req_id"
      ;;
    *session/prompt*)
      req_id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
      sleep 3
      printf '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"slow-session-1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"## Summary\\nSlow agent finished"}}}}\n'
      printf '{"jsonrpc":"2.0","id":%s,"result":{"stopReason":"end_turn"}}\n' "$req_id"
      ;;
    *session/cancel*)
      exit 0
      ;;
  esac
done
EOF
chmod +x "$TMP/slow-agent.sh"

mkdir -p "$TMP/busy-vault"
busy_holder=$(PIN_VAULT="$TMP/busy-vault" PIN_PROJECT=busy "$PIN_BIN" add '# Busy holder' --kind technical --type task --format json)
busy_holder_id=$(printf '%s' "$busy_holder" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
busy_waiter=$(PIN_VAULT="$TMP/busy-vault" PIN_PROJECT=busy "$PIN_BIN" add '# Busy waiter' --kind technical --type task --format json)
busy_waiter_id=$(printf '%s' "$busy_waiter" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')

start_view "$TMP/busy-vault" "$TMP/busy-view.log" "$TMP/slow-agent.sh"
busy_origin=$(printf '%s' "$view_url" | cut -d/ -f1-3)
holder_code=$(curl -s -o /dev/null -w "%{http_code}" -X POST -H "Origin: $busy_origin" -H 'Content-Type: application/json' -H 'X-Pin-Action: true' "${view_url}items/${busy_holder_id}/run")
[ "$holder_code" = "200" ] || {
    stop_view
    fail "first primary run did not start: $holder_code"
}
busy_body=$(curl -s -S -X POST -H "Origin: $busy_origin" -H 'Content-Type: application/json' -H 'X-Pin-Action: true' "${view_url}items/${busy_waiter_id}/run")
assert_contains "$busy_body" '"status":"primary_busy"'
assert_contains "$busy_body" "\"active_id\":\"${busy_holder_id}\""
assert_contains "$(PIN_VAULT="$TMP/busy-vault" "$PIN_BIN" read "$busy_waiter_id")" 'status: "created"'
stop_view

# ── Worktree provisioning failure must surface, not report success ──────
mkdir -p "$TMP/no-commits" "$TMP/wt-vault"
# An empty repository: `git worktree add` cannot resolve HEAD, so provisioning fails
git -C "$TMP/no-commits" init -q
wt_item=$(PIN_VAULT="$TMP/wt-vault" PIN_PROJECT=wt "$PIN_BIN" add '# Worktree failure' --kind technical --type task --format json)
wt_id=$(printf '%s' "$wt_item" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')

(
    cd "$TMP/no-commits"
    exec env PIN_VAULT="$TMP/wt-vault" "$PIN_BIN" view --no-open --format plain --acp-command "$TMP/mock-agent.sh" >"$TMP/wt-view.log" 2>&1
) &
view_pid=$!
view_url=""
for i in 1 2 3 4 5 6 7 8 9 10; do
    sleep 0.1
    if [ -s "$TMP/wt-view.log" ]; then
        view_url=$(tr -d '\r' <"$TMP/wt-view.log" | head -n 1)
        break
    fi
done
[ -n "$view_url" ] || fail "view server did not start for the worktree failure case"
wt_origin=$(printf '%s' "$view_url" | cut -d/ -f1-3)
wt_code=$(curl -s -o "$TMP/wt-run.json" -w "%{http_code}" -X POST -H "Origin: $wt_origin" -H 'Content-Type: application/json' -H 'X-Pin-Action: true' --data '{"use_worktree":true}' "${view_url}items/${wt_id}/run")
[ "$wt_code" = "500" ] || fail "expected 500 when worktree provisioning fails, got $wt_code"
assert_contains "$(cat "$TMP/wt-run.json")" '"error"'
[ ! -d "$TMP/no-commits/.pin_worktrees" ] || fail ".pin_worktrees was left behind after a failed worktree add"
assert_contains "$(PIN_VAULT="$TMP/wt-vault" "$PIN_BIN" read "$wt_id")" 'status: "created"'
stop_view

# ── Worktree run from main: CLI claim, transition, and agent confirmation ──
mkdir -p "$TMP/wt-repo" "$TMP/wt-main-vault"
git -C "$TMP/wt-repo" init -q -b main
git -C "$TMP/wt-repo" config user.email "test@test.com"
git -C "$TMP/wt-repo" config user.name "Test User"
echo "hello main" >"$TMP/wt-repo/README.md"
git -C "$TMP/wt-repo" add README.md
git -C "$TMP/wt-repo" commit -q -m "initial main commit"

# 1. pin claim --worktree provisions worktree from main
claim_wt_item=$(PIN_VAULT="$TMP/wt-main-vault" PIN_PROJECT=wt-repo "$PIN_BIN" add '# Worktree claim item' --kind technical --type task --format json)
claim_wt_id=$(printf '%s' "$claim_wt_item" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')

(
    cd "$TMP/wt-repo"
    claim_res=$(PIN_VAULT="$TMP/wt-main-vault" "$PIN_BIN" claim "$claim_wt_id" --worktree --format json)
    assert_contains "$claim_res" '"status":"in_progress"'
    assert_contains "$claim_res" '"worktree":'
    [ -d "$TMP/wt-repo/.pin_worktrees/$claim_wt_id" ] || fail "worktree folder .pin_worktrees/$claim_wt_id was not created"
    wt_branch=$(git -C "$TMP/wt-repo/.pin_worktrees/$claim_wt_id" rev-parse --abbrev-ref HEAD)
    [ "$wt_branch" = "pin/$claim_wt_id" ] || fail "expected branch pin/$claim_wt_id, got $wt_branch"
)

# 2. pin transition --worktree provisions worktree
trans_wt_item=$(PIN_VAULT="$TMP/wt-main-vault" PIN_PROJECT=wt-repo "$PIN_BIN" add '# Worktree trans item' --kind technical --type task --format json)
trans_wt_id=$(printf '%s' "$trans_wt_item" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')

(
    cd "$TMP/wt-repo"
    trans_res=$(PIN_VAULT="$TMP/wt-main-vault" "$PIN_BIN" transition "$trans_wt_id" --to in_progress --worktree --format plain)
    assert_contains "$trans_res" "Worktree:"
    [ -d "$TMP/wt-repo/.pin_worktrees/$trans_wt_id" ] || fail "worktree folder .pin_worktrees/$trans_wt_id was not created"
)

# 3. POST /items/:id/run with confirm_worktree alias
confirm_wt_item=$(PIN_VAULT="$TMP/wt-main-vault" PIN_PROJECT=wt-repo "$PIN_BIN" add '# Confirm worktree item' --kind technical --type task --format json)
confirm_wt_id=$(printf '%s' "$confirm_wt_item" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')

(
    cd "$TMP/wt-repo"
    exec env PIN_VAULT="$TMP/wt-main-vault" "$PIN_BIN" view --no-open --format plain --acp-command "$TMP/mock-agent.sh" >"$TMP/wt-confirm-view.log" 2>&1
) &
view_pid=$!
view_url=""
for i in 1 2 3 4 5 6 7 8 9 10; do
    sleep 0.1
    if [ -s "$TMP/wt-confirm-view.log" ]; then
        view_url=$(tr -d '\r' <"$TMP/wt-confirm-view.log" | head -n 1)
        break
    fi
done
[ -n "$view_url" ] || fail "view server did not start for confirm_worktree"
confirm_origin=$(printf '%s' "$view_url" | cut -d/ -f1-3)
confirm_code=$(curl -s -o "$TMP/wt-confirm-run.json" -w "%{http_code}" -X POST -H "Origin: $confirm_origin" -H 'Content-Type: application/json' -H 'X-Pin-Action: true' --data '{"confirm_worktree":true}' "${view_url}items/${confirm_wt_id}/run")
[ "$confirm_code" = "200" ] || fail "expected 200 for confirm_worktree, got $confirm_code: $(cat "$TMP/wt-confirm-run.json")"
assert_contains "$(cat "$TMP/wt-confirm-run.json")" '"status":"started"'
stop_view

# 4. pin to-spec CLI command converts an idea into a task
idea_item=$(PIN_VAULT="$TMP/view-vault" PIN_PROJECT=view "$PIN_BIN" add '# Explore Caching Idea' --kind technical --type idea --format json)
idea_id=$(printf '%s' "$idea_item" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
[ -n "$idea_id" ] || fail "failed to create idea item"

to_spec_res=$(PIN_VAULT="$TMP/view-vault" "$PIN_BIN" to-spec "$idea_id" --title "Cached Cache Layer" --format json)
assert_contains "$to_spec_res" '"type":"task"'
assert_contains "$to_spec_res" '"title":"Cached Cache Layer"'

# 5. Idea Mode interactive run & chat endpoints
start_view "$TMP/view-vault" "$TMP/idea-view.log" "$TMP/mock-agent.sh"
idea_item2=$(PIN_VAULT="$TMP/view-vault" PIN_PROJECT=view "$PIN_BIN" add '# Broad Architecture Idea' --kind technical --type idea --format json)
idea_id2=$(printf '%s' "$idea_item2" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')

idea_origin=$(printf '%s' "$view_url" | cut -d/ -f1-3)
start_idea_code=$(curl -s -o "$TMP/idea-run.json" -w "%{http_code}" -X POST -H "Origin: $idea_origin" -H 'Content-Type: application/json' -H 'X-Pin-Action: true' --data '{"mode":"idea"}' "${view_url}items/${idea_id2}/run")
[ "$start_idea_code" = "200" ] || fail "expected 200 starting idea mode, got $start_idea_code: $(cat "$TMP/idea-run.json")"
assert_contains "$(cat "$TMP/idea-run.json")" '"status":"started"'

# Wait briefly for agent to complete turn 1 and enter waiting state
sleep 0.15

msg_code=$(curl -s -o "$TMP/idea-msg.json" -w "%{http_code}" -X POST -H "Origin: $idea_origin" -H 'Content-Type: application/json' -H 'X-Pin-Action: true' --data '{"message":"Let us use SQLite instead"}' "${view_url}items/${idea_id2}/message")
[ "$msg_code" = "200" ] || fail "expected 200 sending message to idea agent, got $msg_code: $(cat "$TMP/idea-msg.json")"
assert_contains "$(cat "$TMP/idea-msg.json")" '"status":"sent"'

finish_code=$(curl -s -o "$TMP/idea-finish.json" -w "%{http_code}" -X POST -H "Origin: $idea_origin" -H 'Content-Type: application/json' -H 'X-Pin-Action: true' "${view_url}items/${idea_id2}/finish")
[ "$finish_code" = "200" ] || fail "expected 200 finishing idea run, got $finish_code: $(cat "$TMP/idea-finish.json")"
assert_contains "$(cat "$TMP/idea-finish.json")" '"status":"finished"'

trans_code=$(curl -s -o "$TMP/idea-trans.json" -w "%{http_code}" -X POST -H "Origin: $idea_origin" -H 'Content-Type: application/json' -H 'X-Pin-Action: true' --data '{"action":"to_spec","title":"Agreed Spec Task"}' "${view_url}items/${idea_id2}/action")
[ "$trans_code" = "200" ] || fail "expected 200 for action to_spec, got $trans_code: $(cat "$TMP/idea-trans.json")"
assert_contains "$(cat "$TMP/idea-trans.json")" '"type":"task"'
assert_contains "$(cat "$TMP/idea-trans.json")" '"title":"Agreed Spec Task"'
stop_view

# 6. Auto-update check and update command / viewer endpoints
export PIN_UPDATE_CACHE="$TMP/mock_update_cache.json"
update_os=$(uname -s | tr '[:upper:]' '[:lower:]')
update_arch=$(uname -m)
target_asset="pin-linux-amd64"
case "$update_os" in
    linux)
        case "$update_arch" in
            x86_64) target_asset="pin-linux-amd64" ;;
            aarch64|arm64) target_asset="pin-linux-arm64" ;;
        esac
        ;;
    darwin)
        case "$update_arch" in
            x86_64) target_asset="pin-macos-amd64" ;;
            aarch64|arm64) target_asset="pin-macos-arm64" ;;
        esac
        ;;
esac

mkdir -p "$TMP/mock_assets"
mock_bin="$TMP/mock_assets/$target_asset"
printf '#!/bin/sh\necho "pin 2.3.0"\n' >"$mock_bin"
chmod +x "$mock_bin"

if command -v sha256sum >/dev/null 2>&1; then
    sha_hash=$(sha256sum "$mock_bin" | awk '{print $1}')
elif command -v shasum >/dev/null 2>&1; then
    sha_hash=$(shasum -a 256 "$mock_bin" | awk '{print $1}')
else
    fail "neither sha256sum nor shasum available for test"
fi
printf '%s  %s\n' "$sha_hash" "$target_asset" >"$TMP/mock_assets/$target_asset.sha256"

cat <<EOF >"$TMP/mock_release.json"
{
  "tag_name": "v2.3.0",
  "name": "Release 2.3.0",
  "html_url": "https://github.com/JoeriKaiser/pin/releases/tag/v2.3.0",
  "assets": [
    {
      "name": "$target_asset",
      "browser_download_url": "file://$TMP/mock_assets/$target_asset"
    },
    {
      "name": "$target_asset.sha256",
      "browser_download_url": "file://$TMP/mock_assets/$target_asset.sha256"
    }
  ]
}
EOF

cat <<EOF >"$TMP/mock_uptodate.json"
{
  "tag_name": "v2.2.0",
  "name": "Release 2.2.0",
  "html_url": "https://github.com/JoeriKaiser/pin/releases/tag/v2.2.0",
  "assets": []
}
EOF

# CLI check update (JSON)
check_json=$(PIN_UPDATE_URL="file://$TMP/mock_release.json" "$PIN_BIN" update --check --format json)
assert_contains "$check_json" '"update_available":true'
assert_contains "$check_json" '"latest_version":"2.3.0"'
assert_contains "$check_json" '"current_version":"2.2.0"'

# CLI check update (Plain)
check_plain=$(PIN_UPDATE_URL="file://$TMP/mock_release.json" "$PIN_BIN" update --check --format plain)
assert_contains "$check_plain" "Update available: v2.3.0 (current: v2.2.0)"
assert_contains "$check_plain" "Run 'pin update' to install the update."

# CLI check when already up to date
uptodate_out=$(PIN_UPDATE_URL="file://$TMP/mock_uptodate.json" "$PIN_BIN" update --check --format plain)
assert_contains "$uptodate_out" "pin is up to date (version 2.2.0)"

# CLI update installation
test_cli_target="$TMP/mock_cli_target_pin"
cp "$PIN_BIN" "$test_cli_target"
update_install_out=$(PIN_UPDATE_URL="file://$TMP/mock_release.json" PIN_TARGET_EXE="$test_cli_target" "$PIN_BIN" update --format json)
assert_contains "$update_install_out" '"success":true'
assert_contains "$update_install_out" '"updated_version":"2.3.0"'
assert_contains "$("$test_cli_target")" "pin 2.3.0"

# CLI update when already up to date without force
test_uptodate_install=$(PIN_UPDATE_URL="file://$TMP/mock_uptodate.json" "$PIN_BIN" update --format json)
assert_contains "$test_uptodate_install" '"installed":false'
assert_contains "$test_uptodate_install" '"message":"pin is already up to date"'

# Viewer update endpoints
export PIN_UPDATE_URL="file://$TMP/mock_release.json"
export PIN_TARGET_EXE="$TMP/mock_view_target_pin"
cp "$PIN_BIN" "$PIN_TARGET_EXE"

start_view "$TMP/view-vault" "$TMP/update-view.log" "$TMP/mock-agent.sh"
update_origin=$(printf '%s' "$view_url" | cut -d/ -f1-3)

# Test GET /update
view_get_update=$(curl -s -S "${view_url}update")
assert_contains "$view_get_update" '"update_available":true'
assert_contains "$view_get_update" '"latest_version":"2.3.0"'

# Test POST /update without Origin or Action header (expect 403 Forbidden)
post_forbidden_code=$(curl -s -o /dev/null -w "%{http_code}" -X POST "${view_url}update")
[ "$post_forbidden_code" = "403" ] || fail "expected 403 for POST /update without origin/header, got $post_forbidden_code"

# Test POST /update with valid Origin and Action header
post_update_out=$(curl -s -S -X POST -H "Origin: $update_origin" -H 'X-Pin-Action: true' "${view_url}update")
assert_contains "$post_update_out" '"success":true'
assert_contains "$post_update_out" '"updated_version":"2.3.0"'
assert_contains "$("$PIN_TARGET_EXE")" "pin 2.3.0"

stop_view
unset PIN_UPDATE_URL
unset PIN_TARGET_EXE
unset PIN_UPDATE_CACHE

# ==============================================================================
# Testing pin sync: regular merge, squash merge, worktree cleanup, dry-run, json
# ==============================================================================
sync_repo="$TMP/sync-repo"
sync_vault="$TMP/sync-vault"
mkdir -p "$sync_repo" "$sync_vault"

git -C "$sync_repo" init -q -b main
git -C "$sync_repo" config user.name "Tester"
git -C "$sync_repo" config user.email "test@example.com"
echo "initial" > "$sync_repo/README.md"
git -C "$sync_repo" add README.md
git -C "$sync_repo" commit -q -m "initial commit"

# 1. Regular merge item
reg_item=$(PIN_VAULT="$sync_vault" PIN_PROJECT=sync-repo "$PIN_BIN" add '# Regular merge item' --kind technical --type task --format json)
reg_id=$(printf '%s' "$reg_item" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
PIN_VAULT="$sync_vault" "$PIN_BIN" transition "$reg_id" --to review >/dev/null
(
    cd "$sync_repo"
    PIN_VAULT="$sync_vault" "$PIN_BIN" transition "$reg_id" --to review --worktree >/dev/null
)
[ -d "$sync_repo/.pin_worktrees/$reg_id" ] || fail "worktree .pin_worktrees/$reg_id not created"
echo "feature code" > "$sync_repo/.pin_worktrees/$reg_id/feature.txt"
git -C "$sync_repo/.pin_worktrees/$reg_id" add feature.txt
git -C "$sync_repo/.pin_worktrees/$reg_id" commit -q -m "feat: regular feature commit"
git -C "$sync_repo" merge --no-ff -q -m "Merge branch pin/$reg_id" "pin/$reg_id"

# 2. Squash merge item
sqsh_item=$(PIN_VAULT="$sync_vault" PIN_PROJECT=sync-repo "$PIN_BIN" add '# Squash merge item' --kind technical --type task --format json)
sqsh_id=$(printf '%s' "$sqsh_item" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
PIN_VAULT="$sync_vault" "$PIN_BIN" transition "$sqsh_id" --to review >/dev/null
(
    cd "$sync_repo"
    PIN_VAULT="$sync_vault" "$PIN_BIN" transition "$sqsh_id" --to review --worktree >/dev/null
)
[ -d "$sync_repo/.pin_worktrees/$sqsh_id" ] || fail "worktree .pin_worktrees/$sqsh_id not created"
echo "squash code" > "$sync_repo/.pin_worktrees/$sqsh_id/squash.txt"
git -C "$sync_repo/.pin_worktrees/$sqsh_id" add squash.txt
git -C "$sync_repo/.pin_worktrees/$sqsh_id" commit -q -m "feat: squash work"
echo "squash code" > "$sync_repo/squash.txt"
git -C "$sync_repo" add squash.txt
git -C "$sync_repo" commit -q -m "feat: squash merged feature ($sqsh_id)"

# 3. Dry run
dry_out=$(cd "$sync_repo" && PIN_VAULT="$sync_vault" "$PIN_BIN" sync --dry-run --format json)
assert_contains "$dry_out" '"action":"dry_run"'
assert_contains "$dry_out" "$reg_id"
assert_contains "$dry_out" "$sqsh_id"
[ -d "$sync_repo/.pin_worktrees/$reg_id" ] || fail "dry run removed regular worktree"
[ -d "$sync_repo/.pin_worktrees/$sqsh_id" ] || fail "dry run removed squash worktree"
assert_contains "$(PIN_VAULT="$sync_vault" "$PIN_BIN" read "$reg_id")" 'status: "review"'

# 4. Real sync
sync_out=$(cd "$sync_repo" && PIN_VAULT="$sync_vault" "$PIN_BIN" sync --format json)
assert_contains "$sync_out" '"action":"archived"'
assert_contains "$sync_out" '"resolution":"implemented"'
assert_contains "$sync_out" '"worktree_removed":true'
assert_contains "$sync_out" '"branch_deleted":true'

[ ! -d "$sync_repo/.pin_worktrees/$reg_id" ] || fail "pin sync left regular worktree behind"
[ ! -d "$sync_repo/.pin_worktrees/$sqsh_id" ] || fail "pin sync left squash worktree behind"

git -C "$sync_repo" rev-parse --verify "pin/$reg_id" 2>/dev/null && fail "pin sync left branch pin/$reg_id behind" || true
git -C "$sync_repo" rev-parse --verify "pin/$sqsh_id" 2>/dev/null && fail "pin sync left branch pin/$sqsh_id behind" || true

reg_meta=$(PIN_VAULT="$sync_vault" "$PIN_BIN" read "$reg_id")
assert_contains "$reg_meta" 'resolution: "implemented"'
assert_contains "$reg_meta" 'archived_at:'

sqsh_meta=$(PIN_VAULT="$sync_vault" "$PIN_BIN" read "$sqsh_id")
assert_contains "$sqsh_meta" 'resolution: "implemented"'
assert_contains "$sqsh_meta" 'archived_at:'

# 5. Subsequent sync
none_out=$(cd "$sync_repo" && PIN_VAULT="$sync_vault" "$PIN_BIN" sync --format plain)
assert_contains "$none_out" "No merged tickets to sync."

# 6. Viewer POST /sync endpoint
start_view "$sync_vault" "$TMP/sync-view.log" "$TMP/mock-agent.sh"
sync_origin=$(printf '%s' "$view_url" | cut -d/ -f1-3)
view_sync_res=$(curl -s -S -X POST -H "Origin: $sync_origin" -H 'Content-Type: application/json' -H 'X-Pin-Action: true' "${view_url}sync")
assert_contains "$view_sync_res" '"status":"ok"'
assert_contains "$view_sync_res" '"synced":[]'
stop_view
echo "CLI tests passed"
