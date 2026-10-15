import { create } from "zustand";
import { getApi } from "./api";
import { COPY } from "./copy";
import { defaultReplacements, isValidReplacement } from "./replacements";
import type {
  ExtractReport,
  ListReport,
  Mode,
  NameEncoding,
  PlanDto,
  ProgressDto,
  WriteReport,
} from "./types";

export type JobKind = "create" | "extract";

export type Job = {
  kind: JobKind;
  progress: ProgressDto;
  dest: string | null;
  epoch: number;
};

export type Sheet = "none" | "open" | "dismissed" | "confirmed";

export type AppState = {
  mode: Mode;
  dropped: string[];
  plan: PlanDto | null;
  replacements: Record<string, string>;
  sheet: Sheet;
  list: ListReport | null;
  archivePath: string | null;
  encodingOverride: NameEncoding | null;
  job: Job | null;
  epoch: number;
  error: string;
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

function startingProgress(): ProgressDto {
  return {
    phase: "starting",
    currentEntry: null,
    entriesDone: 0,
    entriesTotal: 0,
    bytesDone: 0,
    bytesTotal: 0,
  };
}

function doneProgress(entries: number, bytes: number): ProgressDto {
  return {
    phase: "done",
    currentEntry: null,
    entriesDone: entries,
    entriesTotal: entries,
    bytesDone: bytes,
    bytesTotal: bytes,
  };
}

function jobInFlight(job: Job | null): boolean {
  return job !== null && job.progress.phase !== "done";
}

const emptyWork = {
  dropped: [] as string[],
  plan: null as PlanDto | null,
  replacements: {} as Record<string, string>,
  sheet: "none" as Sheet,
  list: null as ListReport | null,
  archivePath: null as string | null,
  encodingOverride: null as NameEncoding | null,
  job: null as Job | null,
  error: "",
};

export const useAppStore = create<AppState>((set, get) => {
  function bumpEpoch(): number {
    const epoch = get().epoch + 1;
    set({ epoch });
    return epoch;
  }

  async function runJob(
    kind: JobKind,
    pickDest: () => Promise<string | null>,
    run: (dest: string) => Promise<WriteReport | ExtractReport>,
    noDest: string,
  ): Promise<void> {
    const epoch = bumpEpoch();
    set({
      error: "",
      job: { kind, progress: startingProgress(), dest: null, epoch },
    });
    try {
      const dest = await pickDest();
      if (get().epoch !== epoch) {
        return;
      }
      if (!dest) {
        set({ error: noDest, job: null });
        return;
      }
      const report = await run(dest);
      if (get().epoch !== epoch) {
        return;
      }
      set({
        job: {
          kind,
          dest: report.dest,
          progress: doneProgress(report.entriesWritten, report.bytesWritten),
          epoch,
        },
      });
    } catch (err) {
      if (get().epoch !== epoch) {
        return;
      }
      set({ error: errText(err), job: null });
    }
  }

  return {
    mode: "create",
    epoch: 0,
    ...emptyWork,

    setMode(mode) {
      if (jobInFlight(get().job)) {
        return;
      }
      set({ mode, ...emptyWork, epoch: get().epoch + 1 });
    },

    setProgress(progress) {
      const job = get().job;
      if (!job || job.epoch !== get().epoch) {
        return;
      }
      set({ job: { ...job, progress } });
    },

    resetWork() {
      set({ ...emptyWork, epoch: get().epoch + 1 });
    },

    async ingestDropped(paths) {
      if (jobInFlight(get().job)) {
        return;
      }
      const usable = paths.filter(Boolean);
      if (usable.length === 0) {
        set({ error: get().mode === "create" ? COPY.noFiles : COPY.noZip });
        return;
      }
      const epoch = bumpEpoch();
      set({ dropped: usable, error: "", job: null });
      const api = getApi();
      if (get().mode === "create") {
        try {
          const plan = await api.planArchive(usable);
          if (get().epoch !== epoch) {
            return;
          }
          const issues = plan.issues ?? [];
          set({
            plan,
            replacements: defaultReplacements(issues),
            sheet: issues.length > 0 ? "open" : "none",
            list: null,
            archivePath: null,
          });
        } catch (err) {
          if (get().epoch !== epoch) {
            return;
          }
          set({ plan: null, sheet: "none", error: errText(err) });
        }
        return;
      }
      const archive = firstZip(usable);
      if (!archive) {
        set({ error: COPY.noZip });
        return;
      }
      try {
        const list = await api.listArchive(archive, get().encodingOverride);
        if (get().epoch !== epoch) {
          return;
        }
        set({ list, plan: null, sheet: "none", archivePath: archive });
      } catch (err) {
        if (get().epoch !== epoch) {
          return;
        }
        set({ list: null, error: errText(err) });
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
        sheet: "open",
        error: isValidReplacement(suggested) ? "" : COPY.invalidReplacement,
      });
    },

    dismissSheet() {
      set({ sheet: "dismissed" });
    },

    async confirmAndCreate() {
      if (!get().plan) {
        return;
      }
      const values = Object.values(get().replacements);
      if (values.some((value) => !isValidReplacement(value))) {
        set({ error: COPY.invalidReplacement, sheet: "open" });
        return;
      }
      set({ sheet: "confirmed" });
      await get().createZip();
    },

    async createZip() {
      if (jobInFlight(get().job)) {
        return;
      }
      const { dropped, plan, replacements, sheet } = get();
      if (dropped.length === 0) {
        set({ error: COPY.noFiles });
        return;
      }
      if (plan && plan.issues.length > 0 && sheet !== "confirmed") {
        set({ sheet: "open", error: COPY.needConfirm });
        return;
      }
      const values = Object.values(replacements);
      if (values.some((value) => !isValidReplacement(value))) {
        set({ error: COPY.invalidReplacement, sheet: "open" });
        return;
      }
      await runJob(
        "create",
        () => getApi().pickSaveZip(destHint(plan, "archive.zip")),
        (dest) => getApi().createArchive({ paths: dropped, replacements }, dest),
        COPY.noDest,
      );
    },

    async setEncoding(encoding) {
      const archive = get().archivePath;
      if (!archive) {
        set({ encodingOverride: encoding });
        return;
      }
      set({ encodingOverride: encoding, error: "" });
      try {
        set({ list: await getApi().listArchive(archive, encoding) });
      } catch (err) {
        set({ error: errText(err) });
      }
    },

    async extractZip() {
      if (jobInFlight(get().job)) {
        return;
      }
      const archive = get().archivePath ?? firstZip(get().dropped);
      if (!archive) {
        set({ error: COPY.noZip });
        return;
      }
      set({ archivePath: archive });
      await runJob(
        "extract",
        () => getApi().pickExtractDir(),
        (dest) =>
          getApi().extractArchive({
            archive,
            dest,
            encoding: get().encodingOverride,
            overwrite: false,
          }),
        COPY.noExtractDest,
      );
    },

    async cancel() {
      bumpEpoch();
      set({ job: null, error: COPY.cancelled });
      await getApi().cancelJob();
    },

    closeJob() {
      set({ job: null });
    },
  };
});
