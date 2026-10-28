// The local profile names ("Profile 2 · FPS"), kept on this PC per keyboard
// model (`profile_names` in settings.json; the keyboard doesn't store
// names). Loaded at start and again when another keyboard model is read;
// Settings > Profile names saves them.

import type { Dict } from "../i18n";
import type { ProfileNamesIntent, ProfileNamesView } from "../ipc/bindings";
import { api } from "../ipc/commands";
import { statusStore, useStatus } from "./status";
import { createStore, useStore } from "./store";

/** The profiles of a W75. */
export const PROFILES = [1, 2, 3, 4] as const;

/** The names as the backend gave them (`null` until loaded). */
export const profileNamesStore = createStore<ProfileNamesView | null>(null);

let loading: Promise<void> | null = null;
let watching = false;
let lastProduct: string | null | undefined;

/** The four names (profiles 1-4) of `product` in `view`; `null` = no name. */
export function namesOf(view: ProfileNamesView | null, product: string | null): (string | null)[] {
  const entry = product === null ? undefined : view?.products.find((p) => p.product === product);
  return PROFILES.map((n) => {
    const name = entry?.names[n - 1] ?? null;
    return name === null || name.trim() === "" ? null : name;
  });
}

/** The keyboard the names apply to: the connected one, else the one read last. */
export function currentProduct(view: ProfileNamesView | null, statusProduct: string | null): string | null {
  return statusProduct ?? view?.current_product ?? null;
}

/** "Profile 2 · FPS", or "Profile 2" without a name. */
export function profileLabel(dict: Dict, n: number, name: string | null): string {
  return name === null ? dict.common.profile({ n }) : dict.common.profileNamed({ n, name });
}

/** Reads the names (again); failures leave the numbers alone (a name is never worth an error). */
export function loadProfileNames(): Promise<void> {
  loading ??= api
    .getProfileNames()
    .then((view) => {
      profileNamesStore.set(view);
    })
    .catch(() => {
      // Without names the switcher shows numbers only.
    })
    .finally(() => {
      loading = null;
    });
  return loading;
}

/** Saves one product's names; rejects with an `AppError`. */
export async function saveProfileNames(intent: ProfileNamesIntent): Promise<ProfileNamesView> {
  const view = await api.setProfileNames(intent);
  profileNamesStore.set(view);
  return view;
}

/** Loads the names now and whenever another keyboard model is connected (once per app run). */
export function startProfileNamesSync(): void {
  if (watching) return;
  watching = true;
  lastProduct = statusStore.get().device?.product ?? null;
  statusStore.subscribe(() => {
    const product = statusStore.get().device?.product ?? null;
    if (product !== null && product !== lastProduct) {
      lastProduct = product;
      void loadProfileNames();
    }
  });
  void loadProfileNames();
}

/** The names of the connected keyboard's profiles 1-4; re-renders when they change. */
export function useProfileNames(): (string | null)[] {
  const view = useStore(profileNamesStore);
  const status = useStatus();
  return namesOf(view, currentProduct(view, status.device?.product ?? null));
}

/** Forgets the names (tests). */
export function resetProfileNames(): void {
  profileNamesStore.set(null);
  loading = null;
}
