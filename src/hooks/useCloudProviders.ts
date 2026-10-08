import { useEffect, useState } from "react";
import { cloud } from "../lib/ipc";
import type { CloudProvider, CloudProviderStatus } from "../lib/types";

/** Which cloud providers can be signed in to; `null` until the backend answers. */
export function useCloudProviders(): Record<CloudProvider, CloudProviderStatus> | null {
  const [statuses, setStatuses] = useState<Record<CloudProvider, CloudProviderStatus> | null>(null);
  useEffect(() => {
    let current = true;
    cloud
      .providers()
      .then((list) => {
        if (!current) return;
        const byProvider = Object.fromEntries(list.map((status) => [status.provider, status]));
        setStatuses(byProvider as Record<CloudProvider, CloudProviderStatus>);
      })
      .catch(() => undefined);
    return () => {
      current = false;
    };
  }, []);
  return statuses;
}
