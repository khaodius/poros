import { useRef, useState, type CSSProperties } from "react";
import { Play, RotateCcw } from "lucide-react";
import { boxCenter, type Box } from "../lib/fileFlight";
import {
  DEFAULT_PROGRESS_BARS,
  PROGRESS_HEIGHT_LIMITS,
  PROGRESS_PRESETS,
  fillTakesShape,
  matchingPreset,
  progressBarStyle,
  progressLabel,
  type ProgressBarSettings,
  type ProgressColoring,
  type ProgressFill,
  type ProgressLabel,
  type ProgressMotion,
  type ProgressShape,
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

interface Choice<T extends string> {
  value: T;
  label: string;
}

const FILLS: (Choice<ProgressFill> & { hint: string })[] = [
  { value: "solid", label: "Solid", hint: "One flat color." },
  { value: "gradient", label: "Gradient", hint: "Fades in from the start of the bar." },
  { value: "striped", label: "Striped", hint: "Faint diagonal stripes." },
  { value: "segmented", label: "Segmented", hint: "Cut into short pieces." },
  { value: "glow", label: "Glowing", hint: "Lit from above, with a glow around it." },
  { value: "candy", label: "Candy", hint: "Wide stripes in two tones." },
  { value: "chevrons", label: "Chevrons", hint: "Arrowheads that point the way the bar grows." },
  { value: "aurora", label: "Aurora", hint: "The color drifts to the hues beside it." },
  { value: "neon", label: "Neon", hint: "An outlined tube with a faint light inside." },
  {
    value: "wave",
    label: "Wave",
    hint: "A liquid. Its surface ripples on bars 9 px and thicker.",
  },
  { value: "line", label: "Line", hint: "A thin line led by a round head." },
  { value: "dots", label: "Dots", hint: "Beads that light up one at a time." },
  { value: "blocks", label: "Blocks", hint: "Cells that light up one at a time." },
];

const COLORINGS: Choice<ProgressColoring>[] = [
  { value: "theme", label: "The theme's" },
  { value: "direction", label: "By direction" },
  { value: "custom", label: "Custom" },
];

const COLORING_HINTS: Record<ProgressColoring, string> = {
  theme: "The theme's Progress bars color, which Colors > All colors changes.",
  direction: "Uploads take the theme's Uploads color, downloads its Downloads color.",
  custom: "The same color for every bar.",
};

const SHAPES: Choice<ProgressShape>[] = [
  { value: "pill", label: "Round" },
  { value: "soft", label: "Soft" },
  { value: "square", label: "Square" },
  { value: "slanted", label: "Slanted" },
];

const MOTIONS: Choice<ProgressMotion>[] = [
  { value: "none", label: "Nothing" },
  { value: "stripes", label: "Moving stripes" },
  { value: "shimmer", label: "Shimmer" },
  { value: "pulse", label: "Pulse" },
  { value: "scan", label: "Scan" },
  { value: "ripple", label: "Ripple" },
  { value: "comet", label: "Comet" },
  { value: "sparkle", label: "Sparkle" },
  { value: "breathe", label: "Breathing glow" },
];

const LABELS: Choice<ProgressLabel>[] = [
  { value: "percent", label: "Percent" },
  { value: "bytes", label: "Transferred and size" },
  { value: "remaining", label: "Time left" },
  { value: "none", label: "No text" },
];

const MIB = 1024 * 1024;

interface SampleTransfer {
  name: string;
  /** The file whose icon the row shows, when the name says more than the file's. */
  file?: string;
  /** None for work that is not a transfer, whose bar takes the plain color. */
  direction?: Direction;
  state: JobState;
  size: number;
  /** Null for work of unknown length. */
  fraction: number | null;
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
    name: "clip.mp4",
    direction: "download",
    state: "failed",
    size: 260 * MIB,
    fraction: 0.55,
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
  {
    name: "Unpacking site.zip",
    file: "site.zip",
    state: "running",
    size: 0,
    fraction: null,
    speed: 0,
  },
];

/** The text a miniature shows on its bar, for looks that put text there. */
const MINIATURE_TEXT: Record<ProgressLabel, string> = {
  percent: "62%",
  bytes: "62 MB",
  remaining: "2m left",
  none: "",
};

const MINIATURE_FRACTION = 0.62;
/** Thick enough for every fill to show what sets it apart. */
const SWATCH_HEIGHT = 10;
const MOTION_SWATCH_HEIGHTS = { min: 6, max: 12 };

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
        const fraction = sample.fraction ?? 0;
        const job = {
          state: sample.state,
          size: sample.size,
          transferred: sample.size * fraction,
          speed: sample.speed,
        };
        return (
          <div
            key={sample.name}
            className={`progress-sample state-${sample.state}`}
            data-direction={sample.direction}
          >
            <span className="progress-sample-name">
              <FileIcon entry={{ name: sample.file ?? sample.name, kind: "file" }} />
              {sample.name}
            </span>
            <span className="progress-sample-bar">
              <ProgressBar
                fraction={sample.fraction}
                label={sample.fraction === null ? "" : progressLabel(job, bars.label, fraction)}
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

interface LookSampleProps {
  look: ProgressBarSettings;
  /** Drawn on the bar. */
  text?: string;
  still?: boolean;
}

/** One running bar in a look of its own, whatever the look of the bars around it. */
function LookSample({ look, text = "", still = false }: LookSampleProps) {
  const { properties, attributes } = progressBarStyle(look);
  return (
    <span
      className="progress-look state-running"
      data-direction="upload"
      style={properties as CSSProperties}
      {...attributes}
    >
      <ProgressBar
        fraction={MINIATURE_FRACTION}
        label={text}
        labelKind={text === "" ? "none" : look.label}
        labelInside={text !== ""}
        still={still}
      />
    </span>
  );
}

interface LookSwatchesProps<T extends string> {
  label: string;
  value: T;
  options: Choice<T>[];
  /** The look a choice is drawn in. */
  lookOf: (value: T) => ProgressBarSettings;
  /** The swatches hold still, for choices that are not about motion. */
  still?: boolean;
  disabled?: boolean;
  /** Fewer, wider swatches to a row. */
  wide?: boolean;
  onChange: (value: T) => void;
}

/** A choice shown as bars drawn each way rather than as a list of names. */
function LookSwatches<T extends string>({
  label,
  value,
  options,
  lookOf,
  still = false,
  disabled = false,
  wide = false,
  onChange,
}: LookSwatchesProps<T>) {
  return (
    <div className={`look-grid ${wide ? "is-wide" : ""}`} role="group" aria-label={label}>
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          className="look-card"
          aria-pressed={option.value === value}
          disabled={disabled}
          onClick={() => onChange(option.value)}
        >
          <LookSample look={lookOf(option.value)} still={still} />
          <span className="look-card-name">{option.label}</span>
        </button>
      ))}
    </div>
  );
}

interface PresetsProps {
  bars: ProgressBarSettings;
  onPick: (change: Partial<ProgressBarSettings>) => void;
}

/** The named looks, each drawn as itself, and the current one when it is none of them. */
function Presets({ bars, onPick }: PresetsProps) {
  const current = matchingPreset(bars);
  return (
    <div className="look-grid is-presets" role="group" aria-label="Presets">
      {PROGRESS_PRESETS.map((preset) => (
        <button
          key={preset.id}
          type="button"
          className="look-card"
          aria-pressed={preset === current}
          onClick={() => onPick(preset.look)}
        >
          <LookSample
            look={{ ...preset.look, color: bars.color }}
            text={preset.look.labelInside ? MINIATURE_TEXT[preset.look.label] : ""}
          />
          <span className="look-card-name">{preset.name}</span>
        </button>
      ))}
      <div
        className={`look-card look-card-custom ${current ? "" : "is-current"}`}
        title={current ? "Change anything below to make a look of your own" : undefined}
      >
        {current ? (
          <span className="look-card-blank" />
        ) : (
          <LookSample look={bars} text={bars.labelInside ? MINIATURE_TEXT[bars.label] : ""} />
        )}
        <span className="look-card-name">Custom</span>
      </div>
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
  /** The current look with one thing changed, and without text. */
  const swatchLook = (change: Partial<ProgressBarSettings>): ProgressBarSettings => ({
    ...bars,
    label: "none",
    labelInside: false,
    ...change,
  });
  const shaped = fillTakesShape(bars.fill);
  const fill = FILLS.find((option) => option.value === bars.fill);

  return (
    <SettingGroup title="Progress bars">
      <ProgressPreview bars={bars} />

      <LookHeading title="Presets" hint="One click sets everything below." />
      <Presets bars={bars} onPick={save} />

      <LookHeading title="Fill" hint={fill?.hint} />
      <LookSwatches
        label="Fill"
        value={bars.fill}
        options={FILLS}
        lookOf={(value) => swatchLook({ fill: value, motion: "none", height: SWATCH_HEIGHT })}
        still
        onChange={(value) => save({ fill: value })}
      />

      <LookHeading title="Color" />
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

      <LookHeading title="Shape" />
      <SettingRow
        label="Ends"
        hint={shaped ? undefined : `${fill?.label ?? "This fill"} keeps its own shape.`}
        disabled={!shaped}
      >
        <LookSwatches
          label="Ends"
          value={bars.shape}
          options={SHAPES}
          lookOf={(value) => swatchLook({ shape: value, motion: "none", height: SWATCH_HEIGHT })}
          still
          disabled={!shaped}
          onChange={(value) => save({ shape: value })}
        />
      </SettingRow>
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

      <LookHeading
        title="Motion"
        hint="Plays on bars whose work is under way, and holds still when the system asks for less motion."
      />
      <LookSwatches
        label="Motion"
        value={bars.motion}
        options={MOTIONS}
        lookOf={(value) =>
          swatchLook({
            motion: value,
            height: Math.min(
              MOTION_SWATCH_HEIGHTS.max,
              Math.max(MOTION_SWATCH_HEIGHTS.min, bars.height),
            ),
          })
        }
        wide
        onChange={(value) => save({ motion: value })}
      />

      <LookHeading title="Text" />
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

/** Names a part of the progress bar settings, which are too many for one list. */
function LookHeading({ title, hint }: { title: string; hint?: string }) {
  return (
    <div className="look-heading">
      <h4>{title}</h4>
      {hint && <span className="setting-hint">{hint}</span>}
    </div>
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
        hint="Each file you upload, download, move or copy hops out of its pane as it finishes and lands on its row where it went, or on the folder it went into."
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
        <button type="button" className="button" onClick={play}>
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
          The system asks apps for less motion, as Windows does while its animation effects are off.
          Files fly anyway while this is on.
        </p>
      )}
    </SettingGroup>
  );
}
