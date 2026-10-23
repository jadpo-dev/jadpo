import { describe, expect, it } from "vitest";
import { uniqueJson } from "../src/strict-json.ts";
import { publicAddress, publicHttps } from "../src/egress.ts";
import { Title } from "../src/schemas.ts";
describe("credential JSON and boundary text", () => {
  it("refuses duplicate members at every nesting depth, including escaped names", () => {
    for (const raw of [
      '{"sub":"alice","sub":"bob"}',
      '{"nested":[{"x":1,"\\u0078":2}]}',
      '{"a":{},"a":{}}',
    ])
      expect(() => uniqueJson(raw)).toThrow();
    expect(uniqueJson('{"array":[1,true,null,{"ok":"value"}]}')).toEqual({
      array: [1, true, null, { ok: "value" }],
    });
  });
  it("normalizes NFC before enforcing Unicode scalar bounds", () => {
    expect(Title.parse("e\u0301".repeat(200))).toBe("é".repeat(200));
    expect(Title.parse("😀".repeat(200))).toHaveLength(400);
    expect(Title.safeParse("😀".repeat(201)).success).toBe(false);
    expect(Title.safeParse("\ud800").success).toBe(false);
  });
});
describe("outbound destination guard", () => {
  it("refuses private and transition address bypasses", () => {
    for (const address of [
      "127.0.0.1",
      "10.0.0.1",
      "169.254.169.254",
      "100.64.0.1",
      "::1",
      "::ffff:127.0.0.1",
      "64:ff9b::7f00:1",
      "2002:7f00:1::",
      "2001::1",
      "fec0::1",
    ])
      expect(publicAddress(address), address).toBe(false);
    expect(publicAddress("8.8.8.8")).toBe(true);
    expect(publicAddress("2606:4700:4700::1111")).toBe(true);
  });
  it("requires credential-free public HTTPS and rejects redirects at the transport boundary", () => {
    for (const url of [
      "http://identity.example",
      "https://127.0.0.1",
      "https://user:secret@identity.example",
      "https://localhost",
    ])
      expect(() => publicHttps(url)).toThrow();
    expect(publicHttps("https://keys.example/jwks").hostname).toBe(
      "keys.example",
    );
  });
});

it("allows public embedded IPv4 destinations and explicit public HTTPS ports", () => {
  for (const address of [
    "::ffff:8.8.8.8",
    "::8.8.8.8",
    "64:ff9b::808:808",
    "2002:808:808::",
    "2001:0:808:808:0:0:f7f7:f7f7",
  ])
    expect(publicAddress(address), address).toBe(true);
  expect(publicHttps("https://keys.example:8443/jwks").port).toBe("8443");
  for (const raw of ['{"sub":"\\ud800"}', '{"\\ud800":"value"}'])
    expect(() => uniqueJson(raw)).toThrow();
});
