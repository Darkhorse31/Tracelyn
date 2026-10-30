import { test } from "node:test";
import assert from "node:assert/strict";
import { healthScore, nextStep, issueLabels } from "./incident-health.ts";
test("health scores account for every issue and clamp to zero", () => {
  assert.equal(healthScore(true, []), 100);
  assert.equal(healthScore(false, []), 0);
  assert.equal(healthScore(true, [{severity:"critical"},{severity:"warning"},{severity:"info"}]), 40);
  assert.equal(healthScore(true, Array(4).fill({severity:"critical"})), 0);
});
test("every issue type has a plain next step with no semicolons", () => {
  for (const kind of Object.keys(issueLabels)) {
    assert.ok(nextStep(kind).length > 20);
    assert.ok(!nextStep(kind).includes(";"));
    assert.ok(!issueLabels[kind].includes(";"));
  }
});
