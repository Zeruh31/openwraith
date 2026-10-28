// The keymap group of the last full read (`get_keys_state`: no keyboard
// traffic), refetched whenever the status announces a new snapshot revision,
// and the macro library of this PC. Shared by the three Keys tabs.

import { useEffect } from "react";

import { useDict, type Dict } from "../../../i18n";
import type { AppError, DeviceStatus, KeysState, MacroLibraryView, MacroProgramView } from "../../../ipc/bindings";
import { api } from "../../../ipc/commands";
import { toAppError } from "../../../ipc/errors";
import { busyText, tableName, writeBlockText } from "../../../flow/labels";
import { statusStore, useStatus } from "../../../state/status";
import { createStore, useStore } from "../../../state/store";
import { toast } from "../../../state/toasts";

interface KeysStoreState {
  state: KeysState | null;
  /**
   * The last state read, kept when the keys state goes away (a lost
   * connection, a reset): a write flow opened on it stays mounted with its
   * progress or report, and the page shows it read-only meanwhile.
   */
  last: KeysState | null;
  /** The status's snapshot revision the state was fetched for. */
  fetchedFor: number | null;
  loading: boolean;
  error: AppError | null;
}

const EMPTY: KeysStoreState = { state: null, last: null, fetchedFor: null, loading: false, error: null };

/** The keys state (tests set it directly). */
export const keysStore = createStore<KeysStoreState>(EMPTY);

let requested: number | null = null;

/** Fetches the keys state when the status's snapshot revision differs from the one it was fetched for. */
export function syncKeys(): void {
  const revision = statusStore.get().snapshot_revision;
  const current = keysStore.get();
  if (revision === null) {
    requested = null;
    if (current.state !== null || current.loading || current.fetchedFor !== null) {
      keysStore.set({ ...EMPTY, last: current.state ?? current.last });
    }
    return;
  }
  if (current.fetchedFor === revision || requested === revision) return;
  requested = revision;
  keysStore.set({ ...current, loading: true });
  api
    .getKeysState()
    .then((state) => {
      if (requested !== revision) return;
      requested = null;
      keysStore.set({ state, last: state, fetchedFor: revision, loading: false, error: null });
    })
    .catch((error: unknown) => {
      if (requested !== revision) return;
      requested = null;
      keysStore.set({ ...keysStore.get(), fetchedFor: revision, loading: false, error: toAppError(error) });
    });
}

let started = false;

/** Keeps the keys state in step with the status (once per app run; started by the first Keys view). */
export function startKeysSync(): void {
  if (started) return;
  started = true;
  statusStore.subscribe(syncKeys);
  syncKeys();
}

/** Fetches the keys state again (e.g. after a refused plan that said the view was out of date). */
export function refetchKeys(): void {
  keysStore.set({ ...keysStore.get(), fetchedFor: null });
  requested = null;
  syncKeys();
}

/** Forgets the keys state (tests). */
export function resetKeysStore(): void {
  requested = null;
  keysStore.set(EMPTY);
  keysFlowCount.set(0);
  macroLibraryStore.set(LIBRARY_EMPTY);
  libraryRequested = false;
}

/** The keys state as the tabs see it. */
export interface KeysView {
  state: KeysState | null;
  /** The last state read (see `KeysStoreState.last`). */
  last: KeysState | null;
  loading: boolean;
  error: AppError | null;
  /** Another program changed the keyboard since the read; writes are blocked. */
  stale: boolean;
}

/** The keys state; starts the sync on first use. */
export function useKeysState(): KeysView {
  const store = useStore(keysStore);
  const status = useStatus();
  useEffect(() => {
    startKeysSync();
  }, []);
  return { state: store.state, last: store.last, loading: store.loading, error: store.error, stale: status.snapshot_stale };
}

/** How many Keys write flows are open (each one counts itself while mounted). */
export const keysFlowCount = createStore(0);

/** Whether a Keys write flow is open. */
export function useKeysFlowOpen(): boolean {
  return useStore(keysFlowCount) > 0;
}

/**
 * Why a key-assignment change can't be reviewed and written now, or `null`
 * (tabs add their own reasons, such as "nothing changed").
 */
export function keysBlockReason(view: KeysView, status: DeviceStatus, dict: Dict): string | null {
  const state = view.state;
  if (state === null) return dict.errors.writeBlocked.not_connected;
  if (!state.editable) {
    const disabled = state.disabled;
    if (disabled?.reason === "unreadable_table") {
      return dict.keys.disabled.unreadable_table({ table: tableName(disabled.table, dict) });
    }
    return dict.keys.disabled.unsupported;
  }
  if (view.stale) return dict.errors.writeBlocked.snapshot_stale;
  if (!status.write_gate.allowed) return writeBlockText(status.write_gate.reasons[0] ?? "not_connected", status, dict);
  if (status.busy !== null) return busyText(status, dict);
  return null;
}

/** {@link keysBlockReason} in a component. */
export function useKeysBlockReason(view: KeysView): string | null {
  const dict = useDict();
  const status = useStatus();
  return keysBlockReason(view, status, dict);
}

// ---------- the macro library (this PC) ----------

interface LibraryState {
  library: MacroLibraryView | null;
  loading: boolean;
  error: AppError | null;
}

const LIBRARY_EMPTY: LibraryState = { library: null, loading: false, error: null };

/** The macro library (tests set it directly). */
export const macroLibraryStore = createStore<LibraryState>(LIBRARY_EMPTY);

let libraryRequested = false;

function acceptLibrary(library: MacroLibraryView, dict: Dict): MacroLibraryView {
  macroLibraryStore.set({ library, loading: false, error: null });
  if (library.notice?.notice === "invalid_file_kept") {
    toast({
      kind: "warning",
      title: dict.keys.library.invalidKept,
      body: dict.keys.library.invalidKeptBody({ file: library.notice.kept_as }),
    });
  }
  return library;
}

function libraryFailed(error: unknown): never {
  const appError = toAppError(error);
  macroLibraryStore.set({ ...macroLibraryStore.get(), loading: false, error: appError });
  throw appError;
}

/** Loads the macro library once (again with `force`). */
export function loadMacroLibrary(dict: Dict, force = false): void {
  if (libraryRequested && !force) return;
  libraryRequested = true;
  macroLibraryStore.set({ ...macroLibraryStore.get(), loading: true });
  api
    .listMacroLibrary()
    .then((library) => acceptLibrary(library, dict))
    .catch((error: unknown) => {
      macroLibraryStore.set({ ...macroLibraryStore.get(), loading: false, error: toAppError(error) });
    });
}

/** The macro library; loads it on first use. */
export function useMacroLibrary(): LibraryState {
  const dict = useDict();
  const state = useStore(macroLibraryStore);
  useEffect(() => {
    loadMacroLibrary(dict);
  }, [dict]);
  return state;
}

/** Saves a macro under a name (an existing identical program is renamed). */
export function saveLibraryMacro(name: string, program: MacroProgramView, dict: Dict): Promise<MacroLibraryView> {
  return api.saveLibraryMacro(name, program).then((library) => acceptLibrary(library, dict), libraryFailed);
}

/** Renames a library macro. */
export function renameLibraryMacro(hash: string, name: string, dict: Dict): Promise<MacroLibraryView> {
  return api.renameLibraryMacro(hash, name).then((library) => acceptLibrary(library, dict), libraryFailed);
}

/** Deletes a library macro. */
export function deleteLibraryMacro(hash: string, dict: Dict): Promise<MacroLibraryView> {
  return api.deleteLibraryMacro(hash).then((library) => acceptLibrary(library, dict), libraryFailed);
}

/** Library names by content hash (to name the keyboard's macro slots). */
export function libraryNames(library: MacroLibraryView | null): ReadonlyMap<string, string> {
  return new Map((library?.entries ?? []).map((entry) => [entry.content_hash, entry.name]));
}
