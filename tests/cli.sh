#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT INT TERM

PIN_BIN=${PIN_BIN:-"$TMP/pin"}
if [ ! -x "$PIN_BIN" ]; then
    zig build-exe "$ROOT/main.zig" -O Debug -femit-bin="$PIN_BIN"
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
assert_contains "$("$PIN_BIN" --version)" "pin 0.4.0"

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
HOME="$TMP/home" "$PIN_BIN" read "$(printf '%s' "$created" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')" | grep -q '^schema: 1$'

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

echo "CLI tests passed"
