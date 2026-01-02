import type {
  ProjectFile,
  ProjectChannel,
  ProjectPattern,
  ProjectArrangement,
  ProjectPlacement,
  ProjectNote,
} from "./types.js";
import { PROJECT_VERSION } from "./types.js";
import type { SynthPatch } from "../synth.js";

// Runtime types matching SequencerContext
export interface RuntimeChannel {
  name: string;
  type: "sample" | "synth";
  sample: string;
  synthPatch: SynthPatch;
  muted: boolean;
  solo: boolean;
  volume: number;
}

export interface RuntimeNote {
  id: string;
  pitch: number;
  startStep: number;
  duration: number;
}

export interface RuntimePattern {
  id: number;
  name: string;
  steps: boolean[][];
  notes: RuntimeNote[][];
}

export interface RuntimeArrangement {
  placements: ProjectPlacement[];
  mutedPatterns: Set<number>;
}

export interface RuntimeState {
  bpm: number;
  currentPatternId: number;
  channels: RuntimeChannel[];
  patterns: RuntimePattern[];
  arrangement: RuntimeArrangement;
}

/**
 * Serialize runtime state to project file format
 */
export function serializeProject(
  state: RuntimeState,
  name: string,
  createdAt?: Date,
): ProjectFile {
  const now = new Date().toISOString();

  return {
    version: PROJECT_VERSION,
    name,
    createdAt: createdAt?.toISOString() ?? now,
    modifiedAt: now,
    bpm: state.bpm,
    currentPatternId: state.currentPatternId,
    channels: state.channels.map(serializeChannel),
    patterns: state.patterns.map(serializePattern),
    arrangement: serializeArrangement(state.arrangement),
  };
}

function serializeChannel(channel: RuntimeChannel): ProjectChannel {
  return {
    name: channel.name,
    type: channel.type,
    sample: channel.sample,
    synthPatch: channel.synthPatch,
    muted: channel.muted,
    solo: channel.solo,
    volume: channel.volume,
  };
}

function serializePattern(pattern: RuntimePattern): ProjectPattern {
  return {
    id: pattern.id,
    name: pattern.name,
    steps: pattern.steps,
    notes: pattern.notes.map((channelNotes) => channelNotes.map(serializeNote)),
  };
}

function serializeNote(note: RuntimeNote): ProjectNote {
  return {
    id: note.id,
    pitch: note.pitch,
    startStep: note.startStep,
    duration: note.duration,
  };
}

function serializeArrangement(
  arrangement: RuntimeArrangement,
): ProjectArrangement {
  return {
    placements: arrangement.placements,
    mutedPatterns: Array.from(arrangement.mutedPatterns),
  };
}

/**
 * Deserialize project file to runtime state
 */
export function deserializeProject(file: ProjectFile): RuntimeState {
  // Handle version migrations if needed
  const migrated = migrateProject(file);

  return {
    bpm: migrated.bpm,
    currentPatternId: migrated.currentPatternId,
    channels: migrated.channels.map(deserializeChannel),
    patterns: migrated.patterns.map(deserializePattern),
    arrangement: deserializeArrangement(migrated.arrangement),
  };
}

function deserializeChannel(channel: ProjectChannel): RuntimeChannel {
  return {
    name: channel.name,
    type: channel.type,
    sample: channel.sample,
    synthPatch: channel.synthPatch,
    muted: channel.muted,
    solo: channel.solo,
    volume: channel.volume,
  };
}

function deserializePattern(pattern: ProjectPattern): RuntimePattern {
  return {
    id: pattern.id,
    name: pattern.name,
    steps: pattern.steps,
    notes: pattern.notes.map((channelNotes) =>
      channelNotes.map(deserializeNote),
    ),
  };
}

function deserializeNote(note: ProjectNote): RuntimeNote {
  return {
    id: note.id,
    pitch: note.pitch,
    startStep: note.startStep,
    duration: note.duration,
  };
}

function deserializeArrangement(
  arrangement: ProjectArrangement,
): RuntimeArrangement {
  return {
    placements: arrangement.placements,
    mutedPatterns: new Set(arrangement.mutedPatterns),
  };
}

/**
 * Migrate older project formats to current version
 */
function migrateProject(file: ProjectFile): ProjectFile {
  const migrated = { ...file };

  // Version 0 or undefined -> Version 1
  if (!migrated.version || migrated.version < 1) {
    // Add any v0 to v1 migrations here
    migrated.version = 1;
  }

  // Future migrations would go here:
  // if (migrated.version < 2) { ... }

  return migrated;
}

/**
 * Validate a project file structure
 */
export function validateProject(file: unknown): file is ProjectFile {
  if (!file || typeof file !== "object") return false;
  const f = file as Record<string, unknown>;

  // Check required fields
  if (typeof f.version !== "number") return false;
  if (typeof f.name !== "string") return false;
  if (typeof f.bpm !== "number") return false;
  if (typeof f.currentPatternId !== "number") return false;
  if (!Array.isArray(f.channels)) return false;
  if (!Array.isArray(f.patterns)) return false;
  if (!f.arrangement || typeof f.arrangement !== "object") return false;

  return true;
}
