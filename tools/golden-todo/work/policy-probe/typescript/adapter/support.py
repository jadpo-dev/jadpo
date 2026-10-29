"""Private verification lifecycle shared by the runner and the gate wrapper."""
from contextlib import contextmanager
import fcntl
from pathlib import Path


@contextmanager
def dependency_lock(project: Path):
    # Hold across installation AND execution: npm ci removes node_modules first.
    # An OS lock releases on exceptions and process death, with no stale PID file.
    directory = project / 'build'
    directory.mkdir(exist_ok=True)
    with (directory / 'verification.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        try:
            yield
        finally:
            fcntl.flock(lock, fcntl.LOCK_UN)
