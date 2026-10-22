"""Check the benchmark fixture with ordinary compiler/formatter paths."""
import os
import subprocess
from pathlib import Path
root = Path(__file__).resolve().parents[2]
fixture = root / 'tools/bench/fixtures/todo'
jadpo = os.environ.get('JADPO_BIN', str(root / 'target/debug/jadpo'))
for args in [('check',), ('fmt', '--check')]:
    subprocess.run([jadpo, *args], cwd=fixture, check=True)
