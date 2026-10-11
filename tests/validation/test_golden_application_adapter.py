"""Identity and disclosure counterexamples for the formal app drivers."""
from copy import deepcopy
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tests' / 'validation'))
import golden_application_adapter as adapter


INTENT = 'ce4d5836-ec90-493f-868d-be2e7effe0b5'
BINDING = 'job.overdue_reminders.delivery.reminder_v1'
REVISION = 'src_561442729b04add7'


def raw_boundary():
    return [{
        'binding': BINDING,
        'sourceRevision': REVISION,
        'boundaryVariants': [{}, {
            'providerCalls': [{
                'idempotencyKey': INTENT,
                'body': {'idempotency_key': INTENT},
            }],
            'durableAfter': {
                'intents': [{'intent_id': INTENT, 'state': 'outcome_unknown'}],
                'claims': [{'intent_id': INTENT, 'invocations': 1, 'possible_dispatch': 1}],
            },
            'operationalEvents': [
                {'eventName': 'transaction.attempt'},
                {'eventName': 'service.attempt', 'operationId': INTENT,
                 'attempt': 1, 'outcome': 'misconfigured'},
                {'schemaVersion': 1, 'kind': 'operational_log_event',
                 'eventName': 'operation.failed',
                 'classification': 'RUNTIME_SERVICE_MISCONFIGURED',
                 'requestId': 'job_' + INTENT, 'traceId': None,
                 'semanticOperationId': 'job:' + BINDING,
                 'sourceRevision': REVISION, 'attributes': {}},
            ],
            'nativeHostOutcome': {'failures': [{'error': {'request_id': 'job_' + INTENT}}]},
        }],
    }]


class SafeBoundaryIdentityTests(unittest.TestCase):
    def test_current_checkpoint_originals_need_only_the_two_instrumented_files(self):
        with tempfile.TemporaryDirectory() as temporary:
            checkpoint = Path(temporary)
            target = checkpoint / 'application/build/target'
            target.mkdir(parents=True)
            (target / 'persistence.ts').write_text('instrumented persistence')
            (target / 'first-party-authentication.ts').write_text('instrumented authentication')
            (checkpoint / 'original-persistence.ts').write_text('original persistence')
            (checkpoint / 'original-authentication.ts').write_text('original authentication')
            hashes = adapter._original_generated_hashes(checkpoint)
        self.assertIn('target/persistence.ts', hashes)
        self.assertIn('target/first-party-authentication.ts', hashes)
        self.assertNotIn('target/jwt-authentication.ts', hashes)
        self.assertNotIn('target/app.ts', hashes)

    def test_accepts_checked_request_identity_bound_to_unknown_intent(self):
        raw = raw_boundary()
        self.assertEqual(adapter.require_safe_boundary_identity(raw, REVISION), INTENT)

    def test_identity_mutations_are_rejected(self):
        mutations = (
            lambda x: x[0]['boundaryVariants'][1]['operationalEvents'][1].update(operationId='other'),
            lambda x: x[0]['boundaryVariants'][1]['operationalEvents'][2].update(requestId='job_other'),
            lambda x: x[0]['boundaryVariants'][1]['operationalEvents'][2].update(semanticOperationId='job:foreign'),
            lambda x: x[0]['boundaryVariants'][1]['operationalEvents'][2].update(sourceRevision='src_foreign'),
            lambda x: x[0]['boundaryVariants'][1]['nativeHostOutcome']['failures'][0]['error'].update(request_id='other'),
            lambda x: x[0]['boundaryVariants'][1]['durableAfter']['intents'][0].update(state='sent'),
            lambda x: x[0]['boundaryVariants'][1]['durableAfter']['claims'][0].update(invocations=2),
            lambda x: x[0]['boundaryVariants'][1]['durableAfter']['claims'][0].update(possible_dispatch=0),
            lambda x: x[0]['boundaryVariants'][1]['operationalEvents'][2].update(attributes={'sql': 'SELECT 1'}),
            lambda x: x[0]['boundaryVariants'][1]['operationalEvents'][2].update(providerBody='private'),
        )
        for mutate in mutations:
            candidate = raw_boundary()
            mutate(candidate)
            with self.subTest(mutation=mutate), self.assertRaises(ValueError):
                adapter.require_safe_boundary_identity(candidate, REVISION)

    def test_legacy_projection_is_a_copy_after_new_identity_validation(self):
        original = raw_boundary()
        self.assertEqual(adapter.require_safe_boundary_identity(original, REVISION), INTENT)
        projected = adapter._normalize_boundary_for_legacy_deriver(original, INTENT)
        actual_event = original[0]['boundaryVariants'][1]['operationalEvents'][2]
        projected_event = projected[0]['boundaryVariants'][1]['operationalEvents'][2]
        self.assertEqual(actual_event['requestId'], 'job_' + INTENT)
        self.assertEqual(actual_event['semanticOperationId'], 'job:' + BINDING)
        self.assertEqual(projected_event['requestId'], INTENT)
        self.assertEqual(projected_event['semanticOperationId'], BINDING)
        self.assertIsNot(projected, original)

    def test_postgres_driver_requires_application_and_client_to_share_live_url(self):
        current = ('const environment = { DATABASE_URL: "https://db.test/golden" }; '
                   'new SQL({ url: Bun.env.DATABASE_URL! })')
        self.assertFalse(adapter.postgres_configuration_is_bound(current))
        configured = ('const environment = { DATABASE_URL: Bun.env.DATABASE_URL! }; '
                      'new SQL({ url: Bun.env.DATABASE_URL!, prepare: false })')
        self.assertTrue(adapter.postgres_configuration_is_bound(configured))
        split = ('const environment = { DATABASE_URL: "https://db.test/golden" }; '
                 'new SQL({ url: Bun.env.DATABASE_URL! })')
        self.assertFalse(adapter.postgres_configuration_is_bound(split))

    def test_removing_postgres_blocker_cannot_promote_unbound_app_configuration(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            tools = root / 'tools'
            tools.mkdir()
            (tools / 'golden_http_checkpoint.ts').write_text(
                'const environment = { DATABASE_URL: "https://db.test/golden" }; '
                'new SQL({ url: Bun.env.DATABASE_URL! })')
            evidence = {
                'registered_backends': ['sqlite'],
                'supported_ids_by_backend': {'sqlite': ['AUTH-001'], 'postgres': ['AUTH-001']},
                'registered_supported_ids_by_backend': {'sqlite': ['AUTH-001'], 'postgres': []},
                'blocked_backends': {'postgres': 'Application URL binding remains unresolved'},
            }
            self.assertTrue(adapter.validate_backend_registration(root, evidence))
            promoted = deepcopy(evidence)
            promoted['blocked_backends'].pop('postgres')
            promoted['registered_backends'].append('postgres')
            promoted['registered_supported_ids_by_backend']['postgres'] = ['AUTH-001']
            with self.assertRaisesRegex(adapter.AdapterSetupError, 'different URL'):
                adapter.validate_backend_registration(root, promoted)
            with self.assertRaisesRegex(adapter.AdapterSetupError, 'different URL'):
                adapter.derive_observations(root, promoted, {'id': 'AUTH-001'}, 'postgres')


if __name__ == '__main__':
    unittest.main()
