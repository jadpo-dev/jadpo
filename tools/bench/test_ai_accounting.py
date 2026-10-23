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


class TrackedModes(unittest.TestCase):
    def test_directory_and_file_symlinks_hash_the_literal_target(self):
        import hashlib
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            (root/'folder').mkdir()
            (root/'file').write_bytes(b'content')
            for target in ('folder','file'):
                link=root/('link-'+target)
                link.symlink_to(target,target_is_directory=target=='folder')
                entry=a.tracked_entry(link,'120000')
                self.assertEqual(entry,{'mode':'120000','sha256':hashlib.sha256(target.encode()).hexdigest()})

    def test_retargeting_same_content_requires_rebuild(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            for name in ('a','b'):(root/name).write_bytes(b'same')
            link=root/'link';link.symlink_to('a')
            before={'link':a.tracked_entry(link,'120000')}
            stamp={'source':before,'artifacts':{'unused':'unused'}}
            link.unlink();link.symlink_to('b')
            with self.assertRaisesRegex(ValueError,'rebuild'):
                a.validate(stamp,{'link':a.tracked_entry(link,'120000')})

    def test_git_materialized_symlink_and_executable_modes(self):
        import hashlib
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'materialized'
            path.write_bytes(b'../directory-target')
            self.assertEqual(a.tracked_entry(path,'120000')['sha256'],hashlib.sha256(path.read_bytes()).hexdigest())
            path.chmod(0o644);a.tracked_entry(path,'100644')
            path.chmod(0o755)
            with self.assertRaisesRegex(ValueError,'mode changed'):a.tracked_entry(path,'100644')
            a.tracked_entry(path,'100755')


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
