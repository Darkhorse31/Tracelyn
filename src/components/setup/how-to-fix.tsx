import { Link } from "react-router-dom";
import { buttonVariants } from "@/components/ui/button";
import { useServersContext } from "@/contexts/servers-context";
import { guideStepFor } from "@/lib/setup-guide";
import { cn } from "@/lib/utils";

export function HowToFix({ text, className }: { text: string; className?: string }) {
  const { selectedServerId } = useServersContext();
  const step = guideStepFor(text);
  const params = new URLSearchParams();
  if (selectedServerId) params.set("server", selectedServerId);
  if (step) params.set("step", step);
  const query = params.toString();
  return (
    <Link
      to={query ? `/setup?${query}` : "/setup"}
      className={cn(buttonVariants({ variant: "outline", size: "sm" }), className)}
    >
      Set up monitoring
    </Link>
  );
}
