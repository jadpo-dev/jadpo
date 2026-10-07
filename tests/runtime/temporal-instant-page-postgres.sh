#!/usr/bin/env bash
set -euo pipefail
for name in initdb pg_ctl createdb postgres bun; do command -v "$name" >/dev/null; done
repository=$(cd "$(dirname "$0")/../.." && pwd)
cluster=$(mktemp -d /tmp/jadpo-page-probe-pg.XXXXXX)
cleanup() { pg_ctl -D "$cluster/data" -m immediate -w stop >/dev/null 2>&1 || true; rm -rf "$cluster"; }
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
postgres --version
bun --version
initdb -D "$cluster/data" --username=jadpo_auth_test --auth=trust --no-locale --encoding=UTF8 >"$cluster/init.log"
port=$(bun --no-install --env-file=/dev/null -e 'const s=Bun.serve({hostname:"127.0.0.1",port:0,fetch:()=>new Response()});console.log(s.port);s.stop(true);')
pg_ctl -D "$cluster/data" -l "$cluster/server.log" -o "-h 127.0.0.1 -p $port -k $cluster -c log_statement=all" -w start >/dev/null
createdb -h 127.0.0.1 -p "$port" -U jadpo_auth_test jadpo_auth_test_runtime
cd "$repository"
unset DATABASE_URL SQLITE_PATH JADPO_DEBUG_TARGET_STACKS
set +e
if [[ "${1:-}" == "--effectful-limit" ]]; then
  shift
  JADPO_TEMPORAL_PAGE_PROBE_DATABASE_URL="postgres://jadpo_auth_test@127.0.0.1:$port/jadpo_auth_test_runtime" bun --no-install --env-file=/dev/null test ./tests/runtime/temporal-page-limit-order.test.ts "$@"
elif [[ "${1:-}" == "--effectful-write" ]]; then
  shift
  JADPO_INSTANT_PAGE_DATABASE_URL="postgres://jadpo_auth_test@127.0.0.1:$port/jadpo_auth_test_runtime" bun --no-install --env-file=/dev/null test ./tests/runtime/temporal-instant-page-effectful.test.ts "$@"
else
  JADPO_INSTANT_PAGE_DATABASE_URL="postgres://jadpo_auth_test@127.0.0.1:$port/jadpo_auth_test_runtime" bun --no-install --env-file=/dev/null test ./tests/runtime/temporal-instant-page-probe.test.ts "$@"
fi
status=$?
set -e
cleanup
trap - EXIT
test ! -e "$cluster"
printf '{"kind":"isolated_pg_page_probe_cluster_cleanup","cluster_removed":true,"test_exit":%s}\n' "$status"
exit "$status"
