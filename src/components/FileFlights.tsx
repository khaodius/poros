import {
  memo,
  useEffect,
  useLayoutEffect,
  useRef,
  type CSSProperties,
  type ReactNode,
  type RefObject,
} from "react";
import { createPortal } from "react-dom";
import { toKeyframe, type IconPath } from "../lib/fileFlight";
import {
  endFlight,
  FLIGHT_ICON_LOOKUP_SIZE,
  FLIGHT_ICON_SIZE,
  revealRows,
  useFlightStore,
  type Flight,
} from "../state/fileFlight";
import { FileIcon } from "./FileIcon";

const BADGE_SIZE = 24;
const RING_MILLIS = 520;
const FLASH_MILLIS = 700;
/** How quickly a flight glides after the row it lands in when that row moves. */
const FOLLOW_MILLIS = 50;

/** File icons in flight between panes, above everything else in the window. */
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

  // Timed from the commit that starts the icons' animations, so the rows show as they land.
  useLayoutEffect(() => {
    if (flight.reveals.length === 0) return;
    const timer = window.setTimeout(() => revealRows(flight.reveals), flight.reveal);
    return () => window.clearTimeout(timer);
  }, [flight.reveals, flight.reveal]);

  const track = useRef<HTMLDivElement>(null);
  useFollowRow(flight, track);

  const { landing, target, firstLanding } = flight;
  return (
    <div ref={track} className="flight-track">
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
      {flight.ring && (
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
      )}
      {flight.icons.map((icon, index) => (
        <FlyingPath key={index} className="flight-icon" path={icon.path} size={FLIGHT_ICON_SIZE}>
          <FileIcon entry={icon.subject} size={FLIGHT_ICON_LOOKUP_SIZE} />
        </FlyingPath>
      ))}
      {flight.badge && (
        <FlyingPath className="flight-badge" path={flight.badge.path} size={BADGE_SIZE}>
          {flight.badge.text}
        </FlyingPath>
      )}
    </div>
  );
});

/**
 * Keeps a flight on the row it lands in should that row move on the way, as it does when a
 * reload lists rows above it or the list scrolls: the icons bend toward where the row is now,
 * the more the nearer they are to landing, and glide there rather than jump.
 */
function useFollowRow(flight: Flight, track: RefObject<HTMLDivElement | null>): void {
  useLayoutEffect(() => {
    const row = flight.follow;
    const element = track.current;
    if (!row || !element) return;
    const started = performance.now();
    let previous = started;
    let shown = { x: 0, y: 0 };
    let frame = 0;
    const step = () => {
      const now = performance.now();
      if (row.isConnected) {
        const icon = (row.querySelector(".file-icon") ?? row).getBoundingClientRect();
        const through = Math.min(1, (now - started) / Math.max(1, flight.firstLanding));
        // Eases in and out, so the icons have caught up with the row by the time they land.
        const reach = through * through * (3 - 2 * through);
        const wanted = {
          x: (icon.left + icon.width / 2 - flight.landing.x) * reach,
          y: (icon.top + icon.height / 2 - flight.landing.y) * reach,
        };
        const blend = 1 - Math.exp(-(now - previous) / FOLLOW_MILLIS);
        const next = {
          x: shown.x + (wanted.x - shown.x) * blend,
          y: shown.y + (wanted.y - shown.y) * blend,
        };
        if (Math.hypot(next.x - shown.x, next.y - shown.y) > 0.05) {
          element.style.transform = `translate(${next.x.toFixed(2)}px, ${next.y.toFixed(2)}px)`;
        }
        shown = next;
      }
      previous = now;
      if (now - started < flight.total) frame = window.requestAnimationFrame(step);
    };
    frame = window.requestAnimationFrame(step);
    return () => window.cancelAnimationFrame(frame);
  }, [flight, track]);
}

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
