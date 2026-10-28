import assert from "node:assert/strict";
import { test } from "node:test";
import {
  activeFilterCount,
  compareFleetRows,
  filterAndSortFleet,
  groupFleetRows,
  seenLabel,
  statusCounts,
  tagsFromStored,
  toggleFleetSort,
  type FleetViewRow,
} from "./fleet-view.ts";

function row(partial: Partial<FleetViewRow> & Pick<FleetViewRow, "id" | "name">): FleetViewRow {
  return {
    host: "10.0.0.1",
    username: "root",
    labels: [],
    status: "connected",
    cpu: 10,
    memory: 20,
    disk: 30,
    apps: 1,
    alerts: 0,
    lastSeen: 1_000,
    ...partial,
  };
}

const rows: FleetViewRow[] = [
  row({ id: "a", name: "Alpha", labels: ["prod", "web"], status: "connected", cpu: 80, alerts: 1 }),
  row({ id: "b", name: "Beta", host: "db.internal", labels: ["prod"], status: "offline", cpu: 5, alerts: 3 }),
  row({ id: "c", name: "Gamma", username: "deploy", labels: ["staging"], status: "degraded", cpu: null, alerts: 0 }),
];

test("filters by query, status, and any selected tag", () => {
  const filters = { query: "prod", status: "all" as const, tags: [] };
  assert.deepEqual(
    filterAndSortFleet(rows, filters, "name", true).map((item) => item.id),
    ["a", "b"],
  );
  assert.deepEqual(
    filterAndSortFleet(rows, { query: "", status: "offline", tags: [] }, "name", true).map(
      (item) => item.id,
    ),
    ["b"],
  );
  assert.deepEqual(
    filterAndSortFleet(
      rows,
      { query: "alpha", status: "all", tags: ["staging", "web"] },
      "name",
      true,
    ).map((item) => item.id),
    ["a"],
  );
  assert.equal(
    filterAndSortFleet(rows, { query: "  DEPLOY ", status: "all", tags: [] }, "name", true)[0]?.id,
    "c",
  );
  assert.deepEqual(
    filterAndSortFleet(rows, { query: "", status: "all", tags: ["web", "staging"] }, "name", true).map(
      (item) => item.id,
    ),
    ["a", "c"],
  );
});

test("empty tag selection matches the old all-tags filter", () => {
  assert.equal(
    filterAndSortFleet(rows, { query: "", status: "all", tags: [] }, "name", true).length,
    3,
  );
  assert.deepEqual(tagsFromStored("all"), []);
  assert.deepEqual(tagsFromStored("prod"), ["prod"]);
  assert.deepEqual(tagsFromStored(["web", "db"]), ["web", "db"]);
  assert.deepEqual(tagsFromStored(null), []);
});

test("sorts names, ranks status, and keeps missing metrics last", () => {
  const byName = [...rows].sort((a, b) => compareFleetRows(a, b, "name", false));
  assert.deepEqual(byName.map((item) => item.name), ["Gamma", "Beta", "Alpha"]);

  const byStatus = [...rows].sort((a, b) => compareFleetRows(a, b, "status", true));
  assert.deepEqual(byStatus.map((item) => item.status), ["offline", "degraded", "connected"]);

  const byCpu = [...rows].sort((a, b) => compareFleetRows(a, b, "cpu", true));
  assert.deepEqual(byCpu.map((item) => item.id), ["b", "a", "c"]);
  assert.equal(compareFleetRows(rows[0], rows[0], "alerts", false), 0);
});

test("groups by the first tag and counts active filters", () => {
  const sorted = filterAndSortFleet(rows, { query: "", status: "all", tags: [] }, "name", true);
  const groups = groupFleetRows(sorted, true);
  assert.deepEqual(
    groups.map((group) => [group.label, group.rows.map((item) => item.id)]),
    [
      ["prod", ["a", "b"]],
      ["staging", ["c"]],
    ],
  );
  assert.equal(groupFleetRows(sorted, false)[0]?.label, "");
  assert.equal(activeFilterCount({ query: " x ", status: "offline", tags: ["a", "b"] }), 4);
  assert.equal(activeFilterCount({ query: " ", status: "all", tags: [] }), 0);
  assert.equal(statusCounts(rows).offline, 1);
  assert.equal(statusCounts(rows).connected, 1);
});

test("column click toggles direction and alerts default to descending", () => {
  assert.deepEqual(toggleFleetSort("status", true, "status"), {
    sortKey: "status",
    ascending: false,
  });
  assert.deepEqual(toggleFleetSort("status", true, "alerts"), {
    sortKey: "alerts",
    ascending: false,
  });
  assert.deepEqual(toggleFleetSort("alerts", false, "name"), {
    sortKey: "name",
    ascending: true,
  });
});

test("formats last seen without a clock in the copy", () => {
  const now = 1_000_000;
  assert.equal(seenLabel(null, now), "Never");
  assert.equal(seenLabel(now - 5_000, now), "Just now");
  assert.equal(seenLabel(now - 45_000, now), "45s ago");
  assert.equal(seenLabel(now - 120_000, now), "2m ago");
  assert.equal(seenLabel(now - 7_200_000, now), "2h ago");
});
