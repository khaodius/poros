import { Cloud, Server } from "lucide-react";
import { isCloud } from "../lib/protocols";
import type { Protocol } from "../lib/types";

export function ConnectionIcon({
  protocol,
  size,
  className,
}: {
  protocol: Protocol;
  size: number;
  className?: string;
}) {
  const Icon = isCloud(protocol) ? Cloud : Server;
  return <Icon size={size} className={className} />;
}
