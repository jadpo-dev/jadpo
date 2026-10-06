#!/usr/bin/env python3
"""Read-only current local runtime/JWT provenance; never qualifies whole RM504."""
from __future__ import annotations
import argparse
import json
from pathlib import Path
import re
import shutil
import subprocess
import golden_cases as harness
import golden_checkpoint as checkpoint
ROOT = Path(__file__).resolve().parents[1]
HELPERS = ('tools/golden_runtime_provenance.py', 'tools/golden_runtime_provenance.ts')


def native_probe(bun: Path, application: Path):
    output = checkpoint.safe_run([str(bun), '--no-install', '--env-file=/dev/null', str(ROOT/HELPERS[1]), str(application)], cwd=ROOT)
    value = json.loads(output.stdout)
    if Path(value['executable']).resolve() != bun.resolve(): raise ValueError('Native executable identity differs')
    return value


def collect(evidence: Path, dependencies: Path, bun: Path):
    report = json.loads((evidence/'golden-checkpoint.json').read_text())
    checkpoint.validate_checkpoint(report, evidence, dependencies)
    binary_sha = harness.digest(bun)
    helper_sha = {name: harness.digest(ROOT/name) for name in HELPERS}
    target = evidence/'application/build/target'
    canonical = ROOT/'jadpo/crates/core/src/runtime/jwt'
    declarations = {}
    for name in ('package.json', 'bun.lock'):
        hashes = {kind: harness.digest(directory/name) for kind, directory in (('canonical', canonical), ('generated', target), ('installed_cache', dependencies))}
        if len(set(hashes.values())) != 1: raise ValueError('JWT declarations differ between source/emission/cache')
        declarations[name] = hashes
    package = json.loads((dependencies/'node_modules/jose/package.json').read_text())
    pin = json.loads((canonical/'dependency.json').read_text())
    if package['version'] != pin['version'] or package.get('dependencies', {}) or package.get('optionalDependencies', {}) or package.get('peerDependencies', {}): raise ValueError('Actual JWT package differs from reviewed dependency shape')
    native = native_probe(bun, target/'app.ts')
    generated = native['generatedTemporalProvenance']
    zone_file = ROOT/'jadpo/data/iana-zones-2026c.txt'
    zones = zone_file.read_text().splitlines()
    if generated['tzdb'] != 'IANA '+re.search(r'(\d{4}[a-z]+)', zone_file.name)[1] or generated['zoneEnumCount'] != len(set(zones)) or generated['runtimeIcu'] != native['reportedIcu'] or generated['runtimeEngine'] != native['engine'] or generated['timezoneRules'] != 'runtime ICU; version recorded for release pinning': raise ValueError('Actual emitted temporal provenance differs from compiler/runtime inputs')
    linked = checkpoint.safe_run(['/usr/bin/otool', '-L', str(bun)]).stdout.decode()
    if native['nativeIcu']['libraryPath'] not in linked: raise ValueError('Native ICU probe library is not a declared executable dependency')
    os_version = {name: checkpoint.safe_run(['/usr/bin/sw_vers', flag]).stdout.decode().strip() for name, flag in (('product', '-productVersion'), ('build', '-buildVersion'))}
    checkpoint.validate_checkpoint(report, evidence, dependencies)
    if harness.digest(bun) != binary_sha or helper_sha != {name: harness.digest(ROOT/name) for name in HELPERS}: raise ValueError('Executable/probe changed during observation')
    return {'schema_version': 1, 'status': 'local_provenance_observed', 'source_report_sha256': harness.digest(evidence/'golden-checkpoint.json'), 'compiler_sha256': report['provenance']['compiler_sha256'], 'helper_sha256': helper_sha, 'executable_sha256': binary_sha, 'native': native, 'macos': os_version, 'declared_native_dependencies': linked.splitlines()[1:], 'standalone_native_library_file_present': Path(native['nativeIcu']['libraryPath']).is_file(), 'standalone_native_library_sha256': harness.digest(Path(native['nativeIcu']['libraryPath'])) if Path(native['nativeIcu']['libraryPath']).is_file() else None, 'zone_enumeration': {'source': str(zone_file.relative_to(ROOT)), 'sha256': harness.digest(zone_file), 'count': len(set(zones)), 'classification': 'compiler closed zone identifiers; not native rule bytes'}, 'jwt': {'declarations': declarations, 'installed_package_version': package['version'], 'installed_files': checkpoint.hashes(dependencies/'node_modules/jose'), 'dependency_manifest_sha256': harness.digest(canonical/'dependency.json'), 'historical_publisher_verification': pin['provenance_verification'], 'publisher_verification_rerun': False}, 'limits': ['Current native read-only observation; does not retroactively attest executable bytes during earlier SQLite/PostgreSQL requests.', 'Engine-reported ICU and directly observed loaded-library release are separate values, never relabeled.', 'Timezone version is observed directly through ucal_getTZDataVersion; equality with compiler zone-list release does not prove identical full rule bytes.', 'macOS native dependency resides in shared cache; no standalone dylib byte digest or cross-host reproducible ICU data pin is claimed.', 'No new PostgreSQL temporal semantics or complete TIME/RM504 qualification, protected release or engine/dependency update.']}


def validate(receipt: dict, evidence: Path, dependencies: Path, bun: Path):
    # Native observations are remeasured rather than accepting a caller label.
    if not harness.same_json(receipt, collect(evidence, dependencies, bun)):
        raise ValueError('Local runtime/dependency provenance differs from fresh native observation')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--checkpoint', type=Path, required=True)
    parser.add_argument('--jwt-dependencies', type=Path, required=True)
    parser.add_argument('--bun', type=Path, default=Path(shutil.which('bun') or 'bun'))
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    receipt = collect(args.checkpoint.resolve(), args.jwt_dependencies.resolve(), args.bun.resolve())
    validate(receipt, args.checkpoint.resolve(), args.jwt_dependencies.resolve(), args.bun.resolve())
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(receipt, indent=2, allow_nan=False)+'\n')
    print('Local executable/native ICU/JWT provenance observed; whole RM504 and rule-byte pinning remain separate')
    return 0


if __name__ == '__main__': raise SystemExit(main())
