import { Link } from "react-router-dom";
import { buttonVariants } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { cn } from "@/lib/utils";

const STEPS = [
  {
    title: "Connect",
    body: "Add a Linux server with its SSH user and a private key or password. Tracelyn stores secrets in Windows Credential Manager and opens SSH from this PC. It does not install an agent.",
  },
  {
    title: "Discover apps",
    body: "After you connect, Tracelyn reads process lists, PM2, systemd, Docker, and web server config. It builds an application list and the log files those apps write.",
  },
  {
    title: "Monitor",
    body: "CPU, memory, disk, web traffic, logs, traces, and infrastructure stay on a read-only SSH session. Charts and the log explorer update while the connection is up.",
  },
  {
    title: "Alerts",
    body: "Rules watch the metrics already collected. When one stays over its threshold, Tracelyn records an incident and can notify a channel you configured.",
  },
  {
    title: "Operations",
    body: "Restart and the terminal stay off until you turn Operations mode on. Those actions need sudo or a user that can already run them, and each one is written to the audit log.",
  },
] as const;

export function HowItWorksPage() {
  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-6">
      <div>
        <h2 className="text-lg font-semibold tracking-tight">How Tracelyn works</h2>
        <p className="mt-1 text-sm text-muted-foreground">
          Tracelyn watches a Linux server over SSH. The checks and the commands you copy never
          run automatically.
        </p>
      </div>
      <ol className="flex flex-col gap-3">
        {STEPS.map((step, index) => (
          <li key={step.title}>
            <Card>
              <CardHeader className="pb-2">
                <CardTitle className="text-sm">
                  <span className="mr-2 font-mono text-muted-foreground">{index + 1}</span>
                  {step.title}
                </CardTitle>
              </CardHeader>
              <CardContent>
                <p className="text-sm text-muted-foreground">{step.body}</p>
              </CardContent>
            </Card>
          </li>
        ))}
      </ol>
      <div className="flex flex-wrap gap-2">
        <Link to="/setup" className={cn(buttonVariants())}>
          Setup guide
        </Link>
        <Link to="/" className={cn(buttonVariants({ variant: "outline" }))}>
          Overview
        </Link>
      </div>
    </div>
  );
}
