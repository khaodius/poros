import type { Protocol } from "../lib/types";
import { CONNECTION_ICONS, connectionKind } from "./tabIcons";

export function ConnectionIcon({
  protocol,
  size,
  className,
}: {
  protocol: Protocol;
  size: number;
  className?: string;
}) {
  const Icon = CONNECTION_ICONS[connectionKind(protocol)];
  return <Icon size={size} className={className} />;
}
