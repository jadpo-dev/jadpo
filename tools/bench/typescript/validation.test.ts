import test from 'node:test';
import assert from 'node:assert/strict';
import { OpenAPIHono, createRoute } from '@hono/zod-openapi';
import { scalarText } from './validation.js';

test('text bounds count Unicode scalars rather than UTF-16 units or graphemes', () => {
  const title = scalarText(1, 200);
  assert.equal(title.safeParse('a'.repeat(200)).success, true);
  assert.equal(title.safeParse('a'.repeat(201)).success, false);
  assert.equal(title.safeParse('😀'.repeat(200)).success, true);
  assert.equal(title.safeParse('😀'.repeat(201)).success, false);
  assert.equal(title.safeParse('e\u0301'.repeat(100)).success, true);
  assert.equal(title.safeParse('e\u0301'.repeat(101)).success, false);
  assert.equal(title.safeParse('').success, false);
});

test('unpaired surrogates are not Unicode scalar values', () => {
  const title = scalarText(1, 200);
  for (const invalid of ['\ud800', '\udfff', 'a\ud800b', '\ud800\ud800']) {
    assert.equal(title.safeParse(invalid).success, false);
  }
});

test('large text has the same exact scalar boundary', () => {
  const payload = scalarText(0, 65536);
  assert.equal(payload.safeParse('').success, true);
  assert.equal(payload.safeParse('x'.repeat(65536)).success, true);
  assert.equal(payload.safeParse('😀'.repeat(65536)).success, true);
  assert.equal(payload.safeParse('😀'.repeat(65537)).success, false);
});

test('OpenAPI publishes the same scalar minLength and maxLength contract', () => {
  const app = new OpenAPIHono();
  app.openapi(createRoute({ method: 'get', path: '/', responses: { 200: { description: 'Title', content: { 'application/json': { schema: scalarText(1, 200).openapi('Title') } } } } }), c => c.json('title', 200));
  const doc = app.getOpenAPIDocument({ openapi: '3.1.0', info: { title: 'Test', version: '1' } });
  const title = doc.components?.schemas?.Title;
  assert.ok(title && !('$ref' in title));
  assert.equal(title.type, 'string');
  assert.equal(title.minLength, 1);
  assert.equal(title.maxLength, 200);
});
