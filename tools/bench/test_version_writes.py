"""PATCH receipt/state and runtime-census controls, independent of timing."""
import copy
import unittest
import version_writes as v


class PatchControls(unittest.TestCase):
    def setUp(self):
        self.before=[{'id':'a','owner_id':'alice','title':'old','status':'open','due_at':None,
                      'created_at':'2026-01-15T12:00:00Z','updated_at':'2026-01-15T12:00:00Z',
                      'reminder_revision':1,'deleted_at':None},
                     {'id':'foreign','owner_id':'bob','title':'other'}]
        self.after=copy.deepcopy(self.before)
        self.after[0].update(title='bench-patch',updated_at='2026-10-23T12:00:00Z')
        self.receipts=[{k:self.after[0][k] for k in ('id','owner_id','title','status','due_at','created_at','updated_at')}]
        self.cases=[{'owner':'alice','expected_json':{'id':'a'}}]
        self.start=v.instant('2026-10-23T11:59:59Z');self.end=self.start+2

    def verify(self,before=None,after=None,receipts=None,successes=1):
        v.validate_patch(before or self.before,after or self.after,self.receipts if receipts is None else receipts,
                         self.cases,successes,self.start,self.end)

    def test_correct_update_and_timezone_equivalence(self):
        self.verify()
        receipts=copy.deepcopy(self.receipts);receipts[0]['created_at']='2026-01-15T13:00:00+01:00'
        self.verify(receipts=receipts)

    def test_each_bad_response_field_or_timestamp_fails(self):
        for field in self.receipts[0]:
            receipts=copy.deepcopy(self.receipts);receipts[0][field]='bad'
            with self.assertRaises((ValueError,TypeError),msg=field):self.verify(receipts=receipts)
        for timestamp in ('2026-10-24T12:00:00Z','2026-10-23T12:00:00'):
            receipts=copy.deepcopy(self.receipts);receipts[0]['updated_at']=timestamp
            with self.assertRaises(ValueError):self.verify(receipts=receipts)

    def test_missing_duplicate_or_unexpected_receipts_fail(self):
        for receipts,successes in (([],1),(self.receipts*2,1),(self.receipts,2)):
            with self.assertRaises(ValueError):self.verify(receipts=receipts,successes=successes)

    def test_missing_or_shared_partition_rows_fail(self):
        self.cases*=2
        with self.assertRaises(ValueError):self.verify()
        self.cases=[self.cases[0],{'owner':'bob','expected_json':{'id':'foreign'}}]
        with self.assertRaises(ValueError):self.verify()

    def test_all_unallowed_stored_changes_fail(self):
        for index,field in ((0,'owner_id'),(0,'status'),(0,'reminder_revision'),(0,'deleted_at'),(0,'created_at'),(1,'title')):
            after=copy.deepcopy(self.after);after[index][field]='bad'
            with self.assertRaises(ValueError,msg=field):self.verify(after=after)
        with self.assertRaises(ValueError):self.verify(after=self.after+[self.after[0]])


class RuntimeControls(unittest.TestCase):
    def result(self):
        row={'count':2,'seconds':1.,'calls_per_second':2.,'p95_ms':2.,'latency_ms':[1.,2.],
             'provider_calls':2,'data_statements_per_call':1,'statements_per_call':3,
             'census_sql':[{'sql':'BEGIN ISOLATION LEVEL repeatable read'},{'sql':'UPDATE table'},{'sql':'COMMIT'}],
             'postgres_statements':4,'provider_control_queries':1,'postgres_log':'actual log',
             'stale_control':True,'refusal_control':True,'fault_control':True,'persisted_note':'Fresh','persisted_label':'Fresh b'}
        return {'protocol':1,'complete':True,'warmup':50,'count':2,'runs':[{**copy.deepcopy(row),'round':r} for r in (1,2,3)]}

    def test_complete_and_each_corruption(self):
        result=self.result();v.validate_runtime(result)
        for field,value in [('complete',False),('protocol',0),('warmup',0),('runs',result['runs'][:2])]:
            wrong=copy.deepcopy(result);wrong[field]=value
            with self.assertRaises(ValueError,msg=field):v.validate_runtime(wrong)
        for field,value in [('count',1),('provider_calls',1),('p95_ms',1.),('calls_per_second',1.),
                            ('latency_ms',[1.]),('statements_per_call',2),('data_statements_per_call',2),
                            ('postgres_statements',3),('stale_control',False),('refusal_control',False),
                            ('fault_control',False),('postgres_log',''),('persisted_note','bad'),('persisted_label','bad')]:
            wrong=copy.deepcopy(result);wrong['runs'][0][field]=value
            with self.assertRaises(ValueError,msg=field):v.validate_runtime(wrong)


if __name__=='__main__':unittest.main()
