import { test } from 'node:test';
import assert from 'node:assert/strict';
import { trustedPeer } from './golden_proxy.mjs';

test('only the declared actual loopback proxy can supply one strict IPv4 address', () => {
  const request = value => new Request('http://127.0.0.1/', {
    headers: value === null ? {} : { 'x-forwarded-for': value },
  });
  for (const actual of ['127.0.0.1', '::1', '::ffff:127.0.0.1']) {
    assert.deepEqual(trustedPeer(request('10.1.2.3'), { peer: actual }), { peer: '10.1.2.3' });
  }
  assert.deepEqual(trustedPeer(request('10.1.2.3'), {
    incoming: { socket: { remoteAddress: '127.0.0.1' } },
  }), { peer: '10.1.2.3' });
  for (const value of [null, '', '10.1.2.3, 10.2.3.4', 'hostname', '127.01.0.1', '::1']) {
    const peer = { peer: '127.0.0.1' };
    assert.equal(trustedPeer(request(value), peer), peer);
  }
  for (const actual of ['unknown', '192.0.2.1', '127.0.0.2', undefined]) {
    const peer = { peer: actual };
    assert.equal(trustedPeer(request('10.1.2.3'), peer), peer);
  }
});
