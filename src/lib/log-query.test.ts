import assert from "node:assert/strict";
import { test } from "node:test";
import type { LogLine } from "./log-types.ts";
import {
  countByField,
  extractTrace,
  grepNeedle,
  groupPatterns,
  lineMatches,
  parseLogQuery,
  patternOf,
} from "./log-query.ts";

function line(partial: Partial<LogLine> & Pick<LogLine, "raw">): LogLine {
  return {
    id: "log-1",
    tsMs: 1,
    sourceId: "app",
    level: "info",
    message: partial.raw,
    ...partial,
  };
}

test("parses fields, quotes, negation, comparisons, and count by", () => {
  const query = parseLogQuery('level:error -"health check" status>=500 | count by source');
  assert.equal(query.countBy, "source");
  assert.equal(query.terms.length, 3);
  assert.deepEqual(query.terms[0], { kind: "field", field: "level", value: "error", neg: false });
  assert.deepEqual(query.terms[1], { kind: "phrase", text: "health check", neg: true });
  assert.deepEqual(query.terms[2], { kind: "cmp", field: "status", op: ">=", value: 500, neg: false });
});

test("matches a line and counts by field", () => {
  const query = parseLogQuery("status>=500 -timeout");
  const hit = line({ raw: "upstream 503", httpStatus: 503, level: "error", sourceId: "nginx" });
  const miss = line({ raw: "upstream timeout 503", httpStatus: 503, sourceId: "nginx" });
  assert.equal(lineMatches(query, hit), true);
  assert.equal(lineMatches(query, miss), false);
  const counted = countByField([hit, hit], "source", () => undefined);
  assert.deepEqual(counted, [{ value: "nginx", count: 2 }]);
  assert.equal(grepNeedle(parseLogQuery("message:refused")), "refused");
  assert.equal(grepNeedle(parseLogQuery("status>=500")), null);
});

test("groups numbers, uuids, and ips into one pattern", () => {
  const a = "connected to 10.0.0.8 user 42 id 11111111-2222-3333-4444-555555555555";
  const b = "connected to 10.1.2.3 user 7 id aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
  assert.equal(patternOf(a), patternOf(b));
  assert.equal(patternOf(a).includes("<ip>"), true);
  assert.equal(patternOf(a).includes("<uuid>"), true);
  assert.equal(patternOf(a).includes("<n>"), true);
  const groups = groupPatterns([a, b, "other line"]);
  assert.equal(groups[0].count, 2);
});

test("extracts trace ids from json, equals, and traceparent", () => {
  assert.deepEqual(extractTrace('{"trace_id":"abc123def456","span_id":"span1"}'), {
    traceId: "abc123def456",
    spanId: "span1",
  });
  assert.equal(extractTrace("trace_id=abc123def456 span_id=span9").traceId, "abc123def456");
  const w3c = extractTrace(
    "traceparent=00-0123456789abcdef0123456789abcdef-0123456789abcdef-01",
  );
  assert.equal(w3c.traceId, "0123456789abcdef0123456789abcdef");
  assert.equal(w3c.spanId, "0123456789abcdef");
});
