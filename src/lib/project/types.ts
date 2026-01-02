import type { SynthPatch } from "../synth.js";

// Re-export for convenience
export type { SynthPatch };

// Channel type
export type ChannelType = "sample" | "synth";

// Channel as stored in project file
export interface ProjectChannel {
  name: string;
  type: ChannelType;
  sample: string;
  synthPatch: SynthPatch;
  muted: boolean;
  solo: boolean;
  volume: number;
}

// Note as stored in project file
export interface ProjectNote {
  id: string;
  pitch: number;
  startStep: number;
  duration: number;
}

// Pattern as stored in project file
export interface ProjectPattern {
  id: number;
  name: string;
  steps: boolean[][];
  notes: ProjectNote[][];
}

// Pattern placement in arrangement
export interface ProjectPlacement {
  id: string;
  patternId: number;
  startBar: number;
  length: number;
}

// Arrangement as stored in project file (mutedPatterns as array for JSON)
export interface ProjectArrangement {
  placements: ProjectPlacement[];
  mutedPatterns: number[]; // Array instead of Set for JSON serialization
}

// Complete project file structure
export interface ProjectFile {
  version: number;
  name: string;
  createdAt: string;
  modifiedAt: string;
  bpm: number;
  currentPatternId: number;
  channels: ProjectChannel[];
  patterns: ProjectPattern[];
  arrangement: ProjectArrangement;
}

// Project metadata for runtime
export interface ProjectMetadata {
  path: string;
  name: string;
  createdAt: Date;
  modifiedAt: Date;
  isDirty: boolean;
}

// Current project file version
export const PROJECT_VERSION = 1;
