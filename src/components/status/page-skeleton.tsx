import { Skeleton } from "@/components/ui/skeleton";

export function PageSkeleton({ rows = 4 }: { rows?: number }) {
  return (
    <div
      role="status"
      aria-label="Loading content"
      className="flex w-full flex-col gap-4"
    >
      <span className="sr-only">Loading…</span>
      <Skeleton className="h-6 w-48" />
      {Array.from({ length: rows }, (_, i) => (
        <Skeleton key={i} className="h-20 w-full rounded-xl" />
      ))}
    </div>
  );
}
