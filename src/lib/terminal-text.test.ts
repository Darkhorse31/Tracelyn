import assert from "node:assert/strict";
import { test } from "node:test";
import { extractLastOutput, stripAnsi } from "./terminal-text.ts";

test("strips ansi colors and titles", () => {
  assert.equal(stripAnsi("\u001b[31mred\u001b[0m"), "red");
  assert.equal(stripAnsi("\u001b]0;title\u0007$ "), "$ ");
});

test("last output uses shell integration markers", () => {
  const raw = "prompt\u001b]133;C\u0007hello\nworld\u001b]133;D;0\u0007";
  assert.equal(extractLastOutput(raw), "hello\nworld");
});

test("last output uses the previous prompt when a new one is waiting", () => {
  const raw = "user@host:~$ ls\nfile.txt\nuser@host:~$ ";
  assert.equal(extractLastOutput(raw), "file.txt");
});

test("last output falls back to text after enter", () => {
  assert.equal(extractLastOutput("ls\r\nfile.txt\n").trim(), "file.txt");
});
