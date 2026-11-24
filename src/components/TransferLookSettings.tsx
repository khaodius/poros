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
import { formatNumber, t, useT } from "../i18n";
import { formatDuration, formatPercent } from "../lib/format";

interface Choice<T extends string> {
  value: T;
  label: string;
}

const FILLS: (Choice<ProgressFill> & { hint: string })[] = [
  {
    value: "solid",
    get label() {
      return t("appearance.solid");
    },
    get hint() {
      return t("appearance.oneFlatColor");
    },
  },
  {
    value: "gradient",
    get label() {
      return t("appearance.gradient");
    },
    get hint() {
      return t("appearance.fadesInFromTheStart");
    },
  },
  {
    value: "striped",
    get label() {
      return t("appearance.striped");
    },
    get hint() {
      return t("appearance.faintDiagonalStripes");
    },
  },
  {
    value: "segmented",
    get label() {
      return t("appearance.segmented");
    },
    get hint() {
      return t("appearance.cutIntoShortPieces");
    },
  },
  {
    value: "glow",
    get label() {
      return t("appearance.glowing");
    },
    get hint() {
      return t("appearance.litFromAboveWithA");
    },
  },
  {
    value: "candy",
    get label() {
      return t("appearance.candy");
    },
    get hint() {
      return t("appearance.wideStripesInTwoTones");
    },
  },
  {
    value: "chevrons",
    get label() {
      return t("appearance.chevrons");
    },
    get hint() {
      return t("appearance.arrowheadsThatPointTheWay");
    },
  },
  {
    value: "aurora",
    get label() {
      return t("appearance.aurora");
    },
    get hint() {
      return t("appearance.theColorDriftsToThe");
    },
  },
  {
    value: "neon",
    get label() {
      return t("appearance.neon");
    },
    get hint() {
      return t("appearance.anOutlinedTubeWithA");
    },
  },
  {
    value: "wave",
    get label() {
      return t("appearance.wave");
    },
    get hint() {
      return t("appearance.aLiquidItsSurfaceRipples");
    },
  },
  {
    value: "line",
    get label() {
      return t("appearance.line");
    },
    get hint() {
      return t("appearance.aThinLineLedBy");
    },
  },
  {
    value: "dots",
    get label() {
      return t("appearance.dots");
    },
    get hint() {
      return t("appearance.beadsThatLightUpOne");
    },
  },
  {
    value: "blocks",
    get label() {
      return t("appearance.blocks");
    },
    get hint() {
      return t("appearance.cellsThatLightUpOne");
    },
  },
];

const COLORINGS: Choice<ProgressColoring>[] = [
  {
    value: "theme",
    get label() {
      return t("appearance.theThemes");
    },
  },
  {
    value: "direction",
    get label() {
      return t("appearance.byDirection");
    },
  },
  {
    value: "custom",
    get label() {
      return t("appearance.custom");
    },
  },
];

const COLORING_HINTS: Record<ProgressColoring, string> = {
  get theme() {
    return t("appearance.theThemesProgressBarsColor");
  },
  get direction() {
    return t("appearance.uploadsTakeTheThemesUploads");
  },
  get custom() {
    return t("appearance.theSameColorForEvery");
  },
};

const SHAPES: Choice<ProgressShape>[] = [
  {
    value: "pill",
    get label() {
      return t("appearance.round");
    },
  },
  {
    value: "soft",
    get label() {
      return t("appearance.soft");
    },
  },
  {
    value: "square",
    get label() {
      return t("appearance.square");
    },
  },
  {
    value: "slanted",
    get label() {
      return t("appearance.slanted");
    },
  },
];

const MOTIONS: Choice<ProgressMotion>[] = [
  {
    value: "none",
    get label() {
      return t("common.nothing");
    },
  },
  {
    value: "stripes",
    get label() {
      return t("appearance.movingStripes");
    },
  },
  {
    value: "shimmer",
    get label() {
      return t("appearance.shimmer");
    },
  },
  {
    value: "pulse",
    get label() {
      return t("appearance.pulse");
    },
  },
  {
    value: "scan",
    get label() {
      return t("appearance.scan");
    },
  },
  {
    value: "ripple",
    get label() {
      return t("appearance.ripple");
    },
  },
  {
    value: "comet",
    get label() {
      return t("appearance.comet");
    },
  },
  {
    value: "sparkle",
    get label() {
      return t("appearance.sparkle");
    },
  },
  {
    value: "breathe",
    get label() {
      return t("appearance.breathingGlow");
    },
  },
];

const LABELS: Choice<ProgressLabel>[] = [
  {
    value: "percent",
    get label() {
      return t("appearance.percent");
    },
  },
  {
    value: "bytes",
    get label() {
      return t("appearance.transferredAndSize");
    },
  },
  {
    value: "remaining",
    get label() {
      return t("appearance.timeLeft");
    },
  },
  {
    value: "none",
    get label() {
      return t("appearance.noText");
    },
  },
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
    get name() {
      return t("appearance.sampleUnpacking", { name: "site.zip" });
    },
    file: "site.zip",
    state: "running",
    size: 0,
    fraction: null,
    speed: 0,
  },
];

/** The text a miniature shows on its bar, for looks that put text there. */
function miniatureText(label: ProgressLabel): string {
  switch (label) {
    case "percent":
      return formatPercent(62);
    case "bytes":
      return `${formatNumber(62)} MB`;
    case "remaining":
      return t("transfers.timeLeft", { duration: formatDuration(120) });
    case "none":
      return "";
  }
}

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
  const t = useT();
  return (
    <div className="progress-preview" aria-label={t("appearance.preview")}>
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
  const t = useT();
  const current = matchingPreset(bars);
  return (
    <div className="look-grid is-presets" role="group" aria-label={t("appearance.presets")}>
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
            text={preset.look.labelInside ? miniatureText(preset.look.label) : ""}
          />
          <span className="look-card-name">{preset.name}</span>
        </button>
      ))}
      <div
        className={`look-card look-card-custom ${current ? "" : "is-current"}`}
        title={current ? t("appearance.changeAnythingBelowToMake") : undefined}
      >
        {current ? (
          <span className="look-card-blank" />
        ) : (
          <LookSample look={bars} text={bars.labelInside ? miniatureText(bars.label) : ""} />
        )}
        <span className="look-card-name">{t("appearance.custom")}</span>
      </div>
    </div>
  );
}

export function ProgressBarGroup() {
  const t = useT();
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
    <SettingGroup title={t("appearance.progressBars")}>
      <ProgressPreview bars={bars} />

      <LookHeading title={t("appearance.presets")} hint={t("appearance.presetsHint")} />
      <Presets bars={bars} onPick={save} />

      <LookHeading title={t("appearance.fill")} hint={fill?.hint} />
      <LookSwatches
        label={t("appearance.fill")}
        value={bars.fill}
        options={FILLS}
        lookOf={(value) => swatchLook({ fill: value, motion: "none", height: SWATCH_HEIGHT })}
        still
        onChange={(value) => save({ fill: value })}
      />

      <LookHeading title={t("appearance.color")} />
      <SettingRow label={t("appearance.barColor")} hint={COLORING_HINTS[bars.coloring]}>
        <span className="progress-color">
          {bars.coloring === "custom" && (
            <ColorInput
              className="swatch-picker"
              aria-label={t("appearance.customBarColor")}
              value={bars.color}
              onPreview={(color) => preview({ color })}
              onCommit={(color) => save({ color })}
            />
          )}
          <select
            value={bars.coloring}
            aria-label={t("appearance.barColor")}
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
        label={t("appearance.colorFinishedPausedAndFailed")}
        hint={t("appearance.colorFinishedPausedAndFailedHint")}
        checked={bars.stateColors}
        onChange={(stateColors) => save({ stateColors })}
      />

      <LookHeading title={t("appearance.shape")} />
      <SettingRow
        label={t("appearance.ends")}
        hint={
          shaped
            ? undefined
            : fill
              ? t("appearance.fillKeepsItsShape", { fill: fill.label })
              : t("appearance.thisFillKeepsItsShape")
        }
        disabled={!shaped}
      >
        <LookSwatches
          label={t("appearance.ends")}
          value={bars.shape}
          options={SHAPES}
          lookOf={(value) => swatchLook({ shape: value, motion: "none", height: SWATCH_HEIGHT })}
          still
          disabled={!shaped}
          onChange={(value) => save({ shape: value })}
        />
      </SettingRow>
      <RangeSetting
        label={t("appearance.thickness")}
        hint={t("appearance.thicknessHint")}
        value={bars.height}
        min={PROGRESS_HEIGHT_LIMITS.min}
        max={PROGRESS_HEIGHT_LIMITS.max}
        format={pixels}
        onPreview={(height) => preview({ height })}
        onCommit={(height) => save({ height })}
      />

      <LookHeading title={t("appearance.motion")} hint={t("appearance.motionHint")} />
      <LookSwatches
        label={t("appearance.motion")}
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

      <LookHeading title={t("appearance.text")} />
      <SelectSetting
        label={t("appearance.transferText")}
        hint={t("appearance.transferTextHint")}
        value={bars.label}
        options={LABELS}
        onChange={(label) => save({ label })}
      />
      <SwitchSetting
        label={t("appearance.textOnTheBar")}
        hint={t("appearance.textOnTheBarHint")}
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
          {t("appearance.resetProgressBars")}
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
  const t = useT();
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
    <SettingGroup title={t("appearance.animation")}>
      <SwitchSetting
        label={t("appearance.flyingFiles")}
        hint={t("appearance.flyingFilesHint")}
        checked={transferAnimation}
        onChange={(checked) =>
          void saveSettingsSection("appearance", { transferAnimation: checked })
        }
      />
      <div className="flight-demo" ref={stageRef}>
        <div className="flight-demo-pane flight-demo-local">
          <span className="flight-demo-title">{t("appearance.thisComputer")}</span>
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
          <span className="flight-demo-title">{t("appearance.server")}</span>
          <span className="flight-demo-files">
            {SERVER_DEMO_FILES.map((file) => (
              <FileIcon key={file.name} entry={file} />
            ))}
          </span>
        </div>
      </div>
      {lessMotion && <p className="setting-note">{t("appearance.theSystemAsksAppsFor")}</p>}
    </SettingGroup>
  );
}
