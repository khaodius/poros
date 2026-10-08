// Permission bits as the Properties dialog edits them. Each bit is on, off, or mixed when the
// selected items disagree; a mixed bit is left as each item has it.

import type { ModeChange } from "./types";

export type BitState = "on" | "off" | "mixed";

/** The twelve permission bits, keyed by value. */
export type BitStates = Record<number, BitState>;

export const SPECIAL_BITS = [
  { bit: 0o4000, label: "Set user ID", hint: "Runs as the file's owner" },
  { bit: 0o2000, label: "Set group ID", hint: "Runs as the group; new files inherit it" },
  { bit: 0o1000, label: "Sticky", hint: "Only owners may delete or rename entries" },
] as const;

export const PERMISSION_CLASSES = [
  { label: "Owner", shift: 6 },
  { label: "Group", shift: 3 },
  { label: "Others", shift: 0 },
] as const;

export const PERMISSION_KINDS = [
  { label: "Read", value: 4 },
  { label: "Write", value: 2 },
  { label: "Execute", value: 1 },
] as const;

export const ALL_BITS: number[] = [
  ...SPECIAL_BITS.map((special) => special.bit),
  ...PERMISSION_CLASSES.flatMap((permissionClass) =>
    PERMISSION_KINDS.map((kind) => kind.value << permissionClass.shift),
  ),
];

/** What the modes agree on; no modes leaves every bit mixed. */
export function bitStates(modes: number[]): BitStates {
  const states: BitStates = {};
  for (const bit of ALL_BITS) {
    const set = modes.filter((mode) => (mode & bit) !== 0).length;
    states[bit] =
      modes.length === 0 ? "mixed" : set === modes.length ? "on" : set === 0 ? "off" : "mixed";
  }
  return states;
}

export function statesFromMode(mode: number): BitStates {
  return bitStates([mode]);
}

export function toModeChange(states: BitStates): ModeChange {
  let set = 0;
  let clear = 0;
  for (const bit of ALL_BITS) {
    if (states[bit] === "on") set |= bit;
    else if (states[bit] === "off") clear |= bit;
  }
  return { set, clear };
}

export function isUnchanged(states: BitStates, original: BitStates): boolean {
  return ALL_BITS.every((bit) => states[bit] === original[bit] || states[bit] === "mixed");
}

/** `755`, or `4755` with special bits; null while any bit is mixed. */
export function toOctal(states: BitStates): string | null {
  if (ALL_BITS.some((bit) => states[bit] === "mixed")) return null;
  return toModeChange(states).set.toString(8).padStart(3, "0");
}

/** Three or four octal digits; null for anything else. */
export function parseOctal(text: string): number | null {
  const trimmed = text.trim();
  if (!/^[0-7]{3,4}$/.test(trimmed)) return null;
  return parseInt(trimmed, 8);
}

/** `rwxr-x---`, with `s`/`S` and `t`/`T` for special bits and `?` where items differ. */
export function symbolic(states: BitStates): string {
  const letter = (bit: number, character: string) =>
    states[bit] === "mixed" ? "?" : states[bit] === "on" ? character : "-";
  const execute = (bit: number, special: number, character: string) => {
    if (states[special] === "mixed" || states[bit] === "mixed") return "?";
    if (states[special] === "off") return letter(bit, "x");
    return states[bit] === "on" ? character : character.toUpperCase();
  };
  return [
    letter(0o400, "r"),
    letter(0o200, "w"),
    execute(0o100, 0o4000, "s"),
    letter(0o040, "r"),
    letter(0o020, "w"),
    execute(0o010, 0o2000, "s"),
    letter(0o004, "r"),
    letter(0o002, "w"),
    execute(0o001, 0o1000, "t"),
  ].join("");
}

/** A mixed bit becomes on, and after that the bit toggles. */
export function nextState(state: BitState): BitState {
  return state === "on" ? "off" : "on";
}
