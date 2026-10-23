"""Negative controls for the contract verifier itself."""
import hashlib
import importlib.util
import json
from pathlib import Path
import unittest
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
if __name__=='__main__':unittest.main()
