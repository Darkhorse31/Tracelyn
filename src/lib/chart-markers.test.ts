import assert from "node:assert/strict";
import test from "node:test";
import { markerKindLabel, markerNear, markersInView, type PlotMarker } from "./chart-markers.ts";

function marker(tsMs: number, kind: string): PlotMarker {
  return { id: String(tsMs), serverId: "s", tsMs, kind, label: kind, detail: "detail" };
}

test("markers stay inside the plotted window and in time order", () => {
  const rows = markersInView(
    [marker(5_000, "alert"), marker(2_000, "deploy"), marker(20_000, "cron")],
    1,
    10,
  );
  assert.deepEqual(rows.map((row) => row.kind), ["deploy", "alert"]);
});

test("the nearest marker inside the slack wins", () => {
  const rows = [marker(1_000, "deploy"), marker(10_000, "restart")];
  assert.equal(markerNear(rows, 1.2, 20)?.kind, "deploy");
  assert.equal(markerNear(rows, 5, 20), null);
  assert.equal(markerKindLabel("ops"), "Ops action");
  assert.equal(markerKindLabel("custom"), "custom");
});
