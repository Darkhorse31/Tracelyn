export interface PrivacyServer {
  host: string;
  username: string;
  port: number;
  keyPath?: string | null;
}

export interface PrivacyCatalog {
  servers: PrivacyServer[];
}

const EMPTY: PrivacyCatalog = { servers: [] };

const IPV4 =
  /\b(?:(?:25[0-5]|2[0-4]\d|1?\d?\d)\.){3}(?:25[0-5]|2[0-4]\d|1?\d?\d)\b/g;
const IPV6_CANDIDATE = /\b(?:[0-9a-f]{0,4}:){2,7}[0-9a-f]{0,4}\b/gi;

export function clientLabel(value: string): string {
  let hash = 2166136261;
  for (let i = 0; i < value.length; i += 1) {
    hash ^= value.charCodeAt(i);
    hash = Math.imul(hash, 16777619);
  }
  return `client-${(hash >>> 0).toString(16).padStart(3, "0").slice(0, 3)}`;
}

function serverLabel(index: number): string {
  return `server-${index + 1}`;
}

function isIpv6(token: string): boolean {
  if (token.includes("::")) return true;
  if (/[a-f]/i.test(token)) return true;
  return token.split(":").length >= 6;
}

function hostLabel(host: string, catalog: PrivacyCatalog): string {
  if (/^server-\d+$/.test(host)) return host;
  const index = catalog.servers.findIndex((server) => server.host === host);
  if (index >= 0) return serverLabel(index);
  return clientLabel(host);
}

export type MaskKind = "host" | "user" | "port" | "path" | "secret" | "text";

export function maskField(value: string, catalog: PrivacyCatalog, kind: MaskKind): string {
  if (!value) return value;
  if (kind === "port") return "••";
  if (kind === "user") return "user";
  if (kind === "path") return "key-file";
  if (kind === "secret") return "•••";
  if (kind === "host") return hostLabel(value, catalog);
  return maskSensitive(value, catalog);
}

/** Render-time mask. Stored data is never changed. */
export function maskSensitive(text: string, catalog: PrivacyCatalog = EMPTY): string {
  let out = text;
  for (const server of catalog.servers) {
    if (server.keyPath) out = out.split(server.keyPath).join("key-file");
  }
  out = out.replace(
    /(?:[A-Za-z]:\\(?:[^\\\s]+\\)*\.ssh\\[^\s]+|\/(?:home|root|Users)\/[^/\s]+\/\.ssh\/[^\s]+)/g,
    "key-file",
  );
  out = out.replace(
    /\b((?:rediss?|postgres(?:ql)?|mysql|mongodb(?:\+srv)?)):\/\/[^\s'"]+/gi,
    "$1://•••",
  );
  out = out.replace(/([a-z][a-z0-9+.-]*:\/\/)[^/\s:@]+:[^/\s@]+@/gi, "$1•••:•••@");
  out = out.replace(/Bearer\s+[A-Za-z0-9._~+/-]+=*/gi, "Bearer •••");
  out = out.replace(
    /(api[_-]?key|access[_-]?token|password|passwd|secret|webhook(?:[_-]?url)?|token)(\s*[:=]\s*["']?)([^\s"',}]+)/gi,
    "$1$2•••",
  );
  out = out.replace(
    /\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b/gi,
    (email) => clientLabel(email).replace("client-", "email-"),
  );
  for (let i = 0; i < catalog.servers.length; i += 1) {
    const host = catalog.servers[i].host;
    if (host && !IPV4.test(host)) out = out.split(host).join(serverLabel(i));
    IPV4.lastIndex = 0;
  }
  out = out.replace(
    /\b([A-Za-z0-9._-]{1,64})@([A-Za-z0-9._-]+)(?::\d{1,5})?\b/g,
    (_match, _user, host: string) => `user@${hostLabel(host, catalog)}`,
  );
  out = out.replace(IPV4, (ip) => hostLabel(ip, catalog));
  out = out.replace(IPV6_CANDIDATE, (ip) => (isIpv6(ip) ? hostLabel(ip, catalog) : ip));
  out = out.replace(/((?:server-\d+|client-[0-9a-f]{3}|email-[0-9a-f]{3})):\d{1,5}\b/g, "$1");
  return out;
}
