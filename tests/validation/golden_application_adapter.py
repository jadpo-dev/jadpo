#!/usr/bin/env python3
"""Formal case-kind drivers backed by retained migrated-app checkpoint traces.

The checkpoint performs one real capture per backend. Drivers then derive each
case result from its raw response/statement record; they never copy a pass label.
This module deliberately registers SQLite only until the candidate's configured
PostgreSQL URL is the URL used by the application itself.
"""
from __future__ import annotations

from copy import deepcopy
import hashlib
import inspect
import json
import os
from pathlib import Path
import re
import subprocess
import sys
from typing import Callable


class AdapterSetupError(RuntimeError):
    """Safe setup failure; never includes process or provider output."""


class CaseNotSupported(RuntimeError):
    """A raw checkpoint did not execute this exact acceptance ID."""


APPLICATION_BACKENDS = ('sqlite', 'postgres')


class DriverRegistry(dict):
    """Driver mapping plus replay metadata for the owning runner to retain."""

    def __init__(self, *args, metadata: dict):
        super().__init__(*args)
        self.metadata = metadata
        self.source_bindings = metadata
        self.checkpoint_dir = metadata['checkpoint_path']
        self.blocked_backends = metadata['blocked_backends']
        self.checkpoint_disposition = metadata['checkpoint_disposition']


def _digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _hash_tree(directory: Path, excluded_parts: tuple[str, ...] = ()) -> dict[str, str]:
    result = {}
    for path in sorted(directory.rglob('*')):
        if not path.is_file() or any(part in excluded_parts for part in path.relative_to(directory).parts):
            continue
        result[str(path.relative_to(directory))] = _digest(path)
    return result


def _load_modules(root: Path):
    tools = root / 'tools'
    if str(tools) not in sys.path:
        sys.path.insert(0, str(tools))
    import golden_cases  # type: ignore[import-not-found]
    import golden_checkpoint  # type: ignore[import-not-found]
    return golden_cases, golden_checkpoint


def _relative_path(root: Path, path: Path) -> str:
    return os.path.relpath(path.resolve(), root.resolve())


def _checkpoint_contract(harness, checkpoint, root: Path, report: dict | None,
                         supported_ids: list[str]) -> tuple[dict, dict | None]:
    if (report is not None and 'acceptance_successor' in report
            and hasattr(checkpoint, 'report_contract')):
        return checkpoint.report_contract(report, root), report['acceptance_successor']
    if 'JOB-001' in supported_ids and hasattr(harness, 'accepted_job001_contract'):
        return harness.accepted_job001_contract(root)
    return harness.read_contract(root), None


_RETAINED_GENERATED_ORIGINALS = (
    ('target/persistence.ts', 'original-persistence.ts'),
    ('target/first-party-authentication.ts', 'original-authentication.ts'),
    ('target/jwt-authentication.ts', 'original-jwt-authentication.ts'),
    ('target/app.ts', 'original-app.ts'),
)


def _original_generated_hashes(checkpoint_dir: Path) -> dict[str, str]:
    actual = _hash_tree(checkpoint_dir / 'application/build', ('node_modules',))
    bound = 0
    for generated, retained in _RETAINED_GENERATED_ORIGINALS:
        path = checkpoint_dir / retained
        if path.is_file():
            actual[generated] = _digest(path)
            bound += 1
    if not bound:
        raise AdapterSetupError('Missing retained originals for generated artifacts modified by the checkpoint')
    return actual


def _checkpoint_file_metadata(root: Path, checkpoint_dir: Path, dependencies: Path,
                              harness, report: dict | None, raw_ids: dict[str, list[str]]) -> dict:
    generated = _hash_tree(checkpoint_dir / 'application/build', ('node_modules',))
    originals = {retained: _digest(checkpoint_dir / retained)
                 for _generated, retained in _RETAINED_GENERATED_ORIGINALS
                 if (checkpoint_dir / retained).is_file()}
    if not originals:
        raise AdapterSetupError('No retained generated originals are available to bind')
    raws = {f'{backend}-raw.json': _digest(checkpoint_dir / f'{backend}-raw.json')
            for backend in raw_ids}
    tls = {}
    for name in ('jwks-fixture-cert.pem', 'jwks-fixture-key.pem'):
        path = checkpoint_dir / name
        if path.is_file():
            tls[name] = _digest(path)
    owner_attempt = None
    if report is not None:
        owner_attempt = report.get('checkpoint', {}).get('owner_attempt')
    if owner_attempt is None:
        owner_attempt_path = checkpoint_dir / 'owner-attempt-probe.json'
        if owner_attempt_path.is_file():
            try:
                import golden_owner_attempt  # type: ignore[import-not-found]
                owner_attempt = golden_owner_attempt.binding(checkpoint_dir)
            except Exception as error:
                raise AdapterSetupError(f'Cannot bind retained owner attempt ({type(error).__name__})') from None
    return {
        'checkpoint_path': _relative_path(root, checkpoint_dir),
        'dependency_path': _relative_path(root, dependencies),
        'checkpoint_report': ({'path': _relative_path(root, checkpoint_dir / 'golden-checkpoint.json'),
                              'sha256': _digest(checkpoint_dir / 'golden-checkpoint.json')}
                             if (checkpoint_dir / 'golden-checkpoint.json').is_file() else None),
        'raw_files': raws,
        'generated_instrumented': generated,
        'generated_original': _original_generated_hashes(checkpoint_dir),
        'generated_original_retained_files': originals,
        'dependency_files': _hash_tree(dependencies / 'node_modules' / 'jose'),
        'tls_fixture': tls,
        'owner_attempt': owner_attempt,
        'captured_supported_ids_by_backend': raw_ids,
    }


def _uuid(value: object) -> bool:
    return isinstance(value, str) and re.fullmatch(
        r'[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}', value) is not None


def postgres_configuration_is_bound(source: str) -> bool:
    """Require app configuration and SQL adapter to share the live PG URL."""
    return (
        'https://db.test/golden' not in source
        and re.search(r'DATABASE_URL\s*:\s*Bun\.env\.DATABASE_URL!?', source) is not None
        and re.search(r'new SQL\(\{\s*url:\s*Bun\.env\.DATABASE_URL', source) is not None
    )


def validate_backend_registration(root: Path, metadata: dict) -> bool:
    """Re-derive backend eligibility instead of trusting report blocker labels."""
    root = root.resolve()
    registered = metadata.get('registered_backends')
    formal = metadata.get('supported_ids_by_backend')
    registration = metadata.get('registered_supported_ids_by_backend')
    blocked = metadata.get('blocked_backends')
    backends = set(APPLICATION_BACKENDS)
    if (not isinstance(registered, list) or len(registered) != len(set(registered))
            or not set(registered).issubset(backends) or not isinstance(formal, dict)
            or not isinstance(registration, dict) or not isinstance(blocked, dict)):
        raise AdapterSetupError('Backend registration metadata is malformed')
    for backend in backends:
        supported_ids = formal.get(backend)
        registered_ids = registration.get(backend)
        if (not isinstance(supported_ids, list) or len(supported_ids) != len(set(supported_ids))
                or not isinstance(registered_ids, list) or len(registered_ids) != len(set(registered_ids))):
            raise AdapterSetupError('Backend case registration inventory is malformed')
        if backend in registered:
            if backend in blocked or set(registered_ids) != set(supported_ids):
                raise AdapterSetupError('Registered backend conflicts with its blocker or support inventory')
        elif registered_ids or (supported_ids and backend not in blocked):
            raise AdapterSetupError('Unregistered supported backend lacks an explicit blocker')
    if 'postgres' in registered:
        checkpoint_source = root / 'tools/golden_http_checkpoint.ts'
        if not checkpoint_source.is_file() or not postgres_configuration_is_bound(checkpoint_source.read_text()):
            raise AdapterSetupError('PostgreSQL cannot be registered while app configuration uses a different URL')
    return True


def require_safe_boundary_identity(raw: list, expected_source_revision: str | None = None) -> str:
    """Require BOUNDARY's auth-fault event to retain checked durable identity.

    The provider response remains uncertain after authentication refusal. Its
    safe event may expose the checked binding and request identity, but no
    provider body, SQL, exception text, stack, or credential.
    """
    if len(raw) != 1 or not isinstance(raw[0], dict) or 'boundaryVariants' not in raw[0]:
        raise ValueError('Missing exact native boundary capture')
    capture = raw[0]
    variants = capture['boundaryVariants']
    if not isinstance(variants, list) or len(variants) != 2:
        raise ValueError('Missing native boundary variants')
    provider = variants[1]
    binding = capture.get('binding')
    if binding != 'job.overdue_reminders.delivery.reminder_v1':
        raise ValueError('Missing checked job binding')
    calls = provider.get('providerCalls')
    durable = provider.get('durableAfter')
    if not isinstance(calls, list) or len(calls) != 1 or not isinstance(durable, dict):
        raise ValueError('Missing provider-fault and durable delivery evidence')
    intents = durable.get('intents')
    claims = durable.get('claims')
    if not isinstance(intents, list) or len(intents) != 1 or not isinstance(claims, list) or len(claims) != 1:
        raise ValueError('Missing durable uncertain intent/claim')
    intent_id = intents[0].get('intent_id')
    if (not _uuid(intent_id) or intents[0].get('state') != 'outcome_unknown'
            or claims[0].get('intent_id') != intent_id or claims[0].get('invocations') != 1
            or claims[0].get('possible_dispatch') != 1
            or calls[0].get('idempotencyKey') != intent_id
            or calls[0].get('body', {}).get('idempotency_key') != intent_id):
        raise ValueError('Provider request is not bound to the one durable unknown intent')

    events = provider.get('operationalEvents')
    if not isinstance(events, list) or [event.get('eventName') for event in events] != [
            'transaction.attempt', 'service.attempt', 'operation.failed']:
        raise ValueError('Missing exact bounded provider failure event sequence')
    attempt, failed = events[1], events[2]
    if (attempt.get('operationId') != intent_id or attempt.get('eventName') != 'service.attempt'
            or attempt.get('attempt') != 1 or attempt.get('outcome') != 'misconfigured'):
        raise ValueError('Provider attempt identity differs from the durable intent')
    expected_revision = expected_source_revision or capture.get('sourceRevision')
    request_id = 'job_' + intent_id
    if (set(failed) != {'schemaVersion', 'kind', 'eventName', 'classification', 'requestId',
                        'traceId', 'semanticOperationId', 'sourceRevision', 'attributes'}
            or failed.get('schemaVersion') != 1 or failed.get('kind') != 'operational_log_event'
            or failed.get('eventName') != 'operation.failed'
            or failed.get('classification') != 'RUNTIME_SERVICE_MISCONFIGURED'
            or failed.get('requestId') != request_id or failed.get('traceId') is not None
            or failed.get('semanticOperationId') != 'job:' + binding
            or not isinstance(expected_revision, str) or failed.get('sourceRevision') != expected_revision
            or failed.get('attributes') != {}):
        raise ValueError('Provider fault event lacks checked binding/request/source identity')
    envelope = provider.get('nativeHostOutcome', {}).get('failures', [])
    if (len(envelope) != 1 or envelope[0].get('error', {}).get('request_id') != request_id):
        raise ValueError('Safe event request identity differs from the closed failure envelope')
    serialized = json.dumps(events, sort_keys=True)
    forbidden = ('CHECKPOINT_PRIVATE_PROVIDER_AUTH_HEADER_STACK', 'Authorization:', 'Bearer ',
                 'postgres://', 'SELECT ', 'INSERT ', 'UPDATE ', 'DELETE ', 'ServiceAdapterFault',
                 'stack', 'password')
    if any(marker.lower() in serialized.lower() for marker in forbidden):
        raise ValueError('Provider operational event contains private payload, query, stack or credential')
    return intent_id


def _normalize_boundary_for_legacy_deriver(raw: list, intent_id: str) -> list:
    """Project already-checked new identity into the older checkpoint schema.

    This compatibility view is used only after `require_safe_boundary_identity`
    verifies the original unmodified event and durable state.
    """
    normalized = deepcopy(raw)
    provider = normalized[0]['boundaryVariants'][1]
    for event in provider['operationalEvents']:
        if event.get('eventName') == 'operation.failed':
            event['requestId'] = intent_id
            event['semanticOperationId'] = normalized[0]['binding']
    provider['nativeHostOutcome']['failures'][0]['error']['request_id'] = intent_id
    return normalized


def load_drivers(root: Path, checkpoint_dir: Path, jwt_dependencies: Path,
                 register_postgres: bool = False) -> DriverRegistry:
    """Load retained traces or capture both backends once, then register drivers.

    PostgreSQL capture remains supporting evidence until `DATABASE_URL` supplied
    to the generated application is the same valid PostgreSQL URL used by its
    storage adapter. No placeholder URL can be promoted by this adapter.
    """
    root = root.resolve()
    checkpoint_dir = checkpoint_dir.resolve()
    jwt_dependencies = jwt_dependencies.resolve()
    if not jwt_dependencies.is_dir():
        raise AdapterSetupError('Pinned JOSE dependency directory is unavailable')

    harness, checkpoint = _load_modules(root)
    source_before = harness.provenance(root)
    source_file = Path(__file__).resolve()
    focused_test = root / 'tests/validation/test_golden_application_adapter.py'
    driver_digest = _digest(source_file)
    focused_test_digest = _digest(focused_test) if focused_test.is_file() else None
    http_runner = (root / 'tools/golden_http_checkpoint.ts').read_text()
    if register_postgres and not postgres_configuration_is_bound(http_runner):
        raise AdapterSetupError('PostgreSQL is not registered: generated app configuration does not use the same live URL as the SQL adapter')
    completed = None
    if not checkpoint_dir.exists():
        command = [sys.executable, str(root / 'tools/golden_checkpoint.py'), '--output', str(checkpoint_dir),
                   '--jwt-dependencies', str(jwt_dependencies)]
        environment = {key: value for key, value in os.environ.items()
                       if key not in ('DATABASE_URL', 'SQLITE_PATH', 'JADPO_BIN', 'JADPO_DEBUG_TARGET_STACKS')}
        completed = subprocess.run(command, cwd=root, env=environment, stdout=subprocess.PIPE,
                                   stderr=subprocess.PIPE, check=False)
    if harness.provenance(root) != source_before:
        raise AdapterSetupError('Source/compiler changed during the real checkpoint capture')
    if _digest(source_file) != driver_digest or (focused_test_digest and _digest(focused_test) != focused_test_digest):
        raise AdapterSetupError('Driver source or focused tests changed during the real checkpoint capture')
    if not all((checkpoint_dir / f'{backend}-raw.json').is_file() for backend in harness.BACKENDS):
        raise AdapterSetupError(f'Real backend capture did not complete (checkpoint exit {completed.returncode})')

    raw_by_backend = {}
    raw_ids_by_backend = {}
    for backend in harness.BACKENDS:
        output = json.loads((checkpoint_dir / f'{backend}-raw.json').read_text())
        if output.get('backend') != backend:
            raise AdapterSetupError('Real backend trace has foreign adapter identity')
        entries = output.get('results')
        ids = [entry.get('id') for entry in entries] if isinstance(entries, list) else []
        if len(ids) != len(set(ids)) or not ids:
            raise AdapterSetupError(f'{backend} raw capture is missing, duplicated or foreign')
        raw_by_backend[backend] = {entry['id']: entry for entry in entries}
        raw_ids_by_backend[backend] = ids

    report_path = checkpoint_dir / 'golden-checkpoint.json'
    report = json.loads(report_path.read_text()) if report_path.is_file() else None
    if report is not None and report.get('provenance') != source_before:
        raise AdapterSetupError('Retained checkpoint report is stale for the current source/compiler')
    supported_by_backend = {
        backend: (list(report.get('checkpoint', {}).get('supported_ids', raw_ids_by_backend[backend]))
                  if report is not None else raw_ids_by_backend[backend])
        for backend in harness.BACKENDS
    }
    accepted_contract, successor_descriptor = _checkpoint_contract(
        harness, checkpoint, root, report, supported_by_backend['sqlite'])
    cases = {case['id']: case for case in accepted_contract['cases']}
    checkpoint_files = _checkpoint_file_metadata(root, checkpoint_dir, jwt_dependencies, harness,
                                                 report, raw_ids_by_backend)
    selected_backends = ('sqlite', 'postgres') if register_postgres else ('sqlite',)
    blocked = {} if register_postgres else {
        'postgres': 'The captured PostgreSQL SQL used an actual server, but generated app configuration still received the HTTPS placeholder DATABASE_URL; waiting for the owner’s URL binding decision.'}
    formal_supported = {
        backend: sorted(set(raw_ids_by_backend[backend]) & set(supported_by_backend[backend]))
        for backend in harness.BACKENDS
    }
    registered_supported = {
        backend: formal_supported[backend] if backend in selected_backends else []
        for backend in harness.BACKENDS
    }
    source_bindings = {
        'schema_version': 1,
        **checkpoint_files,
        'source_provenance': source_before,
        'instrumentation_sources': {
            name: digest for name, digest in source_before['files'].items()
            if name in ('tools/golden_checkpoint.py', 'tools/golden_http_checkpoint.ts',
                        'tools/golden_checkpoint_fetch.ts', 'tools/golden_sql_observer.ts')
        },
        'driver_source': {'path': _relative_path(root, source_file), 'sha256': driver_digest},
        'focused_driver_tests': ({'path': _relative_path(root, focused_test), 'sha256': focused_test_digest}
                                 if focused_test_digest else None),
        'acceptance_sha256': harness.digest(root / harness.ACCEPTANCE),
        'successor': successor_descriptor,
        'contract_clock': accepted_contract.get('clock'),
        'supported_ids_by_backend': formal_supported,
        'registered_supported_ids_by_backend': registered_supported,
        'registered_backends': list(selected_backends),
        'blocked_backends': blocked,
        'checkpoint_disposition': (
            'retained_checkpoint_validated' if report is not None and completed is None
            else 'checkpoint_completed_and_validated' if completed is not None and completed.returncode == 0
            else 'raw_capture_retained_but_legacy_checkpoint_validation_rejected' if completed is not None
            else 'retained_raw_capture_without_checkpoint_report'),
    }
    if _digest(source_file) != driver_digest or (focused_test_digest and _digest(focused_test) != focused_test_digest):
        raise AdapterSetupError('Driver source or focused tests changed during checkpoint loading')
    drivers: dict[tuple[str, str], Callable] = {}
    for backend in selected_backends:
        available = set(formal_supported[backend])
        if not available:
            continue
        available_kinds = {cases[case_id]['kind'] for case_id in available if case_id in cases}
        for kind in sorted(available_kinds):
            def drive(case: dict, requested_backend: str, *, _backend=backend, _kind=kind):
                if requested_backend != _backend or case.get('kind') != _kind:
                    raise ValueError('Driver received a foreign backend or kind')
                return derive_observations(root, source_bindings, case, requested_backend)
            drivers[(backend, kind)] = drive
    return DriverRegistry(drivers, metadata=source_bindings)


def validate_bindings(root: Path, metadata: dict) -> bool:
    """Rehash source, generated, raw, dependency and retained-report pins."""
    root = root.resolve()
    harness, _checkpoint = _load_modules(root)
    if metadata.get('schema_version') != 1 or harness.provenance(root) != metadata.get('source_provenance'):
        raise AdapterSetupError('Checkpoint source/compiler provenance is stale')
    validate_backend_registration(root, metadata)
    current_provenance = harness.provenance(root)['files']
    if any(current_provenance.get(name) != digest
           for name, digest in metadata.get('instrumentation_sources', {}).items()):
        raise AdapterSetupError('SQL or HTTP observation instrumentation source is stale')
    for key in ('driver_source', 'focused_driver_tests'):
        binding = metadata.get(key)
        if binding is None:
            continue
        path = root / binding['path']
        if not path.is_file() or _digest(path) != binding['sha256']:
            raise AdapterSetupError(f'{key} source binding is stale')
    checkpoint_dir = (root / metadata['checkpoint_path']).resolve()
    dependencies = (root / metadata['dependency_path']).resolve()
    report_binding = metadata.get('checkpoint_report')
    if report_binding is not None:
        report_path = root / report_binding['path']
        if not report_path.is_file() or _digest(report_path) != report_binding['sha256']:
            raise AdapterSetupError('Retained checkpoint report binding is stale')
    for name, expected in metadata.get('raw_files', {}).items():
        path = checkpoint_dir / name
        if not path.is_file() or _digest(path) != expected:
            raise AdapterSetupError(f'Retained raw trace is stale: {name}')
    if _hash_tree(checkpoint_dir / 'application/build', ('node_modules',)) != metadata.get('generated_instrumented'):
        raise AdapterSetupError('Instrumented generated artifacts are stale')
    if _original_generated_hashes(checkpoint_dir) != metadata.get('generated_original'):
        raise AdapterSetupError('Original generated artifacts are stale')
    for name, expected in metadata.get('generated_original_retained_files', {}).items():
        path = checkpoint_dir / name
        if not path.is_file() or _digest(path) != expected:
            raise AdapterSetupError(f'Retained original generated file is stale: {name}')
    if _hash_tree(dependencies / 'node_modules' / 'jose') != metadata.get('dependency_files'):
        raise AdapterSetupError('Pinned JOSE dependency files are stale')
    for name, expected in metadata.get('tls_fixture', {}).items():
        path = checkpoint_dir / name
        if not path.is_file() or _digest(path) != expected:
            raise AdapterSetupError(f'Retained TLS fixture binding is stale: {name}')
    for backend, ids in metadata.get('captured_supported_ids_by_backend', {}).items():
        raw = json.loads((checkpoint_dir / f'{backend}-raw.json').read_text())
        actual = [entry.get('id') for entry in raw.get('results', [])]
        if actual != ids:
            raise AdapterSetupError(f'{backend} raw result order or inventory changed')
    return True


def derive_observations(root: Path, metadata: dict, case: dict, backend: str) -> list:
    """Re-derive one case directly from the pinned backend raw trace."""
    root = root.resolve()
    validate_backend_registration(root, metadata)
    harness, checkpoint = _load_modules(root)
    supported = metadata.get('supported_ids_by_backend', {}).get(backend, [])
    registered = metadata.get('registered_supported_ids_by_backend', {}).get(backend, [])
    if backend not in metadata.get('registered_backends', []) or case.get('id') not in registered:
        raise CaseNotSupported(f'{backend} is not registered for {case.get("id")}')
    if case.get('id') not in supported:
        raise CaseNotSupported(f'{backend} has no captured result for {case.get("id")}')
    checkpoint_dir = (root / metadata['checkpoint_path']).resolve()
    raw_path = checkpoint_dir / f'{backend}-raw.json'
    expected_digest = metadata.get('raw_files', {}).get(raw_path.name)
    if expected_digest is None or _digest(raw_path) != expected_digest:
        raise AdapterSetupError('Raw trace changed before observation derivation')
    output = json.loads(raw_path.read_text())
    entry = next((item for item in output['results'] if item.get('id') == case['id']), None)
    if entry is None:
        raise CaseNotSupported(f'{backend} raw trace omits {case.get("id")}')
    raw = entry.get('scenarios') if 'scenarios' in entry else entry.get('raw')
    if not isinstance(raw, list):
        raise AdapterSetupError('Raw application observation is missing')
    if case['id'] == 'BOUNDARY-001':
        revision = json.loads((checkpoint_dir / 'application/build/app.meta.json').read_text())['source_revision']
        intent_id = require_safe_boundary_identity(raw, revision)
        raw = _normalize_boundary_for_legacy_deriver(raw, intent_id)
    report_path = metadata.get('checkpoint_report')
    report = json.loads((root / report_path['path']).read_text()) if report_path else None
    if report is not None and hasattr(checkpoint, 'report_contract'):
        contract = checkpoint.report_contract(report, root)
    elif metadata.get('successor') is not None and hasattr(harness, 'accepted_job001_contract'):
        contract, _ = harness.accepted_job001_contract(root)
    else:
        contract = harness.read_contract(root)
    frozen_case = next((item for item in contract['cases'] if item.get('id') == case.get('id')), None)
    if frozen_case is None or not harness.same_json(frozen_case, case):
        raise AdapterSetupError('Requested case differs from pinned acceptance contract')
    parameters = inspect.signature(checkpoint.observations).parameters
    if 'backend' in parameters:
        return checkpoint.observations(case, raw, backend=backend)
    return checkpoint.observations(case, raw, contract.get('clock'))
