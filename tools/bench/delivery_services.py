"""Full-gate real-service controls; owns one pinned install/build before testing."""
import os
import json
import time
import tempfile
import unittest
import subprocess
import importlib.util
from pathlib import Path
import deliveries
import run as spike


specification = importlib.util.spec_from_file_location('delivery_postgres', deliveries.ROOT / 'tools/golden-todo/postgres.py')
normalizer = importlib.util.module_from_spec(specification)
specification.loader.exec_module(normalizer)


class WarmServices(unittest.TestCase):
    """Real HTTP children and PostgreSQL: no controller or virtual-clock adapter."""
    @classmethod
    def setUpClass(cls):
        cls.folder = tempfile.TemporaryDirectory(prefix='delivery-services-')
        cls.previous = spike.WORK
        spike.WORK = Path(cls.folder.name)
        cls.borrowed = os.environ.get('JADPO_TEST_DATABASE_URL')
        cls.pg = None
        cls.created = []
        cls.addClassCleanup(cls.cleanup)
        if cls.borrowed:
            cls.pg = deliveries.Database.__new__(deliveries.Database)
            base = normalizer.postgres_uri(cls.borrowed)
            prefix = 'delivery_services_' + str(os.getpid()) + '_'
            cls.pg.url = lambda name: normalizer.postgres_uri(base, normalizer.connection_parameters(base).get('dbname', 'postgres') if name == 'postgres' else prefix + name)
        else:
            cls.pg = deliveries.Database.__new__(deliveries.Database)
            cls.pg.__init__()
        for target in ('bun', 'node'):
            cls.pg.sql('postgres', f'CREATE DATABASE {normalizer.connection_parameters(cls.pg.url(target))["dbname"]}')
            cls.created.append(target)
            cls.pg.sql(target, next((deliveries.FIXTURE / 'database/migrations').glob('*/postgres.sql')).read_text())

    @classmethod
    def cleanup(cls):
        try:
            if cls.borrowed:
                for target in cls.created:
                    cls.pg.sql('postgres', f'DROP DATABASE {normalizer.connection_parameters(cls.pg.url(target))["dbname"]} WITH (FORCE)')
            elif cls.pg is not None and hasattr(cls.pg, 'data'):
                cls.pg.stop()
        finally:
            spike.WORK = cls.previous
            cls.folder.cleanup()

    def test_keyword_shared_connection_preserves_endpoint_and_startup_parameters(self):
        # Exercise this adapter, not only the normalizer: the local gate supplies
        # a socket keyword DSN, while CI supplies a TCP URI. No server substitute.
        base = self.pg.url('postgres')
        parameters = normalizer.connection_parameters(base)
        parameters['application_name'] = "delivery service ' + marker"
        keyword = ' '.join(key + "='" + value.replace('\\', '\\\\').replace("'", "\\'") + "'"
                           for key, value in parameters.items())
        probe = deliveries.Database.__new__(deliveries.Database)
        probe.url = lambda name: normalizer.postgres_uri(keyword, normalizer.connection_parameters(self.pg.url(name))['dbname'])
        self.assertEqual(normalizer.connection_parameters(probe.url('bun'))['host'], parameters['host'])
        for target in ('bun', 'node'):
            self.pg.seed(target, {'event': 'queued', 'subscriber': 'record_ticket', 'encoding': 1}, 100)
            owned = []
            try:
                process, port = deliveries.typescript_service(target, Path(self.folder.name), probe,
                    Path(self.folder.name) / f'{target}-keyword.log', owned)
                deliveries.validate_backlog(probe.state(target), 100)
                # Pool sessions preserve quoted startup settings on that server.
                self.assertGreater(int(probe.query(target,
                    "SELECT count(*) FROM pg_stat_activity WHERE pid<>pg_backend_pid() AND application_name='delivery service '' + marker'")), 0)
                self.assertIsNone(process.poll())
            finally:
                for child in reversed(owned):
                    spike.stop(child)

    def test_service_lifetime_fresh_ticks_durability_and_failure_cleanup(self):
        template = {'event': 'queued', 'subscriber': 'record_ticket', 'encoding': 1}
        for target in ('bun', 'node'):
            with self.subTest(target=target):
                owned = []
                self.pg.seed(target, template, 100)
                self.pg.sql(target, "UPDATE jadpo_deliveries SET next_attempt_at=clock_timestamp()+interval '1 hour'")
                try:
                    process, port = deliveries.typescript_service(target, Path(self.folder.name), self.pg,
                        Path(self.folder.name) / f'{target}.log', owned)
                    first = deliveries.tick(target, None, self.pg, port)
                    self.assertEqual(json.loads(first['body'])['pid'], process.pid)
                    self.assertEqual(json.loads(first['body'])['selected'], 0)
                    # These become due after the service has started: a frozen clock fails.
                    self.pg.sql(target, "UPDATE jadpo_deliveries SET next_attempt_at=clock_timestamp()")
                    time.sleep(.02)
                    second = deliveries.tick(target, None, self.pg, port)
                    self.assertEqual(json.loads(second['body'])['pid'], process.pid)
                    self.assertEqual(json.loads(second['body'])['completed'], 100)
                    deliveries.validate_backlog(self.pg.state(target), 100)
                    tried = self.pg.query(target, 'SELECT max(last_tried_at)::text FROM jadpo_deliveries')
                    time.sleep(.02)
                    self.pg.seed(target, template, 100)
                    third = deliveries.tick(target, None, self.pg, port)
                    self.assertEqual(json.loads(third['body'])['completed'], 100)
                    self.assertGreater(self.pg.query(target, 'SELECT max(last_tried_at)::text FROM jadpo_deliveries'), tried)
                    reports = []
                    deliveries.controls(target, None, self.pg, template, port, reports)
                    self.assertEqual(len([r for r in reports if r['ok']]), 5)
                    self.pg.sql(target, "UPDATE jadpo_deliveries SET status='pending',next_attempt_at=clock_timestamp()")
                    deliveries.tick(target, None, self.pg, port)
                    replay = self.pg.state(target)
                    self.assertEqual(replay['receipts'], 100)
                    self.assertEqual(replay['attempts'], 200)
                    self.pg.sql(target, 'ALTER TABLE jadpo_deliveries RENAME TO hidden_deliveries')
                    try:
                        with self.assertRaises(deliveries.TickError):
                            deliveries.tick(target, None, self.pg, port)
                    finally:
                        self.pg.sql(target, 'ALTER TABLE hidden_deliveries RENAME TO jadpo_deliveries')
                    self.assertEqual(json.loads(deliveries.tick(target, None, self.pg, port)['body'])['pid'], process.pid)
                finally:
                    for child in reversed(owned):
                        spike.stop(child)
                self.assertIsNotNone(process.returncode)
                self.assertEqual(self.pg.query(target, "SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid()"), '0')


if __name__ == '__main__':
    subprocess.run(['npm', 'ci', '--ignore-scripts', '--no-audit', '--no-fund'], cwd=deliveries.HERE / 'typescript', check=True)
    subprocess.run(['npm', 'run', 'build'], cwd=deliveries.HERE / 'typescript', check=True)
    unittest.main()
