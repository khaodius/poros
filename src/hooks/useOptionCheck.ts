import { useEffect, useState } from "react";
import { sync, toAppError } from "../lib/ipc";
import type { OptionCheck, SyncDirection, SyncOptions } from "../lib/types";

/** How long typing pauses before the options are checked. */
const CHECK_DELAY_MS = 250;

/**
 * The options with each layer of arguments applied, checked by the backend after typing
 * pauses. `null` until the first answer.
 */
export function useOptionCheck(
  options: SyncOptions,
  layers: string[],
  direction: SyncDirection,
): OptionCheck | null {
  const [check, setCheck] = useState<OptionCheck | null>(null);
  const key = JSON.stringify([options, layers, direction]);
  useEffect(() => {
    let current = true;
    const [checkedOptions, checkedLayers, checkedDirection] = JSON.parse(key) as [
      SyncOptions,
      string[],
      SyncDirection,
    ];
    const timer = window.setTimeout(() => {
      sync
        .checkOptions(checkedOptions, checkedLayers, checkedDirection)
        .then((result) => {
          if (current) setCheck(result);
        })
        .catch((caught) => {
          if (current) {
            setCheck({
              options: checkedOptions,
              arguments: "",
              warnings: [],
              errors: [toAppError(caught).message],
            });
          }
        });
    }, CHECK_DELAY_MS);
    return () => {
      current = false;
      window.clearTimeout(timer);
    };
  }, [key]);
  return check;
}
