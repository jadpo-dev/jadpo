#!/usr/bin/env bash
set -euo pipefail

mode=${1:-}
case "$mode" in
  authentication|service-authentication|golden-service-credentials|golden-todo-routes|jwt-authentication|persistence|readiness|validation-persistence|validation-auth-policy|entity-dossier|lifecycle|lifecycle-maintenance|delivery-hook-components|delivery-completion-representation|relationship-conflicts|configuration-preflight|deployment-readiness) ;;
  *) echo 'Usage: bash tests/runtime/postgres.sh authentication|service-authentication|golden-service-credentials|golden-todo-routes|jwt-authentication|persistence|readiness|validation-persistence|validation-auth-policy|entity-dossier|lifecycle|lifecycle-maintenance|delivery-hook-components|delivery-completion-representation|relationship-conflicts|configuration-preflight|deployment-readiness' >&2; exit 2 ;;
esac
for executable in initdb pg_ctl createdb postgres bun; do
  command -v "$executable" >/dev/null || { echo "Required executable: $executable" >&2; exit 1; }
done
repository=$(cd "$(dirname "$0")/../.." && pwd)
cluster=$(mktemp -d /tmp/jadpo-validation-postgres.XXXXXX)
cleanup() {
  pg_ctl -D "$cluster/data" -m immediate -w stop >/dev/null 2>&1 || true
  rm -rf "$cluster"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
postgres --version
bun --version
initdb -D "$cluster/data" --username=jadpo_auth_test --auth=trust --no-locale --encoding=UTF8 >"$cluster/init.log"
port=$(bun --no-install --env-file=/dev/null -e 'const server = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: () => new Response() }); console.log(server.port); server.stop(true);')
if ! pg_ctl -D "$cluster/data" -l "$cluster/server.log" -o "-h 127.0.0.1 -p $port -k $cluster" -w start >/dev/null; then
  cat "$cluster/server.log" >&2
  exit 1
fi
createdb -h 127.0.0.1 -p "$port" -U jadpo_auth_test jadpo_auth_test_runtime
cd "$repository"
unset DATABASE_URL SQLITE_PATH JADPO_AUTH_TEST_DATABASE_URL JADPO_SERVICE_AUTH_DATABASE_URL JADPO_GOLDEN_SERVICE_DATABASE_URL JADPO_JWT_AUTH_DATABASE_URL JADPO_READINESS_TEST_DATABASE_URL JADPO_READINESS_AUTH_DATABASE_URL JADPO_READINESS_RECOVERY_DATABASE_URL JADPO_READINESS_CONTROL_DATABASE_URL JADPO_VALIDATION_PERSISTENCE_DATABASE_URL JADPO_VALIDATION_AUTH_POLICY_DATABASE_URL JADPO_ENTITY_DOSSIER_DATABASE_URL JADPO_LIFECYCLE_DATABASE_URL JADPO_DELIVERY_HOOK_DATABASE_URL JADPO_DEBUG_TARGET_STACKS JADPO_CONFIG_PREFLIGHT_DATABASE_URL JADPO_DEPLOYMENT_READINESS_DATABASE_URL JADPO_DEPLOYMENT_PORT
if [[ "$mode" == authentication ]]; then
  JADPO_AUTH_TEST_DATABASE_URL="postgres://jadpo_auth_test@127.0.0.1:$port/jadpo_auth_test_runtime" \
    bun --no-install --env-file=/dev/null test ./tests/runtime/first-party-authentication.test.ts
elif [[ "$mode" == service-authentication ]]; then
  JADPO_SERVICE_AUTH_DATABASE_URL="postgres://jadpo_auth_test@127.0.0.1:$port/jadpo_auth_test_runtime" \
    bun --no-install --env-file=/dev/null test ./tests/runtime/service-authentication.test.ts
elif [[ "$mode" == golden-service-credentials ]]; then
  JADPO_GOLDEN_SERVICE_DATABASE_URL="postgres://jadpo_auth_test@127.0.0.1:$port/jadpo_auth_test_runtime" \
    bun --no-install --env-file=/dev/null test ./tests/runtime/golden-service-credentials.test.ts
elif [[ "$mode" == golden-todo-routes ]]; then
  DATABASE_URL="postgres://jadpo_auth_test@127.0.0.1:$port/jadpo_auth_test_runtime" \
    bun --no-install --env-file=/dev/null test ./tests/runtime/golden-todo-postgres.test.ts
elif [[ "$mode" == jwt-authentication ]]; then
  JADPO_JWT_AUTH_DATABASE_URL="postgres://jadpo_auth_test@127.0.0.1:$port/jadpo_auth_test_runtime" \
    bun --no-install --env-file=/dev/null test ./tests/runtime/jwt-integration.test.ts
elif [[ "$mode" == validation-persistence ]]; then
  JADPO_VALIDATION_PERSISTENCE_DATABASE_URL="postgres://jadpo_auth_test@127.0.0.1:$port/jadpo_auth_test_runtime" \
    bun --no-install --env-file=/dev/null test ./tests/runtime/validation-persistence.test.ts
elif [[ "$mode" == delivery-completion-representation ]]; then
  JADPO_DELIVERY_HOOK_DATABASE_URL="postgres://jadpo_auth_test@127.0.0.1:$port/jadpo_auth_test_runtime" \
    bun --no-install --env-file=/dev/null test ./tests/runtime/delivery-completion-representation.test.ts
elif [[ "$mode" == delivery-hook-components ]]; then
  JADPO_DELIVERY_HOOK_DATABASE_URL="postgres://jadpo_auth_test@127.0.0.1:$port/jadpo_auth_test_runtime" \
    bun --no-install --env-file=/dev/null test ./tests/runtime/delivery-hook-components.test.ts
elif [[ "$mode" == validation-auth-policy ]]; then
  JADPO_VALIDATION_AUTH_POLICY_DATABASE_URL="postgres://jadpo_auth_test@127.0.0.1:$port/jadpo_auth_test_runtime" \
    bun --no-install --env-file=/dev/null test ./tests/runtime/validation-auth-policy.test.ts
elif [[ "$mode" == entity-dossier ]]; then
  JADPO_ENTITY_DOSSIER_DATABASE_URL="postgres://jadpo_auth_test@127.0.0.1:$port/jadpo_auth_test_runtime" \
    bun --no-install --env-file=/dev/null test ./tests/runtime/entity-dossier.test.ts
elif [[ "$mode" == lifecycle-maintenance ]]; then
  JADPO_LIFECYCLE_DATABASE_URL="postgres://jadpo_auth_test@127.0.0.1:$port/jadpo_auth_test_runtime" \
    bun --no-install --env-file=/dev/null test ./tests/runtime/lifecycle-maintenance-scheduling.test.ts
elif [[ "$mode" == lifecycle ]]; then
  JADPO_LIFECYCLE_DATABASE_URL="postgres://jadpo_auth_test@127.0.0.1:$port/jadpo_auth_test_runtime" \
    bun --no-install --env-file=/dev/null test ./tests/runtime/entity-lifecycle.test.ts
elif [[ "$mode" == relationship-conflicts ]]; then
  JADPO_LIFECYCLE_DATABASE_URL="postgres://jadpo_auth_test@127.0.0.1:$port/jadpo_auth_test_runtime" \
    bun --no-install --env-file=/dev/null test ./tests/runtime/relationship-conflicts.test.ts
elif [[ "$mode" == configuration-preflight ]]; then
  JADPO_CONFIG_PREFLIGHT_DATABASE_URL="postgres://jadpo_auth_test@127.0.0.1:$port/jadpo_auth_test_runtime" \
    bun --no-install --env-file=/dev/null test ./tests/runtime/configuration-preflight.test.ts
elif [[ "$mode" == deployment-readiness ]]; then
  JADPO_DEPLOYMENT_READINESS_DATABASE_URL="postgres://jadpo_auth_test@127.0.0.1:$port/jadpo_auth_test_runtime" \
    bun --no-install --env-file=/dev/null test ./tests/runtime/deployment-readiness.test.ts
elif [[ "$mode" == readiness ]]; then
  # These fixture applications have incompatible User layouts. Keep their
  # schemas separate inside the same disposable test cluster.
  createdb -h 127.0.0.1 -p "$port" -U jadpo_auth_test jadpo_readiness_auth_runtime
  createdb -h 127.0.0.1 -p "$port" -U jadpo_auth_test jadpo_readiness_recovery_runtime
  JADPO_READINESS_TEST_DATABASE_URL="postgres://jadpo_auth_test@127.0.0.1:$port/jadpo_auth_test_runtime" \
    JADPO_READINESS_AUTH_DATABASE_URL="postgres://jadpo_auth_test@127.0.0.1:$port/jadpo_readiness_auth_runtime" \
    JADPO_READINESS_RECOVERY_DATABASE_URL="postgres://jadpo_auth_test@127.0.0.1:$port/jadpo_readiness_recovery_runtime" \
    JADPO_READINESS_CONTROL_DATABASE_URL="postgres://jadpo_auth_test@127.0.0.1:$port/postgres" \
    bun --no-install --env-file=/dev/null test ./tests/runtime/readiness-recovery.test.ts
else
  DATABASE_URL="postgres://jadpo_auth_test@127.0.0.1:$port/jadpo_auth_test_runtime" \
    bun --no-install --env-file=/dev/null test ./tests/runtime/persistence-postgres.test.ts
fi
