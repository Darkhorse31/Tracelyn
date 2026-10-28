import assert from "node:assert/strict"
import { test } from "node:test"
import { licenseMessage, showTrialBanner, type LicenseSnapshot } from "./license.ts"
const trial: LicenseSnapshot = { status: "Trial", canWrite: true, daysLeft: 14, maskedKey: null, device: "PC", plan: "Trial", error: null }
test("trial reminder appears only in the last three days", () => {
  assert.equal(showTrialBanner(trial), false)
  assert.equal(showTrialBanner({ ...trial, daysLeft: 3 }), true)
  assert.equal(showTrialBanner({ ...trial, daysLeft: 1 }), true)
  assert.equal(showTrialBanner({ ...trial, status: "Expired", daysLeft: 0 }), false)
  assert.equal(licenseMessage({ ...trial, daysLeft: 1 }), "1 day left in your trial")
})
test("read-only states explain that monitoring stays available", () => {
  assert.match(licenseMessage({ ...trial, status: "Expired", canWrite: false }), /Monitoring remains available/)
  assert.match(licenseMessage({ ...trial, status: "Invalid", canWrite: false }), /clock|valid key/)
  assert.match(licenseMessage({ ...trial, status: "Grace" }), /7 days/)
  assert.match(licenseMessage({ ...trial, status: "Active" }), /active/)
})
test("storage errors take precedence and UI copy has no semicolons", () => {
  assert.equal(licenseMessage({ ...trial, error: "Credential access unavailable" }), "Credential access unavailable")
  for (const status of ["Trial", "Active", "Expired", "Grace", "Invalid"] as const) {
    assert.equal(licenseMessage({ ...trial, status }).includes(";"), false)
  }
})
