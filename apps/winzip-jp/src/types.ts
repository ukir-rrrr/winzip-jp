export type Mode = "create" | "extract";

export type NameEncoding = "cp932" | "utf8" | "cp437";

export type UnmappableKind = "compatIdeograph" | "emoji" | "other";

export type Unmappable = {
  relativePath: string;
  originalChar: string;
  suggested: string;
  kind: UnmappableKind;
};

export type PlanDto = {
  entries: string[];
  issues: Unmappable[];
  excluded: string[];
  destNameHint: string | null;
};

export type ListedEntry = {
  nameHex: string;
  displayName: string;
  isDir: boolean;
  uncompressedSize: number;
  gpbf: number;
};

export type ListReport = {
  entries: ListedEntry[];
  encoding: NameEncoding;
  confidence: number;
  ambiguous: boolean;
};

export type ProgressPhase = "starting" | "writing" | "finishing" | "done";

export type ProgressDto = {
  phase: ProgressPhase;
  currentEntry: string | null;
  entriesDone: number;
  entriesTotal: number;
  bytesDone: number;
  bytesTotal: number;
};

export type WriteReport = {
  dest: string;
  entriesWritten: number;
  bytesWritten: number;
  excluded: string[];
};

export type ExtractReport = {
  dest: string;
  entriesWritten: number;
  bytesWritten: number;
};

export type SplitPresetMb = 8 | 10 | 25 | 100;

export const SPLIT_PRESET_MB: readonly SplitPresetMb[] = [8, 10, 25, 100];

export const DEFAULT_SPLIT_MB: SplitPresetMb = 10;

export function splitMbToBytes(mb: number): number {
  return mb * 1024 * 1024;
}

export type CreateReq = {
  paths: string[];
  replacements: Record<string, string>;
  password?: string | null;
  splitBytes?: number | null;
};

export type ExtractReq = {
  archive: string;
  dest: string;
  encoding: NameEncoding | null;
  overwrite: boolean;
};

export type ZipApi = {
  planArchive: (paths: string[], destNameHint?: string | null) => Promise<PlanDto>;
  checkNames: (names: string[]) => Promise<boolean>;
  createArchive: (req: CreateReq, dest: string) => Promise<WriteReport>;
  listArchive: (path: string, encoding: NameEncoding | null) => Promise<ListReport>;
  extractArchive: (req: ExtractReq) => Promise<ExtractReport>;
  cancelJob: () => Promise<void>;
  pickFiles: () => Promise<string[]>;
  pickSaveZip: (defaultName: string) => Promise<string | null>;
  pickExtractDir: () => Promise<string | null>;
};
