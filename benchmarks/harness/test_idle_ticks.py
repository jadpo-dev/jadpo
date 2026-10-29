import json
import unittest
import idle_ticks as idle

class IdleAuditTests(unittest.TestCase):
    def test_missing_failed_and_duplicate_tick_logs_are_rejected(self):
        good={'event':'runtime.ticked','problems':[],'deliveries':0,'deleted':0,'ran':[]}
        idle.audit_tick(json.dumps(good))
        for text in ('',json.dumps({'event':'operation.failed'}),
                     json.dumps({**good,'problems':[{'problem':'store_failed'}]}),
                     json.dumps(good)+'\n'+json.dumps(good)):
            with self.assertRaises(ValueError): idle.audit_tick(text)

    def test_census_requires_real_prompt_statements_not_startup_only(self):
        with self.assertRaises(ValueError):
            idle.census_sql('LOG:  statement: SELECT * FROM jadpo_migrations')
        all_sql,tick_sql=idle.census_sql('LOG:  execute q1: SELECT * FROM "jadpo_deliveries"\n'
            'LOG:  statement: UPDATE "jadpo_circuit_breakers" SET alerted_at=NULL\n'
            'LOG:  statement: SELECT * FROM jadpo_migrations')
        self.assertEqual(len(all_sql),3)
        self.assertEqual(len(tick_sql),2)

if __name__=='__main__': unittest.main()
