#!/usr/bin/env python3
"""Starts the verifier's image and uses it over HTTP, as the registry would:
CI's check that the image's sandbox works and its requests are guarded,
before it is ever published. It reaches GitHub, so it also shows a commit
really is fetched over HTTPS at a pinned address."""

import json
import os
import subprocess
import sys
import time
import urllib.error
import urllib.request

PORT = 18080
BASE = f'http://127.0.0.1:{PORT}'
NAME = 'verifier-smoke'
TOKEN = os.environ['JADPO_VERIFIER_TOKEN']
# A public repository that isn't a package: its commit is fetched and listed,
# then refused for what it holds, and none of its code is ever run.
REPOSITORY = 'https://github.com/jadpo-dev/jadpo.git'


def call(method, path, body=None, token=TOKEN):
    request = urllib.request.Request(
        BASE + path,
        method=method,
        data=None if body is None else json.dumps(body).encode(),
    )
    if token:
        request.add_header('Authorization', f'Bearer {token}')
    try:
        with urllib.request.urlopen(request, timeout=120) as answer:
            return answer.status, json.load(answer)
    except urllib.error.HTTPError as error:
        return error.code, json.load(error)


def main(image):
    subprocess.run(['docker', 'rm', '--force', NAME], capture_output=True)
    subprocess.run(
        ['docker', 'run', '--detach', '--name', NAME,
         '--security-opt', 'seccomp=unconfined',
         '--security-opt', 'apparmor=unconfined',
         '--security-opt', 'systempaths=unconfined',
         '--env', 'JADPO_VERIFIER_TOKEN',
         '--publish', f'127.0.0.1:{PORT}:8080', image],
        check=True,
    )
    try:
        check()
    except BaseException:
        subprocess.run(['docker', 'logs', NAME])
        raise
    finally:
        subprocess.run(['docker', 'rm', '--force', NAME], capture_output=True)


def check():
    for _ in range(60):
        try:
            status, body = call('GET', '/health', token=None)
            if status == 200:
                assert body == {'status': 'ok'}, body
                break
        except (urllib.error.URLError, ConnectionError):
            pass
        time.sleep(1)
    else:
        raise SystemExit('it never answered')

    status, body = call('GET', '/v1/verifications/x', token=None)
    assert status == 401, (status, body)
    status, body = call('POST', '/v1/verifications', {
        'repository': 'https://169.254.169.254/latest',
        'commit': 'a' * 40,
        'previous_digest': None,
    })
    assert status == 422 and body['error']['code'] == 'repository_refused', (status, body)

    status, body = call('POST', '/v1/resolve', {'repository': REPOSITORY, 'tag': 'v0.1.0'})
    assert status == 200, (status, body)
    commit = body['commit']

    status, body = call('POST', '/v1/verifications', {
        'repository': REPOSITORY,
        'commit': commit,
        'previous_digest': None,
    })
    assert status == 202, (status, body)
    verification = body['id']
    for _ in range(300):
        status, body = call('GET', f'/v1/verifications/{verification}')
        assert status == 200, (status, body)
        if body['state'] != 'verifying':
            break
        time.sleep(1)
    assert body['state'] == 'checked', body
    result = body['result']
    assert result['passed'] is False, result
    assert result['bar'][0]['item'] == 'files', result
    assert result['bar'][0]['state'] == 'failed', result
    print('the image serves, guards its requests and fetches over HTTPS')


if __name__ == '__main__':
    main(sys.argv[1])
