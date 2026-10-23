"""Freshness controls must fail on source identity and artifact drift."""
import tempfile
from pathlib import Path
import unittest

import ai_accounting as a


class Guards(unittest.TestCase):
    def test_source_head_tree_or_mode_changes_require_rebuild(self):
        with tempfile.TemporaryDirectory() as directory:
            artifact = Path(directory)/'fixture'
            artifact.write_bytes(b'compiled')
            source={'head':'h','tree':'t','files':{'a':{'mode':'100644','sha256':'d'}}}
            stamp={'source':source,'artifacts':{str(artifact):a.sha(artifact)}}
            a.validate(stamp,source)
            for changed in [{'head':'new','tree':'t','files':source['files']},
                            {'head':'h','tree':'new','files':source['files']},
                            {'head':'h','tree':'t','files':{'a':{'mode':'100755','sha256':'d'}}}]:
                with self.assertRaisesRegex(ValueError,'rebuild'):a.validate(stamp,changed)
            artifact.write_bytes(b'wrong')
            with self.assertRaisesRegex(ValueError,'artifact changed'):a.validate(stamp,source)


class Results(unittest.TestCase):
    @staticmethod
    def sample():
        runs=[]
        for round in range(1,4):
            for scenario in ('app','caller','retry-caller'):
                def state(n):
                    return [dict(scope=scope,key={'app':'','caller':'a'*32,'prompt':'answer local/model'}[scope],
                                 calls=n,input_tokens=n*(13 if scenario=='retry-caller' else 6),output_tokens=n,
                                 cached_tokens=2*n,reserved_tokens=0,cost_micros=0,reserved_micros=0,currency=None)
                            for scope in (['app','prompt'] if scenario=='app' else ['app','caller','prompt'])]
                runs.append(dict(round=round,scenario=scenario,latency_ms=[1.],p50_ms=1.,p95_ms=1.,seconds=.001,calls_per_second=1000.,
                                 census_sql=[dict(sql='statement',params='[]')],statements_per_call=1,postgres_statements=1,postgres_log='LOG',
                                 fault_control=True,refusal_control=True,census_before=state(50),census_after=state(51),before=state(51),after=state(52)))
        return dict(protocol=2,complete=True,count=1,warmup=50,runs=runs)

    def test_complete_result_passes(self):
        a.validate_results(self.sample())

    def test_incomplete_faulted_or_corrupt_result_is_rejected(self):
        import copy
        good=self.sample()
        mutations=[lambda r:r.update(complete=False),lambda r:r['runs'].pop(),
                   lambda r:r['runs'][0].update(p95_ms=999),lambda r:r['runs'][0].update(postgres_statements=2),
                   lambda r:r['runs'][0].update(fault_control=False),lambda r:r['runs'][0]['after'][0].update(input_tokens=0)]
        for mutation in mutations:
            result=copy.deepcopy(good);mutation(result)
            with self.assertRaises(ValueError):a.validate_results(result)
