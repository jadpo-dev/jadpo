#!/usr/bin/env python3
"""A deployable holds no testing machinery (docs/compiler-runtime.md §2.6,
"The `testing` feature"; roadmap chunk 8.1e).

Builds jadpo-host-native's `deployable` example, a deployable's shape on the
native host, on its own, so nothing turns on the runtime's `testing` feature,
then reads the binary's symbols: none may name the fakes, the recordings,
the crash points, the outcome-and-effect record, the delivery between a
test's steps or the test authenticator, nor OpenSSL or `webauthn-rs` (area 8,
"Passkeys"), while the runtime's HTTP component, its passkey ceremonies and
the driver factory must be there, so the check reads a real deployable.

  python3 tools/checks/deployable.py
"""

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

# How a path in the crate shows in a symbol, legacy and v0 mangling alike:
# each name its length, then its text.
def path(*names: str) -> bytes:
    return ''.join(f'{len(name)}{name}' for name in names).encode()


ABSENT = {
    'the fakes': path('jadpo_runtime', 'fakes'),
    'the `ai` fake': path('ai', 'fake'),
    'the host with fakes in front': path('capability', 'Faked'),
    'the recordings': path('jadpo_runtime', 'recorded'),
    'the prompt recordings': path('jadpo_runtime', 'prompt_recordings'),
    'the crash points': path('jadpo_runtime', 'crashes'),
    'the outcome-and-effect record': path('jadpo_runtime', 'record'),
    'the delivery between a test\'s steps': path('jadpo_runtime', 'test_steps'),
    'the test authenticator': path('jadpo_runtime', 'test_authenticator'),
    # No native deployable links OpenSSL (area 8, "Passkeys"): the passkey
    # ceremonies are the runtime's own, in pure Rust.
    'OpenSSL\'s bindings': path('openssl'),
    'the WebAuthn library the passkey tests are held to': path('webauthn_rs'),
}

PRESENT = {
    'the HTTP component': path('jadpo_runtime', 'http'),
    'the driver factory': path('jadpo_drivers', 'factory'),
    'the passkey ceremonies': path('jadpo_runtime', 'webauthn'),
}


def main() -> int:
    build = subprocess.run(
        ['cargo', 'build', '--locked', '-p', 'jadpo-host-native', '--example', 'deployable',
         '--message-format', 'json-render-diagnostics'],
        cwd=ROOT, stdout=subprocess.PIPE, text=True)
    if build.returncode != 0:
        print('the deployable example doesn\'t build without the `testing` feature')
        return 1
    binary = None
    for line in build.stdout.splitlines():
        message = json.loads(line)
        if message.get('reason') == 'compiler-artifact' and message['target']['name'] == 'deployable':
            binary = message.get('executable')
    if not binary:
        print('the build named no deployable binary')
        return 1
    held = Path(binary).read_bytes()
    problems = [f'it holds {what} ({name.decode()})' for what, name in ABSENT.items() if name in held]
    problems += [f'it lacks {what} ({name.decode()}), so it isn\'t a deployable'
                 for what, name in PRESENT.items() if name not in held]
    for problem in problems:
        print(problem)
    if not problems:
        print(f'a deployable built without `testing` holds none of it: {len(held):,} bytes')
    return 1 if problems else 0


if __name__ == '__main__':
    sys.exit(main())
