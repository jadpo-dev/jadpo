"""Pinned workerd must overwrite caller-supplied Cloudflare identity headers."""
import http.client
import json
from pathlib import Path
import subprocess
import tempfile

import run as spike


def main():
    executable = spike.workerd()
    with tempfile.TemporaryDirectory(prefix='bench-ingress-') as temporary:
        directory = Path(temporary)
        port = spike.free_port()
        (directory / 'main.mjs').write_text('export default {fetch(request){return Response.json({client:request.headers.get("CF-Connecting-IP")});}};')
        (directory / 'config.capnp').write_text(f'''using Workerd = import "/workerd/workerd.capnp";
const config :Workerd.Config = (
  services = [(name = "echo", worker = (modules = [(name = "main.mjs", esModule = embed "main.mjs")], compatibilityDate = "2026-10-01"))],
  sockets = [(name = "http", address = "127.0.0.1:{port}", http = {spike.LOOPBACK_WORKER_HTTP}, service = "echo")],
);
''')
        with tempfile.TemporaryFile() as log:
            process = subprocess.Popen([str(executable), 'serve', 'config.capnp'], cwd=directory,
                                       stdout=log, stderr=log, stdin=subprocess.DEVNULL)
            try:
                spike.wait_for_port(port)
                for headers in ({}, {'CF-Connecting-IP': '192.0.2.5'},
                                {'CF-Connecting-IP': 'forged, malformed', 'X-Forwarded-For': '203.0.113.8'}):
                    connection = http.client.HTTPConnection('127.0.0.1', port, timeout=5)
                    try:
                        connection.request('GET', '/', headers=headers)
                        response = connection.getresponse()
                        actual = json.loads(response.read())
                        if response.status != 200 or actual != {'client': '127.0.0.1'}:
                            raise AssertionError('The trusted loopback socket did not overwrite the caller header')
                    finally:
                        connection.close()
            finally:
                spike.stop(process)
    print('passed: workerd ingress · missing and forged client headers overwritten')


if __name__ == '__main__':
    main()
