export type OpsKind = "pm2" | "systemd" | "docker" | "nginx" | "apache" | "process" | "companion";
export type OpsVerb = "start" | "stop" | "restart" | "reload" | "install" | "update" | "uninstall" | "sync" | "configure";

export interface ActionRequest {
  serverId: string;
  kind: OpsKind;
  verb: OpsVerb;
  target: string;
  pm2User?: string | null;
  pm2Home?: string | null;
  logPaths?: string[];
  useSudo: boolean;
  confirmName?: string | null;
  healthTimeoutSecs?: number;
}

export interface ActionPreview {
  command: string;
  dangerous: boolean;
  confirmName: string;
  pm2User?: string | null;
  pm2Home?: string | null;
  useSudo: boolean;
}

export interface ActionOutcome {
  auditId: string;
  command: string;
  exitCode: number;
  outputExcerpt: string;
  healthOk?: boolean | null;
  healthMessage: string;
}

export interface AuditRow {
  id: string;
  actor: string;
  serverId: string;
  serverName: string;
  target: string;
  command: string;
  tsMs: number;
  exitCode?: number | null;
  outputExcerpt: string;
}

export interface AuditFilter {
  serverId?: string;
  target?: string;
  sinceMs?: number;
  untilMs?: number;
  failedOnly?: boolean;
  limit?: number;
}

export interface TermEvent {
  dataB64: string;
  closed: boolean;
  error?: string | null;
}

export const OPS_MODE_EVENT = "tracelyn-ops-mode";

export function defaultVerbs(kind: OpsKind): OpsVerb[] {
  if (kind === "process") return ["stop"];
  if (kind === "nginx" || kind === "apache") return ["reload"];
  if (kind === "docker") return ["start", "stop", "restart"];
  return ["start", "stop", "restart", "reload"];
}

export function defaultSudo(kind: OpsKind): boolean {
  return kind !== "pm2";
}
