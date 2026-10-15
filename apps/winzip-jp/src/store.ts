import { create } from "zustand";
import { getApi } from "./api";
import { COPY } from "./copy";
import { defaultReplacements, isValidReplacement, normalizeReplacements } from "./replacements";
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
};

export type Sheet = "none" | "open" | "dismissed";

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

function replacementsOk(replacements: Record<string, string>): boolean {
  return Object.values(replacements).every(isValidReplacement);
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
  const live = (epoch: number) => get().epoch === epoch;

  function begin(patch: Partial<AppState>): number {
    const epoch = get().epoch + 1;
    set({ ...patch, epoch });
    return epoch;
  }

  async function runJob(
    kind: JobKind,
    pickDest: () => Promise<string | null>,
    run: (dest: string) => Promise<WriteReport | ExtractReport>,
    noDest: string,
  ): Promise<void> {
    const epoch = begin({
      error: "",
      job: { kind, progress: startingProgress(), dest: null },
    });
    try {
      const dest = await pickDest();
      if (!live(epoch)) {
        return;
      }
      if (!dest) {
        set({ error: noDest, job: null });
        return;
      }
      const report = await run(dest);
      if (!live(epoch)) {
        return;
      }
      set({
        job: {
          kind,
          dest: report.dest,
          progress: doneProgress(report.entriesWritten, report.bytesWritten),
        },
      });
    } catch (err) {
      if (!live(epoch)) {
        return;
      }
      const reopen = kind === "create" && (get().plan?.issues.length ?? 0) > 0;
      set({ error: errText(err), job: null, sheet: reopen ? "open" : get().sheet });
    }
  }

  function startCreate(): Promise<void> {
    const { dropped, plan, replacements } = get();
    return runJob(
      "create",
      () => getApi().pickSaveZip(destHint(plan, "archive.zip")),
      (dest) =>
        getApi().createArchive({ paths: dropped, replacements: normalizeReplacements(replacements) }, dest),
      COPY.noDest,
    );
  }

  return {
    mode: "create",
    epoch: 0,
    ...emptyWork,

    setMode(mode) {
      if (jobInFlight(get().job)) {
        return;
      }
      begin({ mode, ...emptyWork });
    },

    setProgress(progress) {
      const job = get().job;
      if (!job || job.progress.phase === "done" || progress.phase === "done") {
        return;
      }
      set({ job: { ...job, progress } });
    },

    resetWork() {
      if (jobInFlight(get().job)) {
        return;
      }
      begin({ ...emptyWork });
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
      const epoch = begin({ ...emptyWork, dropped: usable, error: "" });
      const api = getApi();
      if (get().mode === "create") {
        try {
          const plan = await api.planArchive(usable);
          if (!live(epoch)) {
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
          if (!live(epoch)) {
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
        if (!live(epoch)) {
          return;
        }
        set({ list, plan: null, sheet: "none", archivePath: archive });
      } catch (err) {
        if (!live(epoch)) {
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
      if (!get().plan || jobInFlight(get().job)) {
        return;
      }
      if (!replacementsOk(get().replacements)) {
        set({ error: COPY.invalidReplacement, sheet: "open" });
        return;
      }
      set({ sheet: "none" });
      await startCreate();
    },

    async createZip() {
      if (jobInFlight(get().job)) {
        return;
      }
      const { dropped, plan } = get();
      if (dropped.length === 0) {
        set({ error: COPY.noFiles });
        return;
      }
      if (plan && plan.issues.length > 0) {
        set({ sheet: "open", error: COPY.needConfirm });
        return;
      }
      await startCreate();
    },

    async setEncoding(encoding) {
      if (jobInFlight(get().job)) {
        return;
      }
      const archive = get().archivePath;
      const previous = get().encodingOverride;
      if (!archive) {
        set({ encodingOverride: encoding });
        return;
      }
      const epoch = begin({ encodingOverride: encoding, error: "" });
      try {
        const list = await getApi().listArchive(archive, encoding);
        if (!live(epoch) || get().archivePath !== archive) {
          return;
        }
        set({ list });
      } catch (err) {
        if (!live(epoch) || get().archivePath !== archive) {
          return;
        }
        set({ error: errText(err), encodingOverride: previous });
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
      begin({ job: null, error: COPY.cancelled });
      await getApi().cancelJob();
    },

    closeJob() {
      begin({ job: null });
    },
  };
});
