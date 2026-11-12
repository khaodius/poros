import { useRef, useState } from "react";
import { Play, RotateCcw } from "lucide-react";
import { boxCenter, type Box } from "../lib/fileFlight";
import {
  DEFAULT_PROGRESS_BARS,
  PROGRESS_HEIGHT_LIMITS,
  progressLabel,
  type ProgressBarSettings,
  type ProgressColoring,
  type ProgressFill,
  type ProgressLabel,
  type ProgressMotion,
} from "../lib/progressBars";
import type { IconSubject } from "../lib/systemIcons";
import type { Direction, JobState } from "../lib/types";
import { playFlight, prefersLessMotion } from "../state/fileFlight";
import {
  previewSettingsSection,
  saveSettingsSection,
  useSettingsStore,
} from "../state/settingsStore";
import { ColorInput } from "./ColorInput";
import { FileIcon } from "./FileIcon";
import { ProgressBar } from "./ProgressBar";
import {
  RangeSetting,
  SelectSetting,
  SettingGroup,
  SettingRow,
  SwitchSetting,
} from "./settingsFields";

const FILLS: { value: ProgressFill; label: string }[] = [
  { value: "solid", label: "Solid" },
  { value: "gradient", label: "Gradient" },
  { value: "striped", label: "Striped" },
  { value: "segmented", label: "Segmented" },
  { value: "glow", label: "Glowing" },
];

const COLORINGS: { value: ProgressColoring; label: string }[] = [
  { value: "theme", label: "The theme's" },
  { value: "direction", label: "By direction" },
  { value: "custom", label: "Custom" },
];

const COLORING_HINTS: Record<ProgressColoring, string> = {
  theme: "The theme's Progress bars color, which Colors > All colors changes.",
  direction: "Uploads take the theme's Uploads color, downloads its Downloads color.",
  custom: "The same color for every bar.",
};

const MOTIONS: { value: ProgressMotion; label: string }[] = [
  { value: "none", label: "Nothing" },
  { value: "stripes", label: "Moving stripes" },
  { value: "shimmer", label: "Shimmer" },
  { value: "pulse", label: "Pulse" },
];

const LABELS: { value: ProgressLabel; label: string }[] = [
  { value: "percent", label: "Percent" },
  { value: "bytes", label: "Transferred and size" },
  { value: "remaining", label: "Time left" },
  { value: "none", label: "No text" },
];

const MIB = 1024 * 1024;

interface SampleTransfer {
  name: string;
  direction: Direction;
  state: JobState;
  size: number;
  fraction: number;
  speed: number;
}

const SAMPLES: SampleTransfer[] = [
  {
    name: "backup-2026-11.tar.gz",
    direction: "upload",
    state: "running",
    size: 700 * MIB,
    fraction: 0.64,
    speed: 22 * MIB,
  },
  {
    name: "photos.zip",
    direction: "download",
    state: "running",
    size: 1800 * MIB,
    fraction: 0.28,
    speed: 40 * MIB,
  },
  {
    name: "report.pdf",
    direction: "upload",
    state: "paused",
    size: 9 * MIB,
    fraction: 0.4,
    speed: 0,
  },
  {
    name: "notes.md",
    direction: "download",
    state: "done",
    size: 12 * 1024,
    fraction: 1,
    speed: 0,
  },
];

const pixels = (value: number) => `${value} px`;

function sameBars(left: ProgressBarSettings, right: ProgressBarSettings): boolean {
  return (Object.keys(left) as (keyof ProgressBarSettings)[]).every(
    (key) => left[key] === right[key],
  );
}

/** Bars drawn as the transfer list draws them, with the settings being edited. */
function ProgressPreview({ bars }: { bars: ProgressBarSettings }) {
  return (
    <div className="progress-preview" aria-label="Preview">
      {SAMPLES.map((sample) => {
        const job = {
          state: sample.state,
          size: sample.size,
          transferred: sample.size * sample.fraction,
          speed: sample.speed,
        };
        return (
          <div
            key={sample.name}
            className={`progress-sample state-${sample.state}`}
            data-direction={sample.direction}
          >
            <span className="progress-sample-name">
              <FileIcon entry={{ name: sample.name, kind: "file" }} />
              {sample.name}
            </span>
            <span className="progress-sample-bar">
              <ProgressBar
                fraction={sample.fraction}
                label={progressLabel(job, bars.label, sample.fraction)}
                labelKind={bars.label}
                labelInside={bars.labelInside}
                still={sample.state !== "running"}
              />
            </span>
          </div>
        );
      })}
    </div>
  );
}

export function ProgressBarGroup() {
  const bars = useSettingsStore((state) => state.settings.appearance.progressBars);
  const latest = () => useSettingsStore.getState().settings.appearance.progressBars;
  const save = (change: Partial<ProgressBarSettings>) =>
    void saveSettingsSection("appearance", { progressBars: { ...latest(), ...change } });
  const preview = (change: Partial<ProgressBarSettings>) =>
    previewSettingsSection("appearance", { progressBars: { ...latest(), ...change } });

  return (
    <SettingGroup title="Progress bars">
      <ProgressPreview bars={bars} />
      <SelectSetting
        label="Fill"
        hint="How the filled part of every progress bar is drawn."
        value={bars.fill}
        options={FILLS}
        onChange={(fill) => save({ fill })}
      />
      <SettingRow label="Bar color" hint={COLORING_HINTS[bars.coloring]}>
        <span className="progress-color">
          {bars.coloring === "custom" && (
            <ColorInput
              className="swatch-picker"
              aria-label="Custom bar color"
              value={bars.color}
              onPreview={(color) => preview({ color })}
              onCommit={(color) => save({ color })}
            />
          )}
          <select
            value={bars.coloring}
            aria-label="Bar color"
            onChange={(event) => save({ coloring: event.target.value as ProgressColoring })}
          >
            {COLORINGS.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </span>
      </SettingRow>
      <SwitchSetting
        label="Color finished, paused and failed transfers"
        hint="Green, gray and red from the theme, whatever the color above."
        checked={bars.stateColors}
        onChange={(stateColors) => save({ stateColors })}
      />
      <RangeSetting
        label="Thickness"
        hint="Of the bars in the transfer list."
        value={bars.height}
        min={PROGRESS_HEIGHT_LIMITS.min}
        max={PROGRESS_HEIGHT_LIMITS.max}
        format={pixels}
        onPreview={(height) => preview({ height })}
        onCommit={(height) => save({ height })}
      />
      <SwitchSetting
        label="Rounded ends"
        checked={bars.rounded}
        onChange={(rounded) => save({ rounded })}
      />
      <SelectSetting
        label="While running"
        hint="Plays on bars whose work is under way, and holds still when the system asks for less motion."
        value={bars.motion}
        options={MOTIONS}
        onChange={(motion) => save({ motion })}
      />
      <SelectSetting
        label="Transfer text"
        hint="What each transfer shows beside its bar."
        value={bars.label}
        options={LABELS}
        onChange={(label) => save({ label })}
      />
      <SwitchSetting
        label="Text on the bar"
        hint="The bar grows to fit it."
        checked={bars.labelInside}
        disabled={bars.label === "none"}
        onChange={(labelInside) => save({ labelInside })}
      />
      <div className="setting-actions">
        <button
          type="button"
          className="button"
          disabled={sameBars(bars, DEFAULT_PROGRESS_BARS)}
          onClick={() => save(DEFAULT_PROGRESS_BARS)}
        >
          <RotateCcw size={14} />
          Reset progress bars
        </button>
      </div>
    </SettingGroup>
  );
}

const LOCAL_DEMO_FILES: IconSubject[] = [
  { name: "photo.jpg", kind: "file" },
  { name: "notes.md", kind: "file" },
  { name: "song.flac", kind: "file" },
  { name: "deploy.sh", kind: "file" },
  { name: "site.zip", kind: "file" },
  { name: "holiday.png", kind: "file" },
  { name: "Projects", kind: "dir" },
];

const SERVER_DEMO_FILES: IconSubject[] = [
  { name: "clip.mp4", kind: "file" },
  { name: "data.csv", kind: "file" },
  { name: "id_ed25519.pub", kind: "file" },
  { name: "index.ts", kind: "file" },
  { name: "report.pdf", kind: "file" },
  { name: "access.log", kind: "file" },
];

function boxOf(element: Element): Box {
  const { left, top, width, height } = element.getBoundingClientRect();
  return { left, top, width, height };
}

export function AnimationGroup() {
  const transferAnimation = useSettingsStore(
    (state) => state.settings.appearance.transferAnimation,
  );
  const stageRef = useRef<HTMLDivElement>(null);
  const [toServer, setToServer] = useState(true);
  const lessMotion = prefersLessMotion();

  const play = () => {
    const stage = stageRef.current;
    if (!stage) return;
    const [from, to] = [
      stage.querySelector(toServer ? ".flight-demo-local" : ".flight-demo-server"),
      stage.querySelector(toServer ? ".flight-demo-server" : ".flight-demo-local"),
    ];
    if (!from || !to) return;
    const icons = [...from.querySelectorAll(".file-icon")].map((icon) => boxCenter(boxOf(icon)));
    const target = boxOf(to);
    const files = toServer ? LOCAL_DEMO_FILES : SERVER_DEMO_FILES;
    playFlight(files, icons, boxCenter(target, 0.55), target);
    setToServer(!toServer);
  };

  return (
    <SettingGroup title="Animation">
      <SwitchSetting
        label="Flying files"
        hint="When you send, move or copy files, a pile of their icons hops out of one pane and lands in the other."
        checked={transferAnimation}
        onChange={(checked) =>
          void saveSettingsSection("appearance", { transferAnimation: checked })
        }
      />
      <div className="flight-demo" ref={stageRef}>
        <div className="flight-demo-pane flight-demo-local">
          <span className="flight-demo-title">This computer</span>
          <span className="flight-demo-files">
            {LOCAL_DEMO_FILES.map((file) => (
              <FileIcon key={file.name} entry={file} />
            ))}
          </span>
        </div>
        <button
          type="button"
          className="button"
          disabled={lessMotion}
          title={lessMotion ? "The system asks for less motion" : undefined}
          onClick={play}
        >
          <Play size={14} />
          {toServer ? "Upload" : "Download"}
        </button>
        <div className="flight-demo-pane flight-demo-server">
          <span className="flight-demo-title">Server</span>
          <span className="flight-demo-files">
            {SERVER_DEMO_FILES.map((file) => (
              <FileIcon key={file.name} entry={file} />
            ))}
          </span>
        </div>
      </div>
      {lessMotion && (
        <p className="setting-note">
          The system asks for less motion, so files do not fly even with this on.
        </p>
      )}
    </SettingGroup>
  );
}
