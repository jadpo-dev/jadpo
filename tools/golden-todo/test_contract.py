"""Negative controls for the contract verifier itself."""
import hashlib
import importlib.util
import json
from pathlib import Path
import unittest
import os
import subprocess
import tempfile
from unittest import mock
import threading
from concurrent.futures import ThreadPoolExecutor
import worker
import run
import contract

ROOT=Path(__file__).resolve().parents[2]
class ContractVerifier(unittest.TestCase):
    def test_candidate_bytes_and_all_case_ids_are_preserved(self):
        original=(ROOT/'examples/golden-todo/acceptance.json').read_bytes()
        self.assertEqual(hashlib.sha256(original).hexdigest(), "3bed090059de86e35d72fb7a61b0fab365990d909e118b4f5f39b7b10ba56608")
        successor=json.loads((ROOT/'examples/golden-todo/acceptance-v0.2.json').read_text())
        contract.validate_registry(successor)
        self.assertEqual({case['id'] for case in json.loads(original)['cases']},set(contract.CASES))
        successor['cases'].pop()
        with self.assertRaises(AssertionError):contract.validate_registry(successor)
    def test_sql_is_classified_once_and_business_user_reads_are_not_authentication(self):
        statements=[
            'SELECT u.id FROM "jadpo_sessions" s JOIN "user" u ON u.id=s.principal_id',
            'SELECT id FROM "jadpo_sessions" WHERE verifier=$1',
            'SELECT "authentication_subject", "email" FROM "user" WHERE "id"=$1',
            'SELECT "id" FROM "user" WHERE "authentication_subject"=$1',
            'UPDATE "todo" SET title=$1 WHERE id=$2',
            'SELECT 1', 'COMMIT',
        ]
        self.assertEqual(contract.counts(statements),dict(principal=2,credential=1,business=2,runtime=1,transaction=1))
    def test_provider_concurrently_deduplicates_and_rejects_changed_payload(self):
        class Helper:
            def call(self,command):return {'result':{'jwks':{'keys':[]}}}
        fixture=worker.Fixture(Helper())
        try:
            barrier=threading.Barrier(8)
            def accept(index):
                barrier.wait();return fixture.accept('same-key',{'title':'Same'})
            with ThreadPoolExecutor(max_workers=8) as pool:answers=list(pool.map(accept,range(8)))
            self.assertEqual(len(fixture.receipts),1)
            self.assertEqual(len({answer[1]['message_id'] for answer in answers}),1)
            self.assertEqual(fixture.accept('same-key',{'title':'Changed'})[0],'conflict')
            self.assertEqual(len(fixture.receipts),1)
        finally:fixture.stop()
    def test_completion_refuses_partial_failed_stale_and_external_runs(self):
        reports=[{'target':target,'id':identifier,'status':'passed'} for target in ['native','workerd'] for identifier in contract.CASES]
        complete=lambda rows=reports,targets=['native','workerd'],filtered=False,stale=False,own=True:run.complete_contract(targets,rows,filtered,stale,own)
        self.assertTrue(complete())
        self.assertFalse(complete(reports[:-1]))
        self.assertFalse(complete(reports[:-1]+[reports[0]]))
        self.assertFalse(complete(targets=['native']))
        self.assertFalse(complete(targets=['native','workerd','native']))
        self.assertFalse(complete(filtered=True))
        self.assertFalse(complete(stale=True))
        self.assertFalse(complete(own=False))
        reports[0]={**reports[0],'status':'failed'}
        self.assertFalse(complete())
    def test_workerd_routes_the_same_socket_server_through_its_external_binding(self):
        self.assertEqual(worker.postgres_address("host='/tmp/socket space' port=5544 user=jadpo dbname=postgres"), 'unix:/tmp/socket space/.s.PGSQL.5544')
        self.assertEqual(worker.postgres_address('postgresql://u:p@localhost:5544/db?host=%2Ftmp%2Fsock'), 'unix:/tmp/sock/.s.PGSQL.5544')
        self.assertEqual(worker.postgres_address('postgresql://u:p@[::1]:5544/db'), '[::1]:5544')
    def test_missing_obligation_is_a_failure(self):
        class Missing:
            def call(self,command):raise NotImplementedError('missing target obligation')
        with self.assertRaises(NotImplementedError):contract.Contract(Missing()).call('tick')

class WorkerOwnership(unittest.TestCase):
    def platform(self, root, version, name='workerd-test', executable='workerd'):
        package=root/'tools/workers/node_modules/@cloudflare'/name
        (package/'bin').mkdir(parents=True,exist_ok=True)
        (package/'package.json').write_text(json.dumps({'version':version}))
        binary=package/'bin'/executable
        binary.write_text('#!/bin/sh\nexec sleep 30\n')
        binary.chmod(0o755)
        return binary

    def test_only_the_pinned_native_platform_binary_is_selected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            tools=root/'tools/workers'
            tools.mkdir(parents=True)
            (tools/'package.json').write_text(json.dumps({'dependencies':{'workerd':'pinned'}}))
            self.platform(root,'older','workerd-old')
            self.assertIsNone(worker.installed_workerd(root))
            binary=self.platform(root,'pinned',executable='workerd.exe')
            self.assertEqual(worker.installed_workerd(root),binary)
            self.assertNotIn('.bin',binary.parts)

    @unittest.skipUnless(os.name=='posix','the lifecycle fixture executes a POSIX sleep')
    def test_failed_constructor_stops_and_reaps_the_actual_native_child(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            (root/'tools/workers').mkdir(parents=True)
            (root/'tools/workers/package.json').write_text(json.dumps({'dependencies':{'workerd':'pinned'}}))
            binary=self.platform(root,'pinned')
            children=[]
            def fail_start(instance):
                instance.log=tempfile.TemporaryFile(mode='w+')
                instance.process=subprocess.Popen([str(instance.executable)],stdout=instance.log,stderr=instance.log)
                children.append(instance.process)
                raise RuntimeError('exercise startup failure cleanup')
            with mock.patch.dict(os.environ,{},clear=True),mock.patch.object(worker,'Fixture') as fixture,mock.patch.object(worker.Worker,'start',fail_start):
                with self.assertRaisesRegex(RuntimeError,'startup failure'):
                    worker.Worker(mock.Mock(environment={}),root,root,root,root,'local-test')
                fixture.return_value.stop.assert_called_once()
            self.assertEqual(len(children),1)
            self.assertIsNotNone(children[0].poll())
            with self.assertRaises(ProcessLookupError):os.kill(children[0].pid,0)

if __name__=='__main__':unittest.main()
