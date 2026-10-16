import { create } from "zustand";
import { getApi } from "./api";
import { readCompatSkip, writeCompatSkip } from "./compatSkip";
import { COPY } from "./copy";
import {
  defaultReplacements,
  normalizeReplacement,
  normalizeReplacements,
  replacementIsReady,
} from "./replacements";
import type {
  ExtractReport,
  ListReport,
  Mode,
  NameEncoding,
  PlanDto,
  ProgressDto,
  Unmappable,
  WriteReport,
} from "./types";
import { DEFAULT_SPLIT_MB, splitMbToBytes } from "./types";

export type JobKind = "create" | "extract";

export type Job = {
  kind: JobKind;
  progress: ProgressDto;
  dest: string | null;
  usedPassword: boolean;
  usedSplit: boolean;
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
  aesEnabled: boolean;
  password: string;
  splitEnabled: boolean;
  splitPresetMb: 8 | 10 | 25 | 100 | "custom";
  splitCustomMb: string;
  job: Job | null;
  confirming: boolean;
  compatConfirm: boolean;
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
  confirmCompat: (keepPassword: boolean, skipNext: boolean) => Promise<void>;
  createZip: () => Promise<void>;
  setAesEnabled: (on: boolean) => void;
  setPassword: (password: string) => void;
  setSplitEnabled: (on: boolean) => void;
  setSplitPresetMb: (value: 8 | 10 | 25 | 100 | "custom") => void;
  setSplitCustomMb: (value: string) => void;
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

function replacementsOk(replacements: Record<string, string>, issues: Unmappable[]): boolean {
  const paths = new Set(issues.map((issue) => issue.relativePath));
  if (paths.size === 0) {
    return Object.values(replacements).every((value) => replacementIsReady(value, "", issues));
  }
  return [...paths].every((path) => replacementIsReady(replacements[path] ?? "", path, issues));
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
  confirming: false,
  compatConfirm: false,
  error: "",
};

export const useAppStore = create<AppState>((set, get) => {
  const live = (epoch: number) => get().epoch === epoch;

  function isBlocked(): boolean {
    return jobInFlight(get().job) || get().confirming || get().compatConfirm;
  }

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
    flags?: { usedPassword?: boolean; usedSplit?: boolean },
  ): Promise<void> {
    const usedPassword = flags?.usedPassword ?? false;
    const usedSplit = flags?.usedSplit ?? false;
    const epoch = begin({
      error: "",
      compatConfirm: false,
      job: { kind, progress: startingProgress(), dest: null, usedPassword, usedSplit },
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
          usedPassword,
          usedSplit,
        },
        password: kind === "create" ? "" : get().password,
      });
    } catch (err) {
      if (!live(epoch)) {
        return;
      }
      const issues = get().plan?.issues ?? [];
      const reopen = kind === "create" && issues.length > 0;
      const message = errText(err);
      const confirmedStillUnmappable = reopen && message === COPY.needConfirm;
      set({
        error: confirmedStillUnmappable ? COPY.invalidReplacement : message,
        job: null,
        sheet: reopen ? "open" : get().sheet,
      });
    }
  }

  function splitBytesOrError(): number | null | "invalid" | "empty" {
    const { splitEnabled, splitPresetMb, splitCustomMb } = get();
    if (!splitEnabled) {
      return null;
    }
    if (splitPresetMb !== "custom") {
      return splitMbToBytes(splitPresetMb);
    }
    const trimmed = splitCustomMb.trim();
    if (trimmed.length === 0) {
      return "empty";
    }
    const mb = Number(trimmed);
    if (!Number.isSafeInteger(mb) || mb <= 0) {
      return "invalid";
    }
    const bytes = splitMbToBytes(mb);
    if (!Number.isSafeInteger(bytes) || bytes <= 0) {
      return "invalid";
    }
    return bytes;
  }

  function preflightCreate(): boolean {
    const { aesEnabled, password } = get();
    if (aesEnabled && password.length === 0) {
      set({ error: COPY.emptyPassword, job: null, compatConfirm: false });
      return false;
    }
    const split = splitBytesOrError();
    if (split === "empty") {
      set({ error: COPY.splitCustomNeed, job: null, compatConfirm: false });
      return false;
    }
    if (split === "invalid") {
      set({ error: COPY.splitCustomInvalid, job: null, compatConfirm: false });
      return false;
    }
    return true;
  }

  function needsPasswordConfirm(): boolean {
    return get().aesEnabled && !readCompatSkip();
  }

  function startCreate(replacements: Record<string, string>): Promise<void> {
    if (!preflightCreate()) {
      return Promise.resolve();
    }
    const { dropped, plan, aesEnabled, password } = get();
    const split = splitBytesOrError();
    if (split === "empty" || split === "invalid") {
      return Promise.resolve();
    }
    const sentPassword = aesEnabled ? password : null;
    return runJob(
      "create",
      () => getApi().pickSaveZip(destHint(plan, "archive.zip")),
      (dest) => {
        set({ password: "" });
        return getApi().createArchive(
          {
            paths: dropped,
            replacements: normalizeReplacements(replacements),
            password: sentPassword,
            splitBytes: split,
          },
          dest,
        );
      },
      COPY.noDest,
      {
        usedPassword: Boolean(sentPassword),
        usedSplit: split != null,
      },
    );
  }

  return {
    mode: "create",
    epoch: 0,
    aesEnabled: false,
    password: "",
    splitEnabled: false,
    splitPresetMb: DEFAULT_SPLIT_MB,
    splitCustomMb: "",
    ...emptyWork,

    setMode(mode) {
      if (isBlocked()) {
        return;
      }
      begin({
        mode,
        aesEnabled: false,
        password: "",
        splitEnabled: false,
        splitPresetMb: DEFAULT_SPLIT_MB,
        splitCustomMb: "",
        ...emptyWork,
      });
    },

    setAesEnabled(on) {
      if (isBlocked()) {
        return;
      }
      set({
        aesEnabled: on,
        password: on ? get().password : "",
        error: !on && get().error === COPY.emptyPassword ? "" : get().error,
      });
    },

    setPassword(password) {
      if (isBlocked()) {
        return;
      }
      set({
        password,
        error: get().error === COPY.emptyPassword && password.length > 0 ? "" : get().error,
      });
    },

    setSplitEnabled(on) {
      if (isBlocked()) {
        return;
      }
      set({
        splitEnabled: on,
        error:
          !on &&
          (get().error === COPY.splitCustomNeed || get().error === COPY.splitCustomInvalid)
            ? ""
            : get().error,
      });
    },

    setSplitPresetMb(value) {
      if (isBlocked()) {
        return;
      }
      set({
        splitPresetMb: value,
        error:
          get().error === COPY.splitCustomNeed || get().error === COPY.splitCustomInvalid
            ? ""
            : get().error,
      });
    },

    setSplitCustomMb(value) {
      if (isBlocked()) {
        return;
      }
      set({
        splitCustomMb: value,
        error:
          (get().error === COPY.splitCustomNeed || get().error === COPY.splitCustomInvalid) &&
          value.trim().length > 0
            ? ""
            : get().error,
      });
    },

    setProgress(progress) {
      const job = get().job;
      if (!job || job.progress.phase === "done" || progress.phase === "done") {
        return;
      }
      set({ job: { ...job, progress } });
    },

    resetWork() {
      if (isBlocked()) {
        return;
      }
      begin({ ...emptyWork });
    },

    async ingestDropped(paths) {
      if (isBlocked()) {
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
      if (isBlocked()) {
        return;
      }
      const issues = get().plan?.issues ?? [];
      const ready = replacementIsReady(suggested, relativePath, issues);
      set({
        replacements: { ...get().replacements, [relativePath]: suggested },
        sheet: "open",
        error: ready ? "" : COPY.invalidReplacement,
      });
      if (!ready) {
        return;
      }
      void getApi()
        .checkNames([normalizeReplacement(suggested)])
        .then((ok) => {
          if (isBlocked() || get().replacements[relativePath] !== suggested) {
            return;
          }
          if (!ok) {
            set({ error: COPY.invalidReplacement, sheet: "open" });
          }
        })
        .catch(() => undefined);
    },

    dismissSheet() {
      set({ sheet: "dismissed" });
    },

    async confirmAndCreate() {
      if (!get().plan || isBlocked()) {
        return;
      }
      set({ confirming: true });
      try {
        const issues = get().plan?.issues ?? [];
        const replacements = { ...get().replacements };
        if (!replacementsOk(replacements, issues)) {
          set({ error: COPY.invalidReplacement, sheet: "open" });
          return;
        }
        const names = [...new Set(issues.map((issue) => issue.relativePath))].map((path) =>
          normalizeReplacement(replacements[path] ?? ""),
        );
        try {
          if (names.length > 0 && !(await getApi().checkNames(names))) {
            set({ error: COPY.invalidReplacement, sheet: "open" });
            return;
          }
        } catch (err) {
          set({ error: errText(err), sheet: "open" });
          return;
        }
        if (get().sheet === "dismissed") {
          return;
        }
        set({ sheet: "none" });
        if (!preflightCreate()) {
          return;
        }
        if (needsPasswordConfirm()) {
          set({ compatConfirm: true, error: "" });
          return;
        }
        await startCreate(replacements);
      } finally {
        set({ confirming: false });
      }
    },

    async confirmCompat(keepPassword, skipNext) {
      if (!get().compatConfirm) {
        return;
      }
      if (skipNext) {
        writeCompatSkip(true);
      }
      if (!keepPassword) {
        set({
          aesEnabled: false,
          password: "",
          error: get().error === COPY.emptyPassword ? "" : get().error,
        });
      }
      set({ compatConfirm: false });
      await startCreate({ ...get().replacements });
    },

    async createZip() {
      if (isBlocked()) {
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
      if (!preflightCreate()) {
        return;
      }
      if (needsPasswordConfirm()) {
        set({ compatConfirm: true, error: "" });
        return;
      }
      await startCreate({ ...get().replacements });
    },

    async setEncoding(encoding) {
      if (isBlocked()) {
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
      if (isBlocked()) {
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
      begin({ job: null, confirming: false, error: COPY.cancelled });
      try {
        await getApi().cancelJob();
      } catch {
        // 画面は既にキャンセル済み。Tauri の生エラーは出さない。
      }
    },

    closeJob() {
      begin({ job: null });
    },
  };
});
