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

"$PIN_BIN" --help >/dev/null 2>&1
assert_contains "$("$PIN_BIN" --version)" "pin 2.0.0"

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

# 2. View launch with --no-open and plain format
mkdir -p "$TMP/view-vault"
view_item=$(PIN_VAULT="$TMP/view-vault" PIN_PROJECT=view "$PIN_BIN" add '# View test' --kind technical --priority low --format json)
view_id=$(printf '%s' "$view_item" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')

view_out="$TMP/view.log"
view_err="$TMP/view.err"
# Launch in background
PIN_VAULT="$TMP/view-vault" "$PIN_BIN" view --no-open --format plain >"$view_out" 2>"$view_err" &
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
curl -s -S "$url"app.js >"$TMP/app.js"
assert_contains "$(cat "$TMP/app.js")" 'DOMPurify.sanitize'
assert_contains "$(cat "$TMP/app.js")" 'bodyWithoutDuplicateTitle'

origin=$(printf '%s' "$url" | cut -d/ -f1-3)
mutation_code=$(curl -s -o /dev/null -w "%{http_code}" -X POST -H 'Content-Type: application/json' --data '{"action":"transition","to":"planned"}' "${url}items/${view_id}/action")
[ "$mutation_code" = "403" ] || fail "viewer accepted a mutation without same-origin headers"
mutation=$(curl -s -S -X POST -H "Origin: $origin" -H 'Content-Type: application/json' -H 'X-Pin-Action: true' --data '{"action":"transition","to":"planned","actor":"human:test","expect_revision":1}' "${url}items/${view_id}/action")
assert_contains "$mutation" '"status":"planned"'
curl -s -S "${url}data.json" | grep -q '"status":"planned"' || fail "viewer data did not refresh after mutation"

# Test POST items (quick-add from viewer)
created_from_view=$(curl -s -S -X POST -H "Origin: $origin" -H 'Content-Type: application/json' -H 'X-Pin-Action: true' --data '{"title":"Created from viewer","type":"task","status":"created","project":"view"}' "${url}items")
assert_contains "$created_from_view" '"title":"Created from viewer"'
assert_contains "$created_from_view" '"status":"created"'
curl -s -S "${url}data.json" | grep -q '"title":"Created from viewer"' || fail "viewer data did not contain newly created item"

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

kill "$view_pid" 2>/dev/null || true
wait "$view_pid" 2>/dev/null || true
view_pid=

echo "CLI tests passed"
