import assert from "node:assert/strict";
import { test } from "node:test";
import { clientLabel, maskSensitive } from "./privacy.ts";

const catalog = {
  servers: [{ host: "147.93.154.90", username: "deploy", port: 22, keyPath: "C:\\keys\\id_ed25519" }],
};

test("masks ipv4 and keeps a stable client label", () => {
  const once = maskSensitive("client 10.1.2.3 hit", { servers: [] });
  const twice = maskSensitive("client 10.1.2.3 hit", { servers: [] });
  assert.equal(once, twice);
  assert.equal(once.includes("10.1.2.3"), false);
  assert.match(once, /client-[0-9a-f]{3}/);
  assert.equal(clientLabel("10.1.2.3"), clientLabel("10.1.2.3"));
});

test("masks ipv6", () => {
  const out = maskSensitive("src 2001:db8::1 end", { servers: [] });
  assert.equal(out.includes("2001"), false);
  assert.match(out, /client-[0-9a-f]{3}/);
});

test("masks credential urls and bearer tokens", () => {
  const url = maskSensitive("see https://alice:s3cret@example.com/hook", { servers: [] });
  assert.equal(url.includes("alice"), false);
  assert.equal(url.includes("s3cret"), false);
  assert.match(url, /https:\/\/•••:•••@/);
  const bearer = maskSensitive("Authorization: Bearer abc.def-ghi", { servers: [] });
  assert.equal(bearer.includes("abc.def"), false);
  assert.match(bearer, /Bearer •••/);
});

test("masks emails and known server endpoints", () => {
  const email = maskSensitive("write ada@example.com", { servers: [] });
  assert.equal(email.includes("ada@"), false);
  assert.match(email, /email-[0-9a-f]{3}/);
  const endpoint = maskSensitive("deploy@147.93.154.90:22", catalog);
  assert.equal(endpoint, "user@server-1");
  assert.equal(maskSensitive("C:\\keys\\id_ed25519", catalog), "key-file");
  const ppk = maskSensitive("key C:\\Users\\a\\Documents\\work.ppk end", { servers: [] });
  assert.equal(ppk.includes("work.ppk"), false);
  assert.equal(ppk.includes("Documents"), false);
  assert.match(ppk, /key-file/);
});
