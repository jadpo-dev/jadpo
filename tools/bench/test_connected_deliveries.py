"""Known corrupt delivery, provider and actual-SQL evidence must fail closed."""
import copy
from pathlib import Path
import tempfile
import unittest
import connected_deliveries as c


class Controls(unittest.TestCase):
    def setUp(self):
        self.before=[{'id':'a','outbox_id':'o','subscriber':'s','status':'pending','attempts':0,
                      'next_attempt_at':'2026-10-24T00:00:00Z','last_tried_at':None,'last_failure':None,
                      'retried_at':None,'held_for':None,'held_since':None,'held_ms':None}]
        self.after=copy.deepcopy(self.before)
        self.after[0].update(status='succeeded',attempts=1,next_attempt_at=None,last_tried_at='2026-10-24T01:00:00Z')
        self.start=c.flags.instant('2026-10-24T01:00:00Z')
        self.calls=[{'method':'GET','path':'/tickets/ticket-00001','authorization':'Bearer benchmark-key'}]

    def test_exact_delivery_columns_and_time(self):
        c.validate_rows(self.before,self.after,1,self.start,self.start+1)
        for field in self.after[0]:
            wrong=copy.deepcopy(self.after);wrong[0][field]='wrong'
            with self.assertRaises((ValueError,TypeError),msg=field):c.validate_rows(self.before,wrong,1,self.start,self.start+1)
        for wrong in ([],self.after*2):
            with self.assertRaises(ValueError):c.validate_rows(self.before,wrong,1,self.start,self.start+1)

    def test_handled_marker_identities_columns_and_clock(self):
        rows=[{'id':'a','handled_at':'2026-10-24T01:00:00Z'}]
        c.validate_handled(self.before,rows,self.start,self.start+1)
        for wrong in ([],rows*2,[{**rows[0],'id':'b'}],[{**rows[0],'handled_at':'2026-10-24T00:00:00Z'}],[{**rows[0],'extra':0}]):
            with self.assertRaises(ValueError):c.validate_handled(self.before,wrong,self.start,self.start+1)

    def test_missing_duplicate_foreign_or_different_provider_calls_fail(self):
        c.validate_provider(self.calls,1)
        for calls in ([],self.calls*2,[{**self.calls[0],'path':'/tickets/ticket-00002'}],
                      [{**self.calls[0],'method':'POST'}],[{**self.calls[0],'authorization':'wrong'}]):
            with self.assertRaises(ValueError):c.validate_provider(calls,1)

    def test_multiline_log_census_excludes_other_database_and_detail(self):
        text='''BENCH|workers||1| LOG:  execute <unnamed>: WITH "claimed" AS (UPDATE "jadpo_deliveries" SET "attempts" = "attempts" + 1)
\tSELECT * FROM "claimed"
BENCH|workers||1| DETAIL:  parameters: $1 = 'x'
BENCH|postgres|psql|2| LOG:  statement: SELECT administrative_control
BENCH|workers||1| LOG:  statement: BEGIN ISOLATION LEVEL READ COMMITTED
BENCH|workers||1| LOG:  statement: COMMIT
'''
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'pg.log';path.write_text('before\n'+text)
            raw,sql=c.parse_log(path,len('before\n'),'workers')
        self.assertEqual(raw,text)
        self.assertEqual(len(sql),3)
        self.assertIn('\nSELECT',sql[0])
        good={'sql':sql,'postgres_log':raw,'data_statements':1,'transaction_statements':2,
              'independent_breaker_reads':0,'joined_claims':1}
        c.validate_census(good,True)
        with self.assertRaises(ValueError):c.validate_census(good,False)
        for field,value in [('sql',[]),('postgres_log',''),('data_statements',2),('transaction_statements',1),
                            ('independent_breaker_reads',1),('joined_claims',0)]:
            with self.assertRaises(ValueError,msg=field):c.validate_census({**good,field:value},True)
        duplicate={**good,'sql':sql+[sql[0]],'data_statements':2}
        with self.assertRaises(ValueError):c.validate_census(duplicate,True)


if __name__=='__main__':unittest.main()
