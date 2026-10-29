import assert from "node:assert/strict";
import { test } from "node:test";
import type { LogLine } from "./log-types.ts";
import {
  RANGE_OPTIONS,
  SOURCE_GROUPS,
  deleteSavedFilter,
  filterLogLines,
  groupSources,
  queryRegexError,
  rangeStart,
  readSavedFilters,
  renameSavedFilter,
  sourceGroup,
  splitQuery,
  toggleSourceGroup,
  upsertSavedFilter,
  visibleSources,
} from "./log-view.ts";

const now = 1_700_000_000_000;

function line(partial: Partial<LogLine> & Pick<LogLine, "id" | "raw">): LogLine {
  return {
    tsMs: now,
    sourceId: "src",
    level: "info",
    message: partial.raw,
    ...partial,
  };
}

test("time windows include 5m and 7d and ignore live", () => {
  assert.equal(rangeStart("5m", "", now), now - 5 * 60_000);
  assert.equal(rangeStart("15m", "", now), now - 15 * 60_000);
  assert.equal(rangeStart("1h", "", now), now - 60 * 60_000);
  assert.equal(rangeStart("24h", "", now), now - 24 * 60 * 60_000);
  assert.equal(rangeStart("7d", "", now), now - 7 * 24 * 60 * 60_000);
  assert.equal(rangeStart("live", "", now), null);
  assert.equal(rangeStart("custom", "2024-01-01T00:00:00.000Z", now), Date.parse("2024-01-01T00:00:00.000Z"));
  assert.equal(rangeStart("custom", "", now), null);
  assert.equal(rangeStart("custom", "not-a-date", now), null);
});

test("minus terms exclude without changing a plain phrase", () => {
  assert.deepEqual(splitQuery("connection refused", false), {
    needle: "connection refused",
    excludes: [],
  });
  assert.deepEqual(splitQuery("error -health timeout", false), {
    needle: "error timeout",
    excludes: ["health"],
  });
  assert.deepEqual(splitQuery("-health", false), { needle: "", excludes: ["health"] });
  assert.deepEqual(splitQuery("foo -bar", true), { needle: "foo -bar", excludes: [] });
});

test("filters keep level, source, http, exclude, and regex behavior", () => {
  const lines = [
    line({ id: "1", appId: "apache", level: "error", raw: "disk full", httpStatus: 500, tsMs: now - 1_000 }),
    line({ id: "2", appId: "pm2", level: "info", raw: "health ok", tsMs: now - 1_000 }),
    line({ id: "3", appId: "pm2", level: "error", raw: "disk (retry)", tsMs: now - 10 * 60_000 }),
  ];
  const base = {
    lines,
    pickedApps: [] as string[],
    levels: [] as LogLine["level"][],
    range: "live" as const,
    customStart: "",
    text: "",
    useRegex: false,
    exclude: "",
    now,
  };
  assert.deepEqual(
    filterLogLines({ ...base, text: "disk -full" }).map((item) => item.id),
    ["3"],
  );
  assert.deepEqual(
    filterLogLines({ ...base, exclude: "health" }).map((item) => item.id),
    ["1", "3"],
  );
  assert.deepEqual(
    filterLogLines({ ...base, levels: ["error"] }).map((item) => item.id),
    ["1", "3"],
  );
  assert.deepEqual(
    filterLogLines({ ...base, pickedApps: ["apache"] }).map((item) => item.id),
    ["1"],
  );
  assert.deepEqual(
    filterLogLines({ ...base, requireHttp: true }).map((item) => item.id),
    ["1"],
  );
  assert.deepEqual(
    filterLogLines({ ...base, lockedAppId: "pm2" }).map((item) => item.id),
    ["2", "3"],
  );
  assert.deepEqual(
    filterLogLines({ ...base, range: "5m" }).map((item) => item.id),
    ["1", "2"],
  );
  assert.match(queryRegexError("(", true) ?? "", /SyntaxError/);
  assert.deepEqual(
    filterLogLines({ ...base, text: "(", useRegex: true }).map((item) => item.id),
    ["3"],
  );
  assert.deepEqual(
    filterLogLines({ ...base, text: "disk\\s+full", useRegex: true }).map((item) => item.id),
    ["1"],
  );
});

test("sources group like the fleet and hide empty counts", () => {
  const apps = [
    { id: "1", kind: "apache", name: "apache:example.com" },
    { id: "2", kind: "pm2", name: "bharattales-api" },
    { id: "3", kind: "pm2", name: "cuekala-web" },
    { id: "4", kind: "systemd", name: "mysql" },
    { id: "5", kind: "service", name: "redis" },
    { id: "6", kind: "systemd", name: "sshd" },
    { id: "7", kind: "systemd", name: "systemd-journald" },
    { id: "8", kind: "docker", name: "redis" },
    { id: "9", kind: "pm2", name: "redis-worker" },
    { id: "10", kind: "nginx", name: "example.com" },
  ];
  assert.equal(sourceGroup("apache", "apache:example.com"), "web");
  assert.equal(sourceGroup("pm2", "bharattales-api"), "apps");
  assert.equal(sourceGroup("pm2", "redis-worker"), "apps");
  assert.equal(sourceGroup("systemd", "mysql"), "data");
  assert.equal(sourceGroup("docker", "redis"), "data");
  assert.equal(sourceGroup("systemd", "sshd"), "system");
  const groups = groupSources(apps);
  assert.deepEqual(
    groups.map((group) => group.label),
    SOURCE_GROUPS.map((group) => group.label),
  );
  assert.deepEqual(
    groups.find((group) => group.id === "apps")?.items.map((item) => item.name),
    ["bharattales-api", "cuekala-web", "redis-worker"],
  );
  const counts = { "1": 2, "2": 0, "4": 3 };
  assert.deepEqual(
    visibleSources(apps, counts, "", false).map((item) => item.id),
    ["1", "4"],
  );
  assert.equal(visibleSources(apps, counts, "mysql", true).length, 1);
  assert.deepEqual(toggleSourceGroup([], ["1", "2"]), ["1", "2"]);
  assert.deepEqual(toggleSourceGroup(["2"], ["1", "2"]), ["2", "1"]);
  assert.deepEqual(toggleSourceGroup(["1", "2"], ["1", "2"]), []);
});

test("saved filters save, rename, and delete", () => {
  const filter = {
    name: "Errors",
    levels: ["error" as const],
    text: "disk",
    useRegex: false,
    exclude: "",
    appIds: ["1"],
    range: "15m" as const,
  };
  const saved = upsertSavedFilter([], filter);
  const both = upsertSavedFilter(saved, { ...filter, name: "Disk", text: "other" });
  assert.equal(upsertSavedFilter(saved, { ...filter, text: "disk full" })[0]?.text, "disk full");
  assert.equal(renameSavedFilter(both, "Errors", "Disk"), null);
  const renamed = renameSavedFilter(saved, "Errors", "Disk errors");
  assert.equal(renamed?.[0]?.name, "Disk errors");
  assert.equal(renameSavedFilter(saved, "Errors", "  "), saved);
  assert.deepEqual(deleteSavedFilter(saved, "Errors"), []);
  assert.deepEqual(readSavedFilters(null), []);
  assert.deepEqual(readSavedFilters("nope"), []);
  assert.equal(readSavedFilters(JSON.stringify(saved))[0]?.name, "Errors");
  for (const option of RANGE_OPTIONS) assert.equal(option.label.includes(";"), false);
  for (const group of SOURCE_GROUPS) assert.equal(group.label.includes(";"), false);
});
