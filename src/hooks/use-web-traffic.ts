import { useEffect, useMemo, useState } from "react";
import type {
  LiveLogLine,
  WebTrafficData,
  WebTrafficFilters,
  WebTrafficMetrics,
} from "@/lib/web-traffic-types";

const MOCK_LOGS: LiveLogLine[] = Array.from({ length: 500 }, (_, i) => {
  const status = i % 17 === 0 ? 500 : i % 11 === 0 ? 404 : 200;
  const path = `/api/orders/${1000 + (i % 40)}/items`;
  return {
    id: `log-${i}`,
    timestamp: Date.now() - i * 1200,
    status,
    method: i % 3 === 0 ? "POST" : "GET",
    path,
    remoteAddr: `203.0.113.${(i % 200) + 1}`,
    latencyMs: 20 + (i % 80) + (status >= 500 ? 200 : 0),
    raw: `203.0.113.${(i % 200) + 1} - - [23/Oct/2026:10:${String(i % 60).padStart(2, "0")}:00 +0000] "${i % 3 === 0 ? "POST" : "GET"} ${path} HTTP/1.1" ${status} 1024 "-" "TracelynMock/1.0" 0.0${30 + (i % 50)}`,
  };
});

function buildMetrics(): WebTrafficMetrics {
  const now = Date.now();
  const series = Array.from({ length: 60 }, (_, i) => ({
    t: now - (59 - i) * 1000,
    v: 42 + Math.sin(i / 8) * 12 + (i % 5),
  }));
  return {
    reqPerSec: 48.6,
    bandwidthMbps: 12.4,
    statusBreakdown: { "2xx": 4120, "3xx": 180, "4xx": 96, "5xx": 24 },
    topUrls: [
      { path: "/api/orders/:id/items", count: 890 },
      { path: "/health", count: 620 },
      { path: "/static/app.js", count: 410 },
    ],
    topIps: [
      { ip: "203.0.113.44", count: 320 },
      { ip: "198.51.100.8", count: 210 },
    ],
    reqPerSecSeries: series,
    timingNotice: {
      message:
        "Response time is not in the access log. Add this log_format and point access_log at it:",
      suggestedLine:
        "log_format timed '$remote_addr - $remote_user [$time_local] \"$request\" $status $body_bytes_sent \"$http_referer\" \"$http_user_agent\" $request_time $upstream_response_time';",
    },
  };
}

/** Data hook — swap implementation when ConnectionManager + tail stream are wired. */
export function useWebTraffic(_serverId: string | null): WebTrafficData {
  const [tick, setTick] = useState(0);

  useEffect(() => {
    const id = window.setInterval(() => setTick((t) => t + 1), 2000);
    return () => window.clearInterval(id);
  }, []);

  const metrics = useMemo(() => {
    const m = buildMetrics();
    if (tick > 0) {
      m.reqPerSec = 45 + (tick % 10);
    }
    return m;
  }, [tick]);

  return {
    metrics,
    logs: MOCK_LOGS,
    loading: false,
  };
}

export function filterWebLogs(
  logs: LiveLogLine[],
  filters: WebTrafficFilters,
): LiveLogLine[] {
  const pathQ = filters.pathQuery.trim().toLowerCase();
  const ipQ = filters.ipQuery.trim().toLowerCase();
  const textQ = filters.textQuery.trim().toLowerCase();
  return logs.filter((line) => {
    if (filters.statusMin !== undefined && line.status < filters.statusMin) {
      return false;
    }
    if (filters.statusMax !== undefined && line.status > filters.statusMax) {
      return false;
    }
    if (pathQ && !line.path.toLowerCase().includes(pathQ)) return false;
    if (ipQ && !line.remoteAddr.toLowerCase().includes(ipQ)) return false;
    if (textQ && !line.raw.toLowerCase().includes(textQ)) return false;
    return true;
  });
}
