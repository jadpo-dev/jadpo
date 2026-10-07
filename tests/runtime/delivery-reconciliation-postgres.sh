#!/usr/bin/env bash
set -euo pipefail
repository=$(cd "$(dirname "$0")/../.." && pwd)
cluster=$(mktemp -d /tmp/jadpo-rm207-reconcile02.XXXXXX)
cleanup(){
 local original=$? stopcode=0 backend=""
 if [[ -f "$cluster/data/postmaster.pid" ]]; then backend=$(head -n1 "$cluster/data/postmaster.pid"); fi
 pg_ctl -D "$cluster/data" -m immediate -w stop >/dev/null 2>&1 || stopcode=$?
 if [[ -n "${JADPO_RECONCILIATION_CAPTURE:-}" ]]; then
  mkdir -p "$JADPO_RECONCILIATION_CAPTURE"
  [[ ! -f "$cluster/server.log" ]] || cp "$cluster/server.log" "$JADPO_RECONCILIATION_CAPTURE/postgres-native.log"
 fi
 rm -rf "$cluster"
 if [[ -n "${JADPO_RECONCILIATION_CAPTURE:-}" ]]; then
  python3 - "$backend" "${port:-0}" "$stopcode" "$cluster" "$JADPO_RECONCILIATION_CAPTURE/cluster-cleanup.json" <<'PYCODE'
import json,sys,os,socket,errno
pid,port,code,cluster,out=sys.argv[1:];gone=True
if pid:
 try:os.kill(int(pid),0);gone=False
 except ProcessLookupError:pass
sock=socket.socket();result=sock.connect_ex(('127.0.0.1',int(port)));sock.close()
proof={'postmasterPID':pid,'postmasterGone':gone,'formerPort':int(port),'formerPortECONNREFUSED':result==errno.ECONNREFUSED,'pgStopExit':int(code),'clusterRemoved':not os.path.exists(cluster)}
open(out,'w').write(json.dumps(proof,indent=2)+'\n')
assert gone and int(code)==0 and proof['clusterRemoved'] and proof['formerPortECONNREFUSED'],proof
PYCODE
 fi
 return "$original"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
initdb -D "$cluster/data" --username=jadpo_auth_test --auth=trust --no-locale --encoding=UTF8 > "$cluster/init.log"
port=$(bun --no-install --env-file=/dev/null -e 'const s=Bun.serve({hostname:"127.0.0.1",port:0,fetch:()=>new Response()});console.log(s.port);s.stop(true);')
pg_ctl -D "$cluster/data" -l "$cluster/server.log" -o "-h 127.0.0.1 -p $port -k $cluster -c log_statement=all" -w start >/dev/null
createdb -h 127.0.0.1 -p "$port" -U jadpo_auth_test jadpo_auth_test_rm207
cd "$repository"
suite=${1:-./tests/runtime/delivery-reconciliation.test.ts}
if [[ $# -gt 0 ]]; then shift; fi
set +e
JADPO_RECONCILIATION_DATABASE_URL="postgres://jadpo_auth_test@127.0.0.1:$port/jadpo_auth_test_rm207" bun --no-install --env-file=/dev/null test "$suite" "$@"
code=$?
set -e
if [[ -n "${JADPO_RECONCILIATION_CAPTURE:-}" ]]; then
 mkdir -p "$JADPO_RECONCILIATION_CAPTURE"
 psql "postgres://jadpo_auth_test@127.0.0.1:$port/jadpo_auth_test_rm207" -Atc "SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND pid <> pg_backend_pid()" > "$JADPO_RECONCILIATION_CAPTURE/post-suite-backends.txt"
 [[ $(cat "$JADPO_RECONCILIATION_CAPTURE/post-suite-backends.txt") == 0 ]]
 cp "$cluster/server.log" "$JADPO_RECONCILIATION_CAPTURE/postgres-native.log"
fi
exit "$code"
