"""Install the local pinned baseline without scripts, then check strict types."""
import subprocess
from pathlib import Path
folder = Path(__file__).resolve().parent / 'typescript'
subprocess.run(['npm', 'ci', '--ignore-scripts', '--no-audit', '--no-fund'], cwd=folder, check=True)
subprocess.run(['npm', 'run', 'check'], cwd=folder, check=True)
