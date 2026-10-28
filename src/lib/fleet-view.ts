import type { ConnectionStatus } from "./types.ts";

export type FleetSortKey =
  | "name"
  | "status"
  | "cpu"
  | "memory"
  | "disk"
  | "apps"
  | "alerts"
  | "lastSeen";

export const FLEET_SORT_COLUMNS: { key: FleetSortKey; label: string }[] = [
  { key: "name", label: "Server" },
  { key: "status", label: "Status" },
  { key: "cpu", label: "CPU" },
  { key: "memory", label: "Memory" },
  { key: "disk", label: "Disk" },
  { key: "apps", label: "Apps" },
  { key: "alerts", label: "Active alerts" },
  { key: "lastSeen", label: "Last seen" },
];

/** Lower rank sorts first when ascending, so problems surface before healthy hosts. */
export const FLEET_STATUS_RANK: Record<ConnectionStatus, number> = {
  offline: 0,
  degraded: 1,
  disconnected: 2,
  connecting: 3,
  connected: 4,
};

export interface FleetViewRow {
  id: string;
  name: string;
  host: string;
  username: string;
  labels: string[];
  status: ConnectionStatus;
  cpu?: number | null;
  memory?: number | null;
  disk?: number | null;
  apps?: number | null;
  alerts?: number | null;
  lastSeen?: number | null;
}

export interface FleetFilters {
  query: string;
  status: string;
  tags: string[];
}

const METRIC_KEY: Partial<
  Record<FleetSortKey, "cpu" | "memory" | "disk" | "apps" | "alerts" | "lastSeen">
> = {
  cpu: "cpu",
  memory: "memory",
  disk: "disk",
  apps: "apps",
  alerts: "alerts",
  lastSeen: "lastSeen",
};

export function fleetSearchText(row: FleetViewRow) {
  return `${row.name} ${row.host} ${row.username} ${row.labels.join(" ")}`.toLowerCase();
}

export function rowMatchesFilters(row: FleetViewRow, filters: FleetFilters) {
  const query = filters.query.trim().toLowerCase();
  const statusOk = filters.status === "all" || row.status === filters.status;
  const tagsOk =
    filters.tags.length === 0 ||
    filters.tags.some((tag) => row.labels.includes(tag));
  return statusOk && tagsOk && fleetSearchText(row).includes(query);
}

/** Empty selection means every tag, matching the previous "All tags" value. */
export function tagsFromStored(value: unknown): string[] {
  if (Array.isArray(value) && value.every((item) => typeof item === "string"))
    return value;
  if (typeof value === "string" && value !== "all" && value !== "") return [value];
  return [];
}

export function activeFilterCount(filters: FleetFilters) {
  return (
    (filters.query.trim() ? 1 : 0) +
    (filters.status === "all" ? 0 : 1) +
    filters.tags.length
  );
}

export function statusCounts(rows: FleetViewRow[]) {
  const counts: Record<ConnectionStatus, number> = {
    disconnected: 0,
    connecting: 0,
    connected: 0,
    degraded: 0,
    offline: 0,
  };
  for (const row of rows) counts[row.status] += 1;
  return counts;
}

function metricValue(row: FleetViewRow, key: FleetSortKey) {
  if (key === "status") return FLEET_STATUS_RANK[row.status];
  const field = METRIC_KEY[key];
  return field ? row[field] : null;
}

export function compareFleetRows(
  a: FleetViewRow,
  b: FleetViewRow,
  sortKey: FleetSortKey,
  ascending: boolean,
) {
  if (sortKey === "name")
    return (ascending ? 1 : -1) * a.name.localeCompare(b.name);
  const first = metricValue(a, sortKey);
  const second = metricValue(b, sortKey);
  if (first == null || second == null)
    return first == null && second == null
      ? a.name.localeCompare(b.name)
      : first == null
        ? 1
        : -1;
  return (
    (ascending ? 1 : -1) * (first - second) || a.name.localeCompare(b.name)
  );
}

export function filterAndSortFleet<T extends FleetViewRow>(
  rows: T[],
  filters: FleetFilters,
  sortKey: FleetSortKey,
  ascending: boolean,
) {
  return rows
    .filter((row) => rowMatchesFilters(row, filters))
    .sort((a, b) => compareFleetRows(a, b, sortKey, ascending));
}

export function groupFleetRows<T extends FleetViewRow>(rows: T[], group: boolean) {
  if (!group) return [{ label: "", rows }];
  const labels = [...new Set(rows.map((row) => row.labels[0] ?? "Untagged"))].sort();
  return labels.map((label) => ({
    label,
    rows: rows.filter((row) => (row.labels[0] ?? "Untagged") === label),
  }));
}

/** Column-header click keeps the previous default: alerts start high-to-low. */
export function toggleFleetSort(
  sortKey: FleetSortKey,
  ascending: boolean,
  next: FleetSortKey,
) {
  if (sortKey === next) return { sortKey, ascending: !ascending };
  return { sortKey: next, ascending: next !== "alerts" };
}

export function seenLabel(timestamp: number | null | undefined, now = Date.now()) {
  if (!timestamp) return "Never";
  const age = Math.max(0, now - timestamp);
  if (age < 15_000) return "Just now";
  if (age < 60_000) return `${Math.floor(age / 1000)}s ago`;
  if (age < 3_600_000) return `${Math.floor(age / 60_000)}m ago`;
  if (age < 86_400_000) return `${Math.floor(age / 3_600_000)}h ago`;
  return new Date(timestamp).toLocaleDateString();
}
