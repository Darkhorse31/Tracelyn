import { Component, type ErrorInfo, type ReactNode } from "react";
import { Button } from "@/components/ui/button";

type Props = {
  children: ReactNode;
  title?: string;
};

type State = {
  error: Error | null;
};

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error(error, info.componentStack);
  }

  render() {
    if (!this.state.error) return this.props.children;
    const details = `${this.state.error.name}: ${this.state.error.message}\n${this.state.error.stack ?? ""}`;
    return (
      <div role="alert" className="mx-auto flex w-full max-w-lg flex-col gap-4 rounded-xl border border-border bg-card p-6">
        <div>
          <h2 className="text-base font-semibold">{this.props.title ?? "This page hit an unexpected error"}</h2>
          <p className="mt-1 text-sm text-muted-foreground">
            The rest of Tracelyn is still open. Reload this view, or copy the details if you want to keep them on this PC.
          </p>
        </div>
        <p className="rounded-lg bg-muted px-3 py-2 font-mono text-xs break-all text-foreground">
          {this.state.error.message || "Unknown error"}
        </p>
        <div className="flex flex-wrap gap-2">
          <Button type="button" onClick={() => this.setState({ error: null })}>
            Reload
          </Button>
          <Button
            type="button"
            variant="outline"
            onClick={() => {
              void navigator.clipboard.writeText(details);
            }}
          >
            Copy error details
          </Button>
        </div>
      </div>
    );
  }
}
