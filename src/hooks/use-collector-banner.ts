import { useEffect, useState } from "react";
import {
  FAILURE_SUSTAINED_MS,
  emptyBannerTrack,
  noteCollectorSample,
  shouldShowCollectorBanner,
  type BannerTrack,
} from "@/lib/collector-banner";

type Sample = {
  collectorError?: string | null;
  collectorName?: string | null;
  collectorFailures?: number | null;
  timestampMs?: number;
} | null;

export function useCollectorBanner(snapshot: Sample) {
  const [track, setTrack] = useState<BannerTrack>(emptyBannerTrack);
  const [nowMs, setNowMs] = useState(() => Date.now());

  useEffect(() => {
    setTrack((prev) =>
      noteCollectorSample(prev, {
        error: snapshot?.collectorError ?? null,
        collectorName: snapshot?.collectorName ?? null,
        failures: snapshot?.collectorFailures ?? 0,
        atMs: snapshot?.timestampMs ?? Date.now(),
      }),
    );
  }, [
    snapshot?.timestampMs,
    snapshot?.collectorError,
    snapshot?.collectorName,
    snapshot?.collectorFailures,
  ]);

  const show = shouldShowCollectorBanner(track, nowMs);

  useEffect(() => {
    if (!track.sinceMs || show || !track.error) return;
    const delay = track.sinceMs + FAILURE_SUSTAINED_MS - Date.now() + 1;
    const id = setTimeout(() => setNowMs(Date.now()), Math.max(0, delay));
    return () => clearTimeout(id);
  }, [track.sinceMs, track.error, show]);

  return {
    show,
    error: track.error,
    collectorName: track.collectorName,
  };
}
