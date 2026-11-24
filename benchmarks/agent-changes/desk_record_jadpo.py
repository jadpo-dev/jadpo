"""The helpdesk Jadpo app through the native contract driver: record its
answers to desk_parity's steps (--steps), or run every hidden test on it
(--calibrate). usage: python3 desk_record_jadpo.py <out.json> [project] [--steps|--calibrate]"""
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'benchmarks/agent-changes'))
import harness  # noqa: E402
import desk_parity  # noqa: E402
from desk import Desk  # noqa: E402

out = Path(sys.argv[1])
args = [a for a in sys.argv[2:] if not a.startswith('--')]
project = Path(args[0]) if args else ROOT / 'benchmarks/helpdesk'
work = harness.WORK / 'desk-work'
work.mkdir(parents=True, exist_ok=True)
compiler = ROOT / 'target/debug/jadpo'
server = harness.Server(work / 'server')
driver = None
try:
    runner = harness.load('desk_contract_run', harness.CONTRACT / 'run.py', harness.CONTRACT)
    runner.PROJECT, runner.WORK, runner.CACHE = project, work / 'contract', ROOT / 'target/agent-changes'
    runner.WORK.mkdir(parents=True, exist_ok=True)
    binary = harness.own_driver(lambda: runner.build_native(ROOT), runner.WORK)
    database = server.create()
    applied = subprocess.run([str(compiler), 'migrate', 'apply', '--url', server.url(database), '--json'], cwd=project,
                             capture_output=True, text=True)
    if applied.returncode:
        raise SystemExit('migrations did not apply: ' + applied.stdout + applied.stderr)
    driver = runner.Native(binary, server.url(database), compiler)
    driver.supported = set(driver.supported) | {'logs'}
    controller = Desk(driver)
    if '--steps' in sys.argv:
        result = desk_parity.record(controller)
        out.write_text(json.dumps(result, indent=1))
        print(f'recorded {len(result)} steps to {out}')
    if '--calibrate' in sys.argv:
        import desk_changes
        controller.routes, controller.diff, controller.secrets = harness.jadpo_routes(project), '', []
        report = []
        for change in desk_changes.CHANGES:
            for test in change.tests:
                harness_controller = Desk(driver)
                harness_controller.routes, harness_controller.diff = controller.routes, ''
                try:
                    harness_controller.reset(); test(harness_controller); status = 'passed'; message = ''
                except desk_changes.Absent as error:
                    status, message = 'absent', str(error)
                except Exception as error:  # noqa: BLE001
                    status, message = 'failed', str(error)[:300]
                report.append({'change': change.id, 'test': test.__name__, 'kind': test.kind, 'status': status, 'message': message})
        out.write_text(json.dumps(report, indent=1))
        for row in report:
            print(row['change'], row['kind'][:4], row['status'].ljust(7), row['test'], row['message'][:120])
finally:
    if driver:
        driver.stop()
    server.stop()
