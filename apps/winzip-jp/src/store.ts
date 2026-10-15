import { create } from "zustand";
import { getApi } from "./api";
import { COPY } from "./copy";
import type {
  ListReport,
  Mode,
  NameEncoding,
  PlanDto,
  ProgressDto,
  Unmappable,
} from "./types";

export type JobKind = "create" | "extract" | null;

export type AppState = {
  mode: Mode;
  dropped: string[];
  plan: PlanDto | null;
  replacements: Record<string, string>;
  replacementsConfirmed: boolean;
  sheetOpen: boolean;
  list: ListReport | null;
  archivePath: string | null;
  encodingOverride: NameEncoding | null;
  progress: ProgressDto | null;
  job: JobKind;
  lastDest: string | null;
  status: string;
  error: string;
  busy: boolean;
  setMode: (mode: Mode) => void;
  setProgress: (progress: ProgressDto) => void;
  resetWork: () => void;
  ingestDropped: (paths: string[]) => Promise<void>;
  pickInputs: () => Promise<void>;
  updateReplacement: (relativePath: string, suggested: string) => void;
  dismissSheet: () => void;
  confirmAndCreate: () => Promise<void>;
  createZip: () => Promise<void>;
  setEncoding: (encoding: NameEncoding) => Promise<void>;
  extractZip: () => Promise<void>;
  cancel: () => Promise<void>;
  closeJob: () => void;
  applyScene: (partial: Partial<Pick<
    AppState,
    | "mode"
    | "dropped"
    | "plan"
    | "replacements"
    | "replacementsConfirmed"
    | "sheetOpen"
    | "list"
    | "archivePath"
    | "encodingOverride"
    | "progress"
    | "job"
    | "lastDest"
    | "status"
    | "error"
    | "busy"
  >>) => void;
};

function errText(err: unknown): string {
  if (err instanceof Error && err.message) {
    return err.message;
  }
  return String(err);
}

function firstZip(paths: string[]): string | undefined {
  return paths.find((p) => p.toLowerCase().endsWith(".zip")) ?? paths[0];
}

function defaultReplacements(issues: Unmappable[]): Record<string, string> {
  const next: Record<string, string> = {};
  for (const issue of issues) {
    if (next[issue.relativePath] === undefined) {
      next[issue.relativePath] = issue.suggested;
    }
  }
  return next;
}

function destHint(plan: PlanDto | null, fallback: string): string {
  if (plan?.destNameHint) {
    return plan.destNameHint.endsWith(".zip") ? plan.destNameHint : `${plan.destNameHint}.zip`;
  }
  const first = plan?.entries.find((name) => name && !name.endsWith("/"));
  if (first) {
    const base = first.split("/")[0] ?? first;
    const stem = base.replace(/\.[^./]+$/, "");
    return `${stem || "archive"}.zip`;
  }
  return fallback;
}

const emptyWork = {
  dropped: [] as string[],
  plan: null as PlanDto | null,
  replacements: {} as Record<string, string>,
  replacementsConfirmed: true,
  sheetOpen: false,
  list: null as ListReport | null,
  archivePath: null as string | null,
  encodingOverride: null as NameEncoding | null,
  progress: null as ProgressDto | null,
  job: null as JobKind,
  lastDest: null as string | null,
  status: "",
  error: "",
  busy: false,
};

export const useAppStore = create<AppState>((set, get) => ({
  mode: "create",
  ...emptyWork,

  setMode(mode) {
    if (get().busy && get().job) {
      return;
    }
    set({ mode, ...emptyWork });
  },

  setProgress(progress) {
    set({ progress });
  },

  resetWork() {
    set({ ...emptyWork });
  },

  applyScene(partial) {
    set(partial);
  },

  async ingestDropped(paths) {
    const usable = paths.filter(Boolean);
    if (usable.length === 0) {
      set({ error: get().mode === "create" ? COPY.noFiles : COPY.noZip });
      return;
    }
    set({
      dropped: usable,
      error: "",
      status: "",
      lastDest: null,
      progress: null,
      job: null,
    });
    const api = getApi();
    if (get().mode === "create") {
      set({ busy: true });
      try {
        const plan = await api.planArchive(usable);
        const issues = plan.issues ?? [];
        set({
          plan,
          replacements: defaultReplacements(issues),
          replacementsConfirmed: issues.length === 0,
          sheetOpen: issues.length > 0,
          list: null,
          archivePath: null,
        });
      } catch (err) {
        set({ plan: null, error: errText(err) });
      } finally {
        set({ busy: false });
      }
      return;
    }
    const archive = firstZip(usable);
    if (!archive) {
      set({ error: COPY.noZip });
      return;
    }
    set({ busy: true, archivePath: archive });
    try {
      const list = await api.listArchive(archive, get().encodingOverride);
      set({ list, plan: null, sheetOpen: false });
    } catch (err) {
      set({ list: null, error: errText(err) });
    } finally {
      set({ busy: false });
    }
  },

  async pickInputs() {
    try {
      const paths = await getApi().pickFiles();
      if (paths.length === 0) {
        set({ error: get().mode === "create" ? COPY.noFiles : COPY.noZip });
        return;
      }
      await get().ingestDropped(paths);
    } catch (err) {
      set({ error: errText(err) });
    }
  },

  updateReplacement(relativePath, suggested) {
    set({
      replacements: { ...get().replacements, [relativePath]: suggested },
      replacementsConfirmed: false,
    });
  },

  dismissSheet() {
    set({ sheetOpen: false, replacementsConfirmed: false });
  },

  async confirmAndCreate() {
    const { plan } = get();
    if (!plan) {
      return;
    }
    set({ replacementsConfirmed: true, sheetOpen: false });
    await get().createZip();
  },

  async createZip() {
    const { dropped, plan, replacements, replacementsConfirmed } = get();
    if (dropped.length === 0) {
      set({ error: COPY.noFiles });
      return;
    }
    if (plan && plan.issues.length > 0 && !replacementsConfirmed) {
      set({ sheetOpen: true, error: COPY.needConfirm });
      return;
    }
    set({
      busy: true,
      error: "",
      status: "",
      job: "create",
      progress: {
        phase: "starting",
        currentEntry: null,
        entriesDone: 0,
        entriesTotal: 0,
        bytesDone: 0,
        bytesTotal: 0,
      },
    });
    try {
      const dest = await getApi().pickSaveZip(destHint(plan, "archive.zip"));
      if (!dest) {
        set({ error: COPY.noDest, busy: false, job: null });
        return;
      }
      const report = await getApi().createArchive({ paths: dropped, replacements }, dest);
      set({
        lastDest: report.dest,
        status: COPY.created,
        progress: {
          phase: "done",
          currentEntry: null,
          entriesDone: report.entriesWritten,
          entriesTotal: report.entriesWritten,
          bytesDone: report.bytesWritten,
          bytesTotal: report.bytesWritten,
        },
      });
    } catch (err) {
      set({ error: errText(err), job: null, progress: null });
    } finally {
      set({ busy: false });
    }
  },

  async setEncoding(encoding) {
    const archive = get().archivePath;
    if (!archive) {
      set({ encodingOverride: encoding });
      return;
    }
    set({ encodingOverride: encoding, busy: true, error: "" });
    try {
      const list = await getApi().listArchive(archive, encoding);
      set({ list });
    } catch (err) {
      set({ error: errText(err) });
    } finally {
      set({ busy: false });
    }
  },

  async extractZip() {
    const archive = get().archivePath ?? firstZip(get().dropped);
    if (!archive) {
      set({ error: COPY.noZip });
      return;
    }
    set({
      busy: true,
      error: "",
      status: "",
      job: "extract",
      archivePath: archive,
      progress: {
        phase: "starting",
        currentEntry: null,
        entriesDone: 0,
        entriesTotal: 0,
        bytesDone: 0,
        bytesTotal: 0,
      },
    });
    try {
      const dest = await getApi().pickExtractDir();
      if (!dest) {
        set({ error: COPY.noExtractDest, busy: false, job: null });
        return;
      }
      const report = await getApi().extractArchive({
        archive,
        dest,
        encoding: get().encodingOverride,
        overwrite: false,
      });
      set({
        lastDest: report.dest,
        status: COPY.extracted,
        progress: {
          phase: "done",
          currentEntry: null,
          entriesDone: report.entriesWritten,
          entriesTotal: report.entriesWritten,
          bytesDone: report.bytesWritten,
          bytesTotal: report.bytesWritten,
        },
      });
    } catch (err) {
      set({ error: errText(err), job: null, progress: null });
    } finally {
      set({ busy: false });
    }
  },

  async cancel() {
    await getApi().cancelJob();
    set({
      busy: false,
      job: null,
      progress: null,
      status: "",
      error: COPY.cancelled,
    });
  },

  closeJob() {
    set({ job: null, progress: null });
  },
}));
