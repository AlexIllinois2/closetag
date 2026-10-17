#!/usr/bin/env bash
# End-to-end smoke test for the closetag release binary.
set -u

BIN=/d/weiheng/proj/music/closetag/target/release/closetag.exe
BASE=http://127.0.0.1:17799
FIXTURE=/tmp/closetag-smoke-repo
FIXTURE_JSON=$(cygpath -m "$FIXTURE") # C:/.../Temp/closetag-smoke-repo, safe inside JSON
DATA=/tmp/closetag-smoke
LOG=/tmp/closetag-smoke.log
PASS=0
FAIL=0

check() {
  if [ "$2" = "0" ]; then
    echo "PASS: $1"
    PASS=$((PASS + 1))
  else
    echo "FAIL: $1"
    FAIL=$((FAIL + 1))
  fi
}

rm -rf "$FIXTURE" "$DATA"
mkdir -p "$FIXTURE/src" "$FIXTURE/node_modules" "$FIXTURE/.hidden"
echo 'export const a = 1' >"$FIXTURE/src/date-helper.ts"
echo 'export const b = 2' >"$FIXTURE/src/date-parser.ts"
echo '# hello' >"$FIXTURE/README.md"
echo 'junk' >"$FIXTURE/node_modules/skip-me.ts"
echo 'secret' >"$FIXTURE/.hidden/secret.ts"

"$BIN" --data-dir "$DATA" --port 17799 --no-browser >"$LOG" 2>&1 &
SERVER_PID=$!

READY=0
for _ in $(seq 1 40); do
  if curl -sf --max-time 2 "$BASE/api/about" >/dev/null 2>&1; then
    READY=1
    break
  fi
  sleep 0.25
done
check "server starts and answers /api/about" $((1 - READY))

curl -s "$BASE/" | grep -q 'id="root"'
check "GET / serves the embedded SPA" $?

# Regression guard: the SPA can only mount if its hashed assets resolve.
# rust-embed keys are paths relative to frontend/dist (assets/index-xxxx.js),
# so a route that only forwards the captured tail will 404 here and blank the page.
ASSET=$(curl -s "$BASE/" | sed -n 's/.*src="\.\/\(assets\/[^"]*\)".*/\1/p' | head -1)
[ -n "$ASSET" ] && [ "$(curl -s -o /dev/null -w '%{http_code}' "$BASE/$ASSET")" = "200" ]
check "GET /assets/<hashed>.js is reachable (white-screen regression)" $?

curl -s "$BASE/items" | grep -q 'id="root"'
check "SPA fallback for /items" $?

curl -s -o /dev/null -w '%{http_code}' "$BASE/api/nope" | grep -q 404
check "unknown /api path returns 404" $?

curl -s "$BASE/api/repos" | grep -q '^\[\]$'
check "repos initially empty" $?

REPO_JSON=$(curl -s -X POST "$BASE/api/repos" -H 'Content-Type: application/json' -d "{\"path\": \"$FIXTURE_JSON\"}")
echo "$REPO_JSON" | grep -q '"item_count":0'
check "POST /api/repos adds repo" $?
REPO_ID=$(echo "$REPO_JSON" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
echo "  repo id = $REPO_ID"

SCAN_JSON=$(curl -s -X POST "$BASE/api/repos/$REPO_ID/scan")
echo "$SCAN_JSON" | grep -q '"scanned":3'
check "scan finds 3 files (node_modules/.hidden skipped)" $?
echo "$SCAN_JSON" | grep -q '"added":3'
check "scan inserts 3 items" $?

ITEMS=$(curl -s "$BASE/api/repos/$REPO_ID/items")
echo "$ITEMS" | grep -q '"name":"date-helper"'
check "info_parse parses date-helper.ts into name=date-helper" $?
echo "$ITEMS" | grep -q '"total":3'
check "items total is 3" $?

TAG_JSON=$(curl -s -X POST "$BASE/api/repos/$REPO_ID/tags" -H 'Content-Type: application/json' -d '{"name":"utils","description":"tooling"}')
TAG_ID=$(echo "$TAG_JSON" | sed -n 's/.*"id":\([0-9]*\),"name":"utils".*/\1/p')
[ -n "$TAG_ID" ]
check "POST tags creates tag utils" $?

curl -s -X POST "$BASE/api/repos/$REPO_ID/tags" -H 'Content-Type: application/json' -d "{\"name\":\"date\",\"parent_id\":$TAG_ID}" | grep -q "\"parent_id\":$TAG_ID"
check "child tag keeps parent_id" $?

# items are serialized through serde_json::json!, so keys come out
# alphabetically: auth, created_at, id, name, path, tags, updated_at
ITEM_ID=$(echo "$ITEMS" | sed -n 's/.*"id":\([0-9]*\),"name":"date-helper","path":"src\/date-helper.ts".*/\1/p')
[ -n "$ITEM_ID" ]
check "locate item id for date-helper.ts" $?

curl -s -X POST "$BASE/api/repos/$REPO_ID/items/$ITEM_ID/tags" -H 'Content-Type: application/json' -d "{\"tag_ids\":[$TAG_ID]}" | grep -q '"name":"utils"'
check "attach tag to item" $?

curl -s "$BASE/api/repos/$REPO_ID/items?tag=$TAG_ID" | grep -q '"total":1'
check "filter items by tag" $?

GROUP_JSON=$(curl -s -X POST "$BASE/api/repos/$REPO_ID/groups" -H 'Content-Type: application/json' -d "{\"name\":\"g1\",\"includes\":[$TAG_ID],\"excludes\":[]}")
GROUP_ID=$(echo "$GROUP_JSON" | sed -n 's/.*"id":\([0-9]*\),"name":"g1".*/\1/p')
curl -s "$BASE/api/repos/$REPO_ID/groups/$GROUP_ID/items" | grep -q '"total":1'
check "group includes=[utils] matches 1 item" $?

curl -s -X POST "$BASE/api/repos/$REPO_ID/groups" -H 'Content-Type: application/json' -d '{"name":"all","includes":null,"excludes":[]}' | grep -q '"item_count":3'
check "group includes=null matches all 3 items" $?

# --- database export ---
CODE=$(curl -s -o /tmp/closetag-export.sqlite -w '%{http_code}' "$BASE/api/repos/$REPO_ID/export")
[ "$CODE" = "200" ]
check "GET /repos/{id}/export answers with the database" $?
head -c 16 /tmp/closetag-export.sqlite | grep -q "SQLite format 3"
check "exported file is a SQLite database" $?
# The live .sqlite is a shell while writes still sit in its -wal: the export has to be taken
# through SQLite (VACUUM INTO), so the scanned items must actually be in the bytes.
grep -qa "date-helper" /tmp/closetag-export.sqlite
check "export holds the scanned items, not only the empty main file" $?
rm -f /tmp/closetag-export.sqlite

# json! 序列化的对象键按字母序输出：auth 在 name 前
curl -s -X POST "$BASE/api/infoparse/preview" -H 'Content-Type: application/json' -d '{"path":"manga/Naruto - Kishimoto.cbz"}' | grep -q '"auth":"Kishimoto","name":"Naruto"'
check "infoparse preview endpoint" $?

curl -s "$BASE/api/config" | grep -q 'closetag-smoke-repo'
check "config round-trips repo path" $?

# --- per-repo rules ---
curl -s "$BASE/api/repos/$REPO_ID/settings" | grep -q '"using_global":true'
check "repo settings start by inheriting the global defaults" $?

curl -s -X PUT "$BASE/api/repos/$REPO_ID/settings" -H 'Content-Type: application/json' \
  -d '{"include_subfixs":["RS"],"info_parse_fns":[{"split":"_","name":"${arr[1]||arr[0]}","auth":"${arr[0]}"}]}' |
  grep -q '"using_global":false'
check "PUT repo settings saves rules owned by that repo" $?

curl -s "$BASE/api/repos/$REPO_ID/settings" | grep -q '"include_subfixs":\[".rs"\]'
check "repo settings are normalized (.RS -> .rs) and reloaded" $?

curl -s -X DELETE "$BASE/api/repos/$REPO_ID/settings" | grep -q '"using_global":true'
check "DELETE repo settings restores inheritance" $?

curl -s -X PUT "$BASE/api/repos/$REPO_ID/items/$ITEM_ID" -H 'Content-Type: application/json' -d '{"path":"src/date-helper2.ts"}' | grep -q 'date-helper2.ts'
check "update item path" $?

curl -s -X DELETE "$BASE/api/repos/$REPO_ID/items/$ITEM_ID" | grep -q 'deleted'
check "delete item" $?

curl -s -X DELETE "$BASE/api/repos/$REPO_ID" | grep -q 'removed'
check "remove repo (drops its db)" $?

# --- websocket + auto-exit ---
bun "$(dirname "$0")/ws-client.ts" 17799 1500
sleep 1
EXITED=0
for _ in $(seq 1 30); do
  if ! curl -sf --max-time 2 "$BASE/api/about" >/dev/null 2>&1; then
    EXITED=1
    break
  fi
  sleep 0.5
done
check "server auto-exits ~3s after the page disconnects" $((1 - EXITED))

echo "----------------------------------------"
echo "RESULT: PASS=$PASS FAIL=$FAIL"
if [ "$FAIL" != "0" ]; then
  echo "--- server log ---"
  cat "$LOG"
fi
kill $SERVER_PID 2>/dev/null
rm -rf "$FIXTURE" "$DATA"
[ "$FAIL" = "0" ]
