/** Maps an on-screen error to a Setup Guide check id. */
export function guideStepFor(text: string): string | null {
  const t = text.toLowerCase();
  if (t.includes("journal")) return "journald";
  if (t.includes("pm2")) return "pm2";
  if (t.includes("docker")) return "docker";
  if (t.includes("opentelemetry") || t.includes("otel")) return "otel";
  if (t.includes("redis")) return "redis";
  if (t.includes("sudo") || t.includes("nopasswd") || t.includes("operations mode")) return "sudo";
  if (t.includes("apache")) return "apache-logs";
  if (t.includes("rabbit")) return "rabbitmq";
  if (t.includes("mongo")) return "mongo";
  if (t.includes("postgres") || t.includes("postgresql")) return "postgres";
  if (t.includes("mysql") || t.includes("mariadb")) return "mysql";
  if (
    t.includes("web-access") ||
    t.includes("access log") ||
    t.includes("nginx") ||
    t.includes(" adm")
  ) {
    return "nginx-logs";
  }
  if (t.includes("not readable") || t.includes("permission denied") || t.includes("cannot read")) {
    return "app-logs";
  }
  return null;
}
