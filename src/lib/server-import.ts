import type { ServerRecord, UpsertServerInput } from "./types.ts";

export interface ImportServer {
  name: string;
  host: string;
  port: number;
  username: string;
  authType: string;
  keyPath: string | null;
  labels: string[];
  proxyJump: string | null;
  issues: string[];
}
export interface ImportPreview {
  servers: ImportServer[];
  warnings: string[];
}

function words(line: string): string[] {
  const out: string[] = [];
  let word = "",
    quote = "";
  for (let i = 0; i < line.length; i++) {
    const c = line[i],
      next = line[i + 1];
    if (c === "\\" && next && (next === quote || next === "#" || next === " "))
      word += line[++i];
    else if (quote && c === quote) quote = "";
    else if (!quote && (c === '"' || c === "'")) quote = c;
    else if (!quote && c === "#") break;
    else if (!quote && /\s/.test(c)) {
      if (word) out.push(word);
      word = "";
    } else word += c;
  }
  if (quote) throw new Error("Unclosed quote");
  if (word) out.push(word);
  return out;
}
function hostMatches(patterns: string[], alias: string): boolean {
  let positive = false;
  for (const pattern of patterns) {
    const negative = pattern.startsWith("!");
    const raw = negative ? pattern.slice(1) : pattern;
    const regex = raw
      .replace(/[.+^${}()|[\]\\]/g, "\\$&")
      .replace(/\*/g, ".*")
      .replace(/\?/g, ".");
    const matched = new RegExp(`^${regex}$`, "i").test(alias);
    if (matched && negative) return false;
    positive ||= matched;
  }
  return positive;
}
function parsePort(raw: string, issues: string[]): number {
  const n = Number(raw);
  if (!/^\d+$/.test(raw) || n < 1 || n > 65535) {
    issues.push("Port must be a number from 1 to 65535");
    return 22;
  }
  return n;
}
function validate(row: ImportServer) {
  if (!row.host.trim() || /\s/.test(row.host) || row.host.startsWith("-"))
    row.issues.push("A host name or IP address is required");
  if (!row.username.trim() || /\s/.test(row.username))
    row.issues.push("An SSH username is required");
  if (row.proxyJump && row.proxyJump.toLowerCase() !== "none")
    row.issues.push(
      "ProxyJump is not supported yet. Use a directly reachable host",
    );
  if (
    row.host.includes("%") ||
    row.keyPath?.includes("%") ||
    row.keyPath?.startsWith("~")
  )
    row.issues.push("Resolve SSH path and host tokens before importing");
}
export function parseSshConfig(
  text: string,
  home = "",
  defaultUser = "",
): ImportPreview {
  const result: ImportPreview = { servers: [], warnings: [] };
  const blocks: { patterns: string[]; values: Map<string, string> }[] = [
    { patterns: ["*"], values: new Map() },
  ];
  const aliases = new Set<string>(),
    seen = new Set<string>();
  let inMatch = false;
  text.split(/\r?\n/).forEach((line, index) => {
    const directive = line.trimStart().match(/^([^\s=]+)\s*(?:=\s*)?(.*)$/);
    if (!directive || directive[1].startsWith("#")) return;
    const key = directive[1].toLowerCase();
    let values: string[];
    try {
      values = words(directive[2]);
    } catch {
      result.warnings.push(`Line ${index + 1}: Unclosed quote`);
      return;
    }
    if (key === "host") {
      inMatch = false;
      for (const alias of values)
        if (!/[*?!]/.test(alias) && !seen.has(alias.toLowerCase())) {
          aliases.add(alias);
          seen.add(alias.toLowerCase());
        }
      blocks.push({ patterns: values, values: new Map() });
    } else if (key === "match") {
      inMatch = true;
      result.warnings.push(`Line ${index + 1}: Match rules were skipped`);
    } else if (key === "include")
      result.warnings.push(
        `Line ${index + 1}: Include files were not opened. Paste their Host entries to import them`,
      );
    else if (!inMatch) {
      const current = blocks[blocks.length - 1].values;
      if (!current.has(key)) current.set(key, values.join(" "));
    }
  });
  for (const alias of aliases) {
    const options = new Map<string, string>();
    for (const block of blocks)
      if (hostMatches(block.patterns, alias))
        for (const [k, v] of block.values)
          if (!options.has(k)) options.set(k, v);
    const host = (options.get("hostname") ?? alias).replaceAll("%h", alias);
    const username = options.get("user") ?? defaultUser;
    const issues: string[] = [];
    const port = parsePort(options.get("port") ?? "22", issues);
    let keyPath = options.get("identityfile") ?? null;
    if (keyPath !== null) {
      if (keyPath === "~") keyPath = home;
      else if (keyPath.startsWith("~/"))
        keyPath = `${home}/${keyPath.slice(2)}`;
      keyPath = keyPath
        .replaceAll("%%", "\u0001")
        .replaceAll("%d", home)
        .replaceAll("%h", host)
        .replaceAll("%n", alias)
        .replaceAll("%r", username)
        .replaceAll("%p", String(port))
        .replaceAll("\u0001", "%");
    }
    if (
      options.has("proxycommand") &&
      options.get("proxycommand")?.toLowerCase() !== "none"
    )
      issues.push(
        "ProxyCommand is not supported. Use a directly reachable host",
      );
    const row: ImportServer = {
      name: alias,
      host,
      port,
      username,
      authType: "key",
      keyPath,
      labels: [],
      proxyJump: options.get("proxyjump") ?? null,
      issues,
    };
    validate(row);
    result.servers.push(row);
  }
  return result;
}

function csvRecords(text: string): string[][] {
  const records: string[][] = [],
    record: string[] = [];
  let field = "",
    quoted = false,
    closed = false;
  text = text.replace(/^\uFEFF/, "");
  const finish = () => {
    record.push(field);
    field = "";
    closed = false;
  };
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (quoted) {
      if (c === '"') {
        if (text[i + 1] === '"') {
          i++;
          field += '"';
        } else {
          quoted = false;
          closed = true;
        }
      } else field += c;
    } else if (c === '"' && !field && !closed) quoted = true;
    else if (c === "," || c === "\n" || c === "\r") {
      finish();
      if (c !== ",") {
        if (c === "\r" && text[i + 1] === "\n") i++;
        if (record.some((v) => v.trim())) records.push([...record]);
        record.length = 0;
      }
    } else if (closed || c === '"')
      throw new Error("Malformed CSV. Quotes must surround the whole field");
    else field += c;
  }
  if (quoted) throw new Error("Malformed CSV. A quoted field was not closed");
  finish();
  if (record.some((v) => v.trim())) records.push(record);
  return records;
}
export function parseServerCsv(text: string): ImportPreview {
  const records = csvRecords(text);
  if (!records.length) throw new Error("CSV is empty");
  const headers = records[0].map((h) => h.trim().toLowerCase());
  if (new Set(headers).size !== headers.length)
    throw new Error("CSV has duplicate column names");
  if (
    !headers.includes("host") ||
    !headers.some((h) => h === "username" || h === "user")
  )
    throw new Error("CSV needs host and username columns");
  if (
    headers.some((h) =>
      ["password", "keypassphrase", "privatekey", "secret"].includes(h),
    )
  )
    throw new Error(
      "Remove secret columns before importing. Add credentials in Edit server",
    );
  const known = [
    "name",
    "host",
    "port",
    "username",
    "user",
    "authtype",
    "keypath",
    "labels",
    "proxyjump",
  ];
  const preview: ImportPreview = {
    servers: [],
    warnings: headers
      .filter((h) => !known.includes(h))
      .map((h) => `Unknown column ignored: ${h}`),
  };
  records.slice(1).forEach((cells, i) => {
    const get = (key: string) => cells[headers.indexOf(key)]?.trim() ?? "";
    const issues: string[] = [];
    if (cells.length !== headers.length)
      issues.push(`CSV row ${i + 2} has the wrong number of columns`);
    const port = parsePort(get("port") || "22", issues);
    const authType = get("authtype") || "key";
    if (authType !== "key" && authType !== "password")
      issues.push("Authentication must be key or password");
    let labels: string[] = [];
    if (get("labels").startsWith("[")) {
      try {
        const parsed: unknown = JSON.parse(get("labels"));
        if (
          !Array.isArray(parsed) ||
          !parsed.every((v) => typeof v === "string")
        )
          throw new Error();
        labels = parsed;
      } catch {
        issues.push("Labels must be a JSON list or separated by |");
      }
    } else
      labels = get("labels")
        .split("|")
        .map((s) => s.trim())
        .filter(Boolean);
    const row: ImportServer = {
      name: get("name") || get("host"),
      host: get("host"),
      port,
      username: get("username") || get("user"),
      authType,
      keyPath: get("keypath") || null,
      labels,
      proxyJump: get("proxyjump") || null,
      issues,
    };
    validate(row);
    preview.servers.push(row);
  });
  return preview;
}

export function endpointKey(
  server: Pick<ServerRecord, "host" | "port" | "username">,
): string {
  return JSON.stringify([
    server.host
      .trim()
      .replace(/^\[|\]$/g, "")
      .replace(/\.$/, "")
      .toLowerCase(),
    server.port,
    server.username.trim(),
  ]);
}
export function importDuplicates(
  rows: ImportServer[],
  existing: ServerRecord[],
): Map<number, string> {
  const seen = new Map(existing.map((s) => [endpointKey(s), s.name]));
  const duplicates = new Map<number, string>();
  rows.forEach((row, i) => {
    const key = endpointKey(row),
      previous = seen.get(key);
    if (previous !== undefined) duplicates.set(i, previous);
    else if (!row.issues.length) seen.set(key, row.name);
  });
  return duplicates;
}
export function importInput(
  row: ImportServer,
  fallbackKey?: string,
): UpsertServerInput {
  if (row.issues.length) throw new Error(row.issues.join(". "));
  return {
    name: row.name,
    host: row.host.replace(/^\[|\]$/g, ""),
    port: row.port,
    username: row.username,
    authType: row.authType as "key" | "password",
    keyPath: row.keyPath ?? fallbackKey,
    labels: row.labels,
  };
}

/** Explicit allowlist prevents credentials from being exported even from wider objects. */
export function exportServerCsv(servers: ServerRecord[]): string {
  const quote = (value: string) => `"${value.replaceAll('"', '""')}"`;
  const columns = [
    "name",
    "host",
    "port",
    "username",
    "authType",
    "keyPath",
    "labels",
  ];
  return (
    [
      columns.join(","),
      ...servers.map((s) =>
        [
          s.name,
          s.host,
          String(s.port),
          s.username,
          s.authType,
          s.keyPath ?? "",
          JSON.stringify(s.labels),
        ]
          .map(quote)
          .join(","),
      ),
    ].join("\r\n") + "\r\n"
  );
}
