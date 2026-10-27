export const CONSECUTIVE_FAILURES = 3;
export const FAILURE_SUSTAINED_MS = 60_000;

export const COLLECTOR_BANNER_TEXT =
  "We could not refresh some server data. Tracelyn will keep retrying.";

export type CollectorSample = {
  error: string | null;
  collectorName: string | null;
  failures: number;
  atMs: number;
};

export type BannerTrack = {
  failures: number;
  sinceMs: number | null;
  error: string | null;
  collectorName: string | null;
};

export const emptyBannerTrack: BannerTrack = {
  failures: 0,
  sinceMs: null,
  error: null,
  collectorName: null,
};

export function plainCollectorText(value: string): string {
  return value.split(";").join(",");
}

export function noteCollectorSample(track: BannerTrack, sample: CollectorSample): BannerTrack {
  if (!sample.error || sample.failures <= 0) {
    return emptyBannerTrack;
  }
  const continued = track.failures > 0 && sample.failures >= track.failures;
  return {
    failures: sample.failures,
    sinceMs: continued && track.sinceMs != null ? track.sinceMs : sample.atMs,
    error: sample.error,
    collectorName: sample.collectorName,
  };
}

export function shouldShowCollectorBanner(track: BannerTrack, nowMs: number): boolean {
  if (!track.error || track.failures <= 0 || track.sinceMs == null) {
    return false;
  }
  if (track.failures >= CONSECUTIVE_FAILURES) {
    return true;
  }
  return nowMs - track.sinceMs > FAILURE_SUSTAINED_MS;
}
