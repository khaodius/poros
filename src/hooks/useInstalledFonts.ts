import { useEffect, useState } from "react";
import { canvasMeasure, usableFonts, type FontOption } from "../lib/fonts";
import { fonts } from "../lib/ipc";

let reading: Promise<FontOption[]> | null = null;

/** The fonts this computer has, read once per window; null until they are known. */
export function useInstalledFonts(): FontOption[] | null {
  const [installed, setInstalled] = useState<FontOption[] | null>(null);
  useEffect(() => {
    let current = true;
    reading ??= fonts
      .list()
      .then((families) => usableFonts(families, canvasMeasure()))
      .catch(() => {
        reading = null;
        return [];
      });
    void reading.then((options) => {
      if (current) setInstalled(options);
    });
    return () => {
      current = false;
    };
  }, []);
  return installed;
}
