"""Check the benchmark fixture with ordinary compiler/formatter paths."""
import os
import json
import shutil
import subprocess
import tempfile
from pathlib import Path
from golden import PROJECT, WORK, application_for
root = Path(__file__).resolve().parents[2]
fixture = root / 'tools/bench/fixtures/todo'
jadpo = os.environ.get('JADPO_BIN', str(root / 'target/debug/jadpo'))
for args in [('check',), ('fmt', '--check')]:
    subprocess.run([jadpo, *args], cwd=fixture, check=True)

# Check the exact production overlays before paying for release compilation.
# The negative variants keep the compiler's target and trigger safeguards live.
WORK.mkdir(parents=True, exist_ok=True)
with tempfile.TemporaryDirectory(prefix='configuration-', dir=WORK) as temporary:
    project = Path(temporary)
    shutil.copytree(PROJECT, project, dirs_exist_ok=True,
                    ignore=shutil.ignore_patterns('typescript', 'build', '.jadpo'))
    def check(application, rule=None):
        (project / 'app.jadpo').write_text(application)
        answer = subprocess.run([jadpo, 'check', '--json'], cwd=project, capture_output=True, text=True)
        if rule is None:
            if answer.returncode:
                raise AssertionError(answer.stdout + answer.stderr)
        elif answer.returncode == 0 or rule not in {item['rule'] for item in json.loads(answer.stdout)['diagnostics']}:
            raise AssertionError('Invalid benchmark configuration escaped ' + rule)
    check(application_for('native'))
    workers = application_for('workers')
    check(workers)
    check(workers.replace('runs: [Trigger.subscribers, Trigger.schedules]', 'runs: []'), 'deployable.unserved')
    check(workers.replace('ClientAddressSource.cloudflare', 'ClientAddressSource.forwarded_for hops: 1'),
          'deployable.target_unsupported')
print('passed: benchmark production configurations and negative controls')
