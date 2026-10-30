export function healthScore(connected: boolean, issues: {severity: string}[]): number {
  if (!connected) return 0;
  return Math.max(0, 100 - issues.reduce((sum, issue) => sum + (issue.severity === "critical" ? 40 : issue.severity === "warning" ? 15 : 5), 0));
}

export const issueLabels: Record<string,string> = {
  server_unreachable: "Server connection lost", cpu_high: "High processor use", memory_high: "High memory use",
  disk_high: "Storage nearly full", disk_forecast: "Storage may run out", app_down: "Application stopped",
  app_crash: "Application stopped", service_stopped: "Service stopped", restart_loop: "Repeated restarts",
  ssl_expiry: "Certificate needs renewal", uptime: "Website unavailable", oom: "Process stopped to free memory",
  error_rate: "More errors than usual", http_5xx_rate: "Website errors", trace_error_rate: "Application errors",
  log_query: "Errors found in logs", log_pattern: "Errors found in logs", unknown: "Other issue",
};
export function nextStep(kind: string): string {
  switch (kind) {
    case "server_unreachable": return "Check your internet connection and the server status in your hosting account.";
    case "cpu_high": return "Open Processes to find the busiest application.";
    case "disk_high": case "disk_forecast": return "Review storage use and remove unneeded files or increase disk space.";
    case "memory_high": case "oom": return "Review application memory use and recent logs.";
    case "ssl_expiry": return "Renew the website certificate and check that the website opens securely.";
    case "app_down": case "app_crash": case "service_stopped": case "restart_loop": return "Open Apps and review the affected application and its recent logs.";
    default: return "Open the incident and review its timeline and suggested next steps.";
  }
}
