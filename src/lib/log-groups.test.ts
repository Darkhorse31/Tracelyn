import assert from "node:assert/strict";
import { test } from "node:test";
import type { LogLine } from "./log-types.ts";
import { firstLine, groupBody, groupLogLines, groupText, textLineCount } from "./log-groups.ts";

function line(id: string, raw: string, level: LogLine["level"] = "error", multilineGroupId?: string): LogLine {
  return {
    id,
    tsMs: 1_700_000_000_000,
    sourceId: "stderr",
    level,
    message: raw.slice(0, 500),
    raw,
    multilineGroupId,
  };
}

test("postgres fields, node version, and segfault collapse into one entry", () => {
  const groups = groupLogLines([
    line("1", "error: relation \"users\" does not exist"),
    line("2", "    at Parser.parseErrorMessage (parser.js:285:98)"),
    line("3", "severity: 'ERROR',"),
    line("4", "code: '42P01',"),
    line("5", "file: 'parse_relation.c',"),
    line("6", "line: '1449',"),
    line("7", "routine: 'parserOpenTable'"),
    line("8", "}"),
    line("9", "Node.js v24.19.0"),
    line("10", "Segmentation fault"),
    line("11", "listening on 3000", "info"),
  ]);
  assert.equal(groups.length, 2);
  assert.equal(groups[0].lines.length, 10);
  assert.equal(groups[0].id, "1");
  assert.equal(firstLine(groupBody(groups[0])), 'error: relation "users" does not exist');
  assert.equal(textLineCount(groupBody(groups[0])), 10);
  assert.equal(groups[1].lines.length, 1);
  assert.equal(groups[1].level, "info");
});

test("plain error lines stay separate", () => {
  const groups = groupLogLines([
    line("1", "disk full"),
    line("2", "connection reset"),
  ]);
  assert.equal(groups.length, 2);
  assert.deepEqual(
    groups.map((group) => group.id),
    ["1", "2"],
  );
});

test("multiline group ids bind lines that are not stack frames", () => {
  const groups = groupLogLines([
    line("1", "Error: boom", "error", "1"),
    line("2", "worker exited", "error", "1"),
    line("3", "next request", "info"),
  ]);
  assert.equal(groups.length, 2);
  assert.equal(groups[0].lines.length, 2);
});

test("a runtime trailer does not attach to an unrelated info line", () => {
  const groups = groupLogLines([
    line("1", "health ok", "info"),
    line("2", "Node.js v24.19.0", "error"),
    line("3", "Segmentation fault", "fatal"),
  ]);
  assert.equal(groups.length, 2);
  assert.equal(groups[0].lines.length, 1);
  assert.equal(groups[1].lines.length, 2);
});

test("a multi-line error object is one text group", () => {
  const text = [
    "error: relation missing",
    "    at Parser.parseErrorMessage (parser.js:1:1)",
    "severity: 'ERROR',",
    "code: '42P01',",
    "file: 'parse_relation.c',",
    "line: '1449',",
    "routine: 'parserOpenTable'",
    "Node.js v24.19.0",
    "Segmentation fault",
  ].join("\n");
  const groups = groupText(text);
  assert.equal(groups.length, 1);
  assert.equal(groups[0].length, 9);
  assert.equal(firstLine(text), "error: relation missing");
});
