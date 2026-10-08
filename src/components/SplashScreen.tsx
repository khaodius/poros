import { useEffect, useState } from "react";
import { PorosLogo } from "./PorosLogo";

/** Long enough for the dots to light up once, so a fast start does not look like a flicker. */
const MIN_SHOWN_MILLIS = 650;
/** Matches the fade in `.splash.is-leaving`. */
const FADE_MILLIS = 220;

/** Covers the window with the logo while the app loads, then fades out. */
export function SplashScreen({ ready }: { ready: boolean }) {
  const [shownLongEnough, setShownLongEnough] = useState(false);
  const [gone, setGone] = useState(false);
  const leaving = ready && shownLongEnough;

  useEffect(() => {
    const timer = window.setTimeout(() => setShownLongEnough(true), MIN_SHOWN_MILLIS);
    return () => window.clearTimeout(timer);
  }, []);

  useEffect(() => {
    if (!leaving) return;
    const timer = window.setTimeout(() => setGone(true), FADE_MILLIS);
    return () => window.clearTimeout(timer);
  }, [leaving]);

  if (gone) return null;
  return (
    <div
      className={`splash ${leaving ? "is-leaving" : ""}`}
      role="status"
      aria-label="Loading Poros"
      aria-hidden={leaving}
    >
      <PorosLogo size={88} />
      <div className="splash-dots">
        <span />
        <span />
        <span />
      </div>
    </div>
  );
}
