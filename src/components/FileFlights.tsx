import {
  memo,
  useEffect,
  useLayoutEffect,
  useRef,
  type CSSProperties,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import { toKeyframe, type IconPath } from "../lib/fileFlight";
import { endFlight, useFlightStore, type Flight } from "../state/fileFlight";
import { FileIcon } from "./FileIcon";

/** The flying icons' size in pixels; the file lists' icons are scaled up to it. */
const ICON_SIZE = 24;
const BADGE_SIZE = 24;
const RING_MILLIS = 520;
const FLASH_MILLIS = 700;

/** Piles of file icons in flight between panes, above everything else in the window. */
export function FileFlights() {
  const flights = useFlightStore((state) => state.flights);
  if (flights.length === 0) return null;
  return createPortal(
    <FlightLayer>
      {flights.map((flight) => (
        <FlightView key={flight.id} flight={flight} />
      ))}
    </FlightLayer>,
    document.body,
  );
}

/**
 * Open dialogs sit in the browser's top layer, above any z-index, so the layer joins it as a
 * popover to fly over them too, as the demo in Settings does.
 */
function FlightLayer({ children }: { children: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const layer = ref.current;
    if (!layer || typeof layer.showPopover !== "function") return;
    layer.showPopover();
    return () => {
      if (layer.matches(":popover-open")) layer.hidePopover();
    };
  }, []);
  return (
    <div ref={ref} className="file-flights" popover="manual" aria-hidden>
      {children}
    </div>
  );
}

const FlightView = memo(function FlightView({ flight }: { flight: Flight }) {
  useEffect(() => {
    const timer = window.setTimeout(() => endFlight(flight.id), flight.total + 40);
    return () => window.clearTimeout(timer);
  }, [flight.id, flight.total]);

  const { landing, target, firstLanding } = flight;
  return (
    <>
      {target && (
        <Animated
          className="flight-flash"
          style={{
            left: target.left,
            top: target.top,
            width: target.width,
            height: target.height,
          }}
          keyframes={[{ opacity: 0 }, { opacity: 1, offset: 0.25 }, { opacity: 0 }]}
          timing={{ delay: firstLanding, duration: FLASH_MILLIS, easing: "ease-out" }}
        />
      )}
      <Animated
        className="flight-ring"
        style={{ left: landing.x, top: landing.y }}
        keyframes={[
          { opacity: 0, transform: "translate(-50%, -50%) scale(0.3)" },
          { opacity: 0.7, transform: "translate(-50%, -50%) scale(0.45)", offset: 0.1 },
          { opacity: 0, transform: "translate(-50%, -50%) scale(1.8)" },
        ]}
        timing={{
          delay: firstLanding,
          duration: RING_MILLIS,
          easing: "cubic-bezier(0.2, 0, 0, 1)",
        }}
      />
      {flight.icons.map((icon, index) => (
        <FlyingPath key={index} className="flight-icon" path={icon.path} size={ICON_SIZE}>
          <FileIcon entry={icon.subject} />
        </FlyingPath>
      ))}
      {flight.badge && (
        <FlyingPath className="flight-badge" path={flight.badge.path} size={BADGE_SIZE}>
          {flight.badge.text}
        </FlyingPath>
      )}
    </>
  );
});

function FlyingPath({
  className,
  path,
  size,
  children,
}: {
  className: string;
  path: IconPath;
  size: number;
  children: ReactNode;
}) {
  const keyframes = path.frames.map((frame) => toKeyframe(frame, size));
  return (
    <Animated
      className={className}
      keyframes={keyframes}
      timing={{ delay: path.delay, duration: path.duration, easing: "linear" }}
    >
      {children}
    </Animated>
  );
}

interface AnimatedProps {
  className: string;
  style?: CSSProperties;
  keyframes: Keyframe[];
  timing: { delay: number; duration: number; easing: string };
  children?: ReactNode;
}

/** An element that plays its keyframes once, from the moment it mounts. */
function Animated({ className, style, keyframes, timing, children }: AnimatedProps) {
  const ref = useRef<HTMLDivElement>(null);
  const played = useRef({ keyframes, timing });
  useLayoutEffect(() => {
    const element = ref.current;
    if (!element) return;
    const animation = element.animate(played.current.keyframes, {
      ...played.current.timing,
      fill: "both",
    });
    return () => animation.cancel();
  }, []);
  return (
    <div ref={ref} className={className} style={{ ...style, opacity: 0 }}>
      {children}
    </div>
  );
}
