#!/usr/bin/env bash
set -euo pipefail
repo=$(cd "$(dirname "$0")/../.." && pwd)
cluster=$(mktemp -d /private/tmp/jadpo-process-owner-pg.XXXXXX)
cleanup(){ pg_ctl -D "$cluster/data" -m immediate -w stop >/dev/null 2>&1 || true; rm -rf "$cluster"; }
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
postgres --version
bun --version
initdb -D "$cluster/data" --username=jadpo_auth_test --auth=trust --no-locale --encoding=UTF8 >"$cluster/init.log"
port=$(bun --no-install --env-file=/dev/null -e 'const s=Bun.serve({hostname:"127.0.0.1",port:0,fetch:()=>new Response()});console.log(s.port);s.stop(true);')
pg_ctl -D "$cluster/data" -l "$cluster/server.log" -o "-h 127.0.0.1 -p $port -k $cluster" -w start >/dev/null
createdb -h 127.0.0.1 -p "$port" -U jadpo_auth_test jadpo_auth_test_runtime
cd "$repo"
set +e
JADPO_READINESS_TEST_DATABASE_URL="postgres://jadpo_auth_test@127.0.0.1:$port/jadpo_auth_test_runtime" JADPO_READINESS_APPLICATION="${JADPO_READINESS_APPLICATION:?exact unchanged generated main path required}" bun --no-install --env-file=/dev/null test ./tests/runtime/process-owner.test.ts "$@"
status=$?
set -e
cleanup
trap - EXIT
test ! -e "$cluster"
printf '{"kind":"process_owner_pg_cleanup","cluster_removed":true,"test_exit":%s}\n' "$status"
exit "$status"
