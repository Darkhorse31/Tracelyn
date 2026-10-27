import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import {
  exportServerCsv,
  importDuplicates,
  importInput,
  parseServerCsv,
  parseSshConfig,
} from "./server-import.ts";
import type { ServerRecord } from "./types.ts";

const fixtures = JSON.parse(
  readFileSync(
    new URL("./server-import.fixtures.json", import.meta.url),
    "utf8",
  ),
);
for (const fixture of fixtures)
  test(fixture.name, () => {
    const result =
      fixture.kind === "ssh"
        ? parseSshConfig(fixture.input, "/home/me", "me")
        : parseServerCsv(fixture.input);
    assert.deepEqual(result.servers, fixture.servers);
  });
test("SSH patterns, first values, Match, Include, quotes, and invalid ports", () => {
  const preview = parseSshConfig(
    'Include config.d/*\nHost *\n User first\nHost a b\n User second\n Port 0\n IdentityFile "C:\\keys\\my key"\nMatch exec unsafe\n HostName ignored\nHost c\n Port 65536',
    "/home/me",
    "me",
  );
  assert.equal(preview.warnings.length, 2);
  assert.equal(preview.servers.length, 3);
  assert.equal(preview.servers[0].username, "first");
  assert.equal(preview.servers[0].keyPath, "C:\\keys\\my key");
  assert.equal(preview.servers[0].host, "a");
  assert.equal(preview.servers[0].issues.length, 1);
  assert.equal(preview.servers[2].issues.length, 1);
  assert.throws(() => importInput(preview.servers[0]));
});
test("CSV rejects malformed quotes, headers, secrets, and marks bad rows", () => {
  for (const raw of [
    "",
    "host\na",
    "host,host,username\na,b,c",
    "host,username,password\na,b,c",
    'host,username\n"a,b',
    'host,username\n"a"oops,b',
  ])
    assert.throws(() => parseServerCsv(raw));
  const result = parseServerCsv(
    "host,username,port,authType\na,b,0,invalid\na,b,22\n",
  );
  assert.equal(result.servers[0].issues.length, 2);
  assert.equal(result.servers[1].issues.length, 1);
});
test("CSV round trips multiline, quotes, Unicode, labels, and excludes secrets", () => {
  const server = {
    id: "one",
    name: 'West, "primary"\n東京',
    host: "vps.example.com",
    port: 2222,
    username: "deploy",
    authType: "key",
    keyPath: "C:\\my keys\\id_ed25519",
    labels: ["prod", "a|b", "with,comma"],
    password: "never-export-this",
    keyPassphrase: "also-secret",
  } as ServerRecord;
  const csv = exportServerCsv([server]);
  assert.ok(!csv.includes("never-export-this"));
  assert.ok(!csv.includes("also-secret"));
  assert.ok(!csv.includes("password"));
  const row = parseServerCsv(csv).servers[0];
  assert.deepEqual(importInput(row), {
    name: server.name,
    host: server.host,
    port: 2222,
    username: "deploy",
    authType: "key",
    keyPath: server.keyPath,
    labels: server.labels,
  });
});
test("Duplicates normalize host casing and trailing dots and include aliases in a batch", () => {
  const rows = parseServerCsv(
    "name,host,username\nfirst,EXAMPLE.COM.,deploy\nsecond,example.com,deploy\nother,example.com,root\n",
  ).servers;
  assert.equal(importDuplicates(rows, []).get(1), "first");
  const existing = [
    {
      id: "one",
      name: "Existing",
      host: "example.com",
      username: "deploy",
      port: 22,
      authType: "key",
      labels: [],
    },
  ] as ServerRecord[];
  assert.equal(importDuplicates(rows, existing).get(0), "Existing");
  assert.equal(importDuplicates(rows, existing).has(2), false);
});
