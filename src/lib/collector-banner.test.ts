import assert from "node:assert/strict";
import { test } from "node:test";
import {
  COLLECTOR_BANNER_TEXT,
  CONSECUTIVE_FAILURES,
  FAILURE_SUSTAINED_MS,
  emptyBannerTrack,
  noteCollectorSample,
  plainCollectorText,
  shouldShowCollectorBanner,
  type BannerTrack,
  type CollectorSample,
} from "./collector-banner.ts";

function sample(failures: number, atMs: number, error: string | null = "SSH: Channel send error"): CollectorSample {
  return {
    error,
    collectorName: error ? "system" : null,
    failures,
    atMs,
  };
}

function trackAfter(steps: CollectorSample[]): BannerTrack {
  return steps.reduce(noteCollectorSample, emptyBannerTrack);
}

test("banner copy has no semicolon", () => {
  assert.equal(COLLECTOR_BANNER_TEXT.includes(";"), false);
  const raw = `SSH: Channel send error${String.fromCharCode(59)} closed`;
  assert.equal(plainCollectorText(raw).includes(";"), false);
  assert.equal(plainCollectorText(raw), "SSH: Channel send error, closed");
});

test("a single failure that recovers stays hidden", () => {
  const failed = trackAfter([sample(1, 1_000)]);
  assert.equal(shouldShowCollectorBanner(failed, 1_000), false);
  const recovered = noteCollectorSample(failed, sample(0, 2_000, null));
  assert.equal(recovered.failures, 0);
  assert.equal(shouldShowCollectorBanner(recovered, 2_000), false);
});

test("three consecutive failures show the banner", () => {
  const track = trackAfter([sample(1, 0), sample(2, 2_000), sample(3, 4_000)]);
  assert.equal(track.failures, CONSECUTIVE_FAILURES);
  assert.equal(track.sinceMs, 0);
  assert.equal(shouldShowCollectorBanner(track, 4_000), true);
  assert.equal(shouldShowCollectorBanner(trackAfter([sample(1, 0), sample(2, 2_000)]), 2_000), false);
});

test("one failure shows after more than 60 seconds", () => {
  const track = trackAfter([sample(1, 0)]);
  assert.equal(shouldShowCollectorBanner(track, FAILURE_SUSTAINED_MS), false);
  assert.equal(shouldShowCollectorBanner(track, FAILURE_SUSTAINED_MS + 1), true);
  assert.equal(track.collectorName, "system");
  assert.equal(track.error, "SSH: Channel send error");
});

test("repeating the same failure count does not restart the clock", () => {
  const first = trackAfter([sample(1, 500)]);
  const again = noteCollectorSample(first, sample(1, 90_000));
  assert.equal(again.sinceMs, 500);
  assert.equal(shouldShowCollectorBanner(again, 500 + FAILURE_SUSTAINED_MS + 1), true);
});
