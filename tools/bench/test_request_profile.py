"""Fault-sensitive inspector protocol controls; no performance thresholds."""
import base64
import copy
import hashlib
import http.server
import json
from pathlib import Path
import socket
import struct
import tempfile
import threading
import unittest

from request_profile import Inspector, validate_profile, payloads, wasm_sections, capture_host, validate_host, summary

GOOD = {'nodes': [{'id': 1, 'callFrame': {'functionName': '(root)', 'url': '', 'scriptId': '0', 'lineNumber': -1, 'columnNumber': -1}, 'children': [2]},
                  {'id': 2, 'callFrame': {'functionName': 'work', 'url': 'wasm://module', 'scriptId': '1', 'lineNumber': 0, 'columnNumber': 1}, 'hitCount': 2}],
        'samples': [2, 2], 'timeDeltas': [100, 200], 'startTime': 1000, 'endTime': 1400}


def exact(stream, size):
    data = bytearray()
    while len(data) < size:
        part = stream.read(size-len(data))
        if not part:
            raise EOFError('closed mock peer')
        data.extend(part)
    return bytes(data)


class Peer(http.server.BaseHTTPRequestHandler):
    protocol_version = 'HTTP/1.1'
    def log_message(self, *args):
        pass
    def do_GET(self):
        mode = self.server.mode
        if self.path == '/json/list':
            target = {'id': 'kiosk', 'title': 'Worker: kiosk', 'type': 'node',
                      'webSocketDebuggerUrl': f'ws://127.0.0.1:{self.server.server_port}/target'}
            targets = [target]
            if mode == 'wrong':
                targets[0]['id'] = 'memory'
            if mode == 'duplicate':
                targets *= 2
            if mode == 'remote':
                targets[0]['webSocketDebuggerUrl'] = 'ws://example.com/target'
            raw = json.dumps(targets).encode()
            self.send_response(200); self.send_header('Content-Length', str(len(raw))); self.end_headers(); self.wfile.write(raw)
            return
        key = self.headers['Sec-WebSocket-Key']
        accept = base64.b64encode(hashlib.sha1((key+'258EAFA5-E914-47DA-95CA-C5AB0DC85B11').encode()).digest()).decode()
        self.send_response(101); self.send_header('Upgrade', 'websocket'); self.send_header('Connection', 'Upgrade'); self.send_header('Sec-WebSocket-Accept', accept); self.end_headers()
        self.connection.settimeout(2)
        try:
            while True:
                first, size = exact(self.rfile, 2)
                masked = size & 128; size &= 127
                if size == 126: size = struct.unpack('!H', exact(self.rfile, 2))[0]
                if size == 127: size = struct.unpack('!Q', exact(self.rfile, 8))[0]
                mask = exact(self.rfile, 4) if masked else None
                raw = exact(self.rfile, size)
                if mask: raw = bytes(c ^ mask[i % 4] for i,c in enumerate(raw))
                if first & 15 == 8: return
                request = json.loads(raw)
                if mode == 'close': return
                if mode in ('truncated', 'slow'):
                    self.wfile.write(bytes([129, 126])+struct.pack('!H', 1000)+b'{'); self.wfile.flush()
                    if mode == 'slow':
                        for _ in range(20):
                            if self.server.release.wait(.08): return
                            self.wfile.write(b' '); self.wfile.flush()
                    return
                if mode == 'timeout':
                    self.server.release.wait(2); return
                answer = {'id': request['id'], 'result': {}}
                if mode == 'omitted' and request['method'] != 'Profiler.stop': answer = {'id': request['id']}
                if mode == 'error': answer = {'id': request['id'], 'error': {'code': -1, 'message': 'mock refusal'}}
                if request['method'] == 'Profiler.stop':
                    profile = copy.deepcopy(GOOD)
                    if mode == 'empty': profile['samples'] = []; profile['timeDeltas'] = []
                    answer['result'] = {'profile': profile}
                raw = b'{' if mode == 'malformed' else json.dumps(answer).encode()
                prefix = bytes([129, len(raw)]) if len(raw) < 126 else bytes([129, 126])+struct.pack('!H', len(raw))
                self.wfile.write(prefix+raw); self.wfile.flush()
        except (EOFError, OSError):
            pass


class InspectorControls(unittest.TestCase):
    def test_weighted_summary_keeps_context_identity_and_inclusive_counts(self):
        # Distinct contexts deliberately share a name. Expected weights/counts
        # are hand-worked: the middle node is sampled once, the leaf twice.
        profile = {'nodes': [
            {'id': 1, 'callFrame': {'functionName': '(root)', 'url': '', 'scriptId': '0', 'lineNumber': -1, 'columnNumber': -1}, 'children': [2]},
            {'id': 2, 'callFrame': {'functionName': 'work', 'url': 'wasm://module', 'scriptId': '1', 'lineNumber': 0, 'columnNumber': 10}, 'children': [3]},
            {'id': 3, 'callFrame': {'functionName': 'work', 'url': 'wasm://module', 'scriptId': '1', 'lineNumber': 0, 'columnNumber': 20}}],
            'samples': [2, 3, 3], 'timeDeltas': [100, 200, 300], 'startTime': 1000, 'endTime': 1700}
        result = summary(profile)
        root = next(row for row in result['inclusive'] if row['function'] == '(root)')
        self.assertEqual(root['samples'], 3)
        inclusive = {row['node_id']: row for row in result['inclusive']}
        own = {row['node_id']: row for row in result['self']}
        self.assertEqual([(inclusive[n]['sampled_us'], inclusive[n]['samples']) for n in (1, 2, 3)],
                         [(600, 3), (600, 3), (500, 2)])
        self.assertEqual([(own[n]['sampled_us'], own[n]['samples']) for n in (2, 3)], [(100, 1), (500, 2)])
        self.assertEqual([(inclusive[n]['parent_id'], inclusive[n]['url'], inclusive[n]['line_number'], inclusive[n]['column_number']) for n in (1, 2, 3)],
                         [(None, '', -1, -1), (1, 'wasm://module', 0, 10), (2, 'wasm://module', 0, 20)])
        self.assertEqual(inclusive[1]['percent'], 100)
        self.assertEqual(result['sampled_us'], 600)

    def test_profile_requires_a_consistent_nonempty_sampled_tree(self):
        validate_profile(GOOD)
        changes = [lambda p: p.update(samples=[]), lambda p: p.update(timeDeltas=[100]),
                   lambda p: p['samples'].__setitem__(0, 99), lambda p: p['nodes'].append(p['nodes'][0]),
                   lambda p: p.update(startTime=float('nan')), lambda p: p.update(endTime=900),
                   lambda p: p['timeDeltas'].__setitem__(0, -1), lambda p: p['timeDeltas'].__setitem__(0, 900),
                   lambda p: p['nodes'][0]['children'].append(99), lambda p: p['nodes'][1].update(children=[1]),
                   lambda p: p['nodes'][1].pop('callFrame')]
        for change in changes:
            bad = copy.deepcopy(GOOD); change(bad)
            with self.subTest(profile=bad), self.assertRaises(ValueError): validate_profile(bad)

    def test_actual_protocol_faults_close_and_do_not_publish_profiles(self):
        for mode in ('good', 'omitted', 'wrong', 'duplicate', 'remote', 'close', 'truncated', 'slow', 'timeout', 'error', 'empty', 'malformed'):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as directory:
                peer = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Peer)
                peer.daemon_threads = True; peer.mode = mode; peer.release = threading.Event()
                thread = threading.Thread(target=peer.serve_forever); thread.start()
                path = Path(directory)/'sample.cpuprofile'
                inspector = Inspector(peer.server_port, path, timeout=.2)
                try:
                    if mode in ('good', 'omitted'):
                        inspector.start(); inspector.stop()
                        self.assertEqual(json.loads(path.read_text()), GOOD)
                        self.assertEqual(inspector.identity['id'], 'kiosk')
                        with self.assertRaises(FileExistsError): Inspector(peer.server_port, path)
                    else:
                        with self.assertRaises((ValueError, OSError, EOFError, TimeoutError)):
                            inspector.start(); inspector.stop()
                        self.assertFalse(path.exists())
                    inspector.close()
                    self.assertIsNone(inspector.socket)
                finally:
                    inspector.close(); peer.release.set(); peer.shutdown(); peer.server_close(); thread.join()

    def test_payload_lengths_are_scalars_bytes_and_wire_bytes(self):
        cases = payloads()
        self.assertEqual([len(cases[k]) for k in cases], [100, 65536, 65536, 65536])
        self.assertEqual(len(cases['ascii-large'].encode()), 65536)
        self.assertGreater(len(cases['unicode-large'].encode()), 65536)
        self.assertEqual(cases['late-escape'][-1], '\n')
        self.assertNotIn('\n', cases['late-escape'][:-1])


    def test_actual_served_files_and_saved_snapshots_cannot_drift(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw); host = root/'host'; host.mkdir(); intended = root/'production'; intended.mkdir()
            for directory in (host, intended):
                (directory/'kiosk.js').write_bytes(b'javascript'); (directory/'kiosk.wasm').write_bytes(b'wasm')
            (host/'config.capnp').write_text('config')
            stamp = {'artifacts': {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in intended.iterdir()}}
            record = capture_host(host, intended, root/'snapshot', stamp)
            validate_host(record)
            (host/'config.capnp').write_text('changed')
            with self.assertRaises(ValueError): validate_host(record)
            (host/'kiosk.wasm').write_bytes(b'wrong')
            with self.assertRaises(ValueError): capture_host(host, intended, root/'bad-snapshot', stamp)
            self.assertFalse((root/'bad-snapshot').exists())
            saved = root/'snapshot/kiosk.js'; saved.write_bytes(b'drift')
            with self.assertRaises(ValueError): validate_host(record)

    def test_diagnostic_equivalence_ignores_only_custom_sections(self):
        header = b'\0asm\x01\0\0\0'
        module = header+b'\x01\x01x'+b'\x00\x02\x01n'
        self.assertEqual(wasm_sections(module), wasm_sections(header+b'\x01\x01x'))
        with self.assertRaises(ValueError): wasm_sections(module[:-1])
        with self.assertRaises(ValueError): wasm_sections(b'wrong')
        self.assertNotEqual(wasm_sections(module), wasm_sections(header+b'\x01\x01y'))


if __name__ == '__main__': unittest.main()
