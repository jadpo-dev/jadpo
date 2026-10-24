import { isIP } from 'node:net';

export function trustedPeer(request, peer) {
  const actual = peer.peer ?? peer.incoming?.socket.remoteAddress;
  const forwarded = request.headers.get('x-forwarded-for');
  return ['127.0.0.1', '::1', '::ffff:127.0.0.1'].includes(actual)
    && forwarded !== null && isIP(forwarded) === 4
    ? { peer: forwarded } : peer;
}
