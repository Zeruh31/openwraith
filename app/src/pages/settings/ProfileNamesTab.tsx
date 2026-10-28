// Settings > Profile names: local names for the four profiles, per keyboard
// model (the keyboard doesn't store names). The connected model comes
// first; models named before are listed too. Saved with "Save names".

import { useEffect, useId, useState } from "react";

import { Button } from "../../components/Button";
import { EmptyState, Spinner } from "../../components/Feedback";
import { Card } from "../../components/Layout";
import { useDict } from "../../i18n";
import type { ProfileNamesView } from "../../ipc/bindings";
import { toAppError } from "../../ipc/errors";
import {
  currentProduct,
  loadProfileNames,
  namesOf,
  PROFILES,
  profileNamesStore,
  saveProfileNames,
} from "../../state/profileNames";
import { useStatus } from "../../state/status";
import { useStore } from "../../state/store";
import { toast } from "../../state/toasts";

/** The longest name the backend keeps. */
export const NAME_MAX = 32;

/** The products to show: the connected (or last read) one first, then the others with names. */
export function productsToShow(view: ProfileNamesView, connected: string | null): string[] {
  const current = currentProduct(view, connected);
  const others = view.products.map((p) => p.product).filter((p) => p !== current);
  return current === null ? others : [current, ...others];
}

/** One model's four names. */
function ProductNames({ view, product, connected }: { view: ProfileNamesView; product: string; connected: boolean }) {
  const dict = useDict();
  const t = dict.settings.profiles;
  const base = useId();
  const saved = namesOf(view, product).map((name) => name ?? "");
  const savedKey = saved.join("\u0000");
  const [names, setNames] = useState(saved);
  const [shownFor, setShownFor] = useState(savedKey);
  const [saving, setSaving] = useState(false);
  // Names saved meanwhile (here or elsewhere) replace the fields.
  if (shownFor !== savedKey) {
    setShownFor(savedKey);
    setNames(saved);
  }
  const changed = names.some((name, index) => name.trim() !== (saved[index] ?? ""));

  const save = () => {
    setSaving(true);
    saveProfileNames({ product, names: names.map((name) => (name.trim() === "" ? null : name.trim())) })
      .then(() => {
        toast({ kind: "success", title: t.saved });
      })
      .catch((error: unknown) => {
        toast({ kind: "danger", title: t.saveFailed, details: toAppError(error).message });
      })
      .finally(() => {
        setSaving(false);
      });
  };

  return (
    <Card
      title={t.product({ product })}
      actions={connected ? <span className="st-chip">{t.connected}</span> : undefined}
      footer={
        <div className="ow-row">
          <Button variant="primary" disabled={saving || !changed} onClick={save}>
            {t.save}
          </Button>
        </div>
      }
    >
      <form
        className="st-names"
        onSubmit={(event) => {
          event.preventDefault();
          if (changed && !saving) save();
        }}
      >
        {PROFILES.map((n, index) => (
          <div key={n} className="st-names__field">
            <label htmlFor={`${base}-${String(n)}`} className="ow-field__label">
              {t.name({ n })}
            </label>
            <input
              id={`${base}-${String(n)}`}
              type="text"
              className="ow-input"
              value={names[index] ?? ""}
              maxLength={NAME_MAX}
              placeholder={t.placeholder}
              autoComplete="off"
              aria-describedby={`${base}-hint`}
              disabled={saving}
              onChange={(event) => {
                const value = event.currentTarget.value;
                setNames((current) => current.map((name, i) => (i === index ? value : name)));
              }}
            />
          </div>
        ))}
        <p id={`${base}-hint`} className="ow-field__notice st-names__hint">
          {t.hint}
        </p>
      </form>
    </Card>
  );
}

/** Settings > Profile names. */
export function ProfileNamesTab() {
  const dict = useDict();
  const t = dict.settings.profiles;
  const status = useStatus();
  const view = useStore(profileNamesStore);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let alive = true;
    void loadProfileNames().then(() => {
      if (alive && profileNamesStore.get() === null) setFailed(true);
    });
    return () => {
      alive = false;
    };
  }, []);

  if (view === null) {
    return failed ? <EmptyState icon="warning" title={t.loadFailed} /> : <Spinner label={dict.common.loading} size={24} />;
  }
  const connected = status.device?.product ?? null;
  const products = productsToShow(view, connected);
  return (
    <div className="ow-stack">
      <p className="ow-muted">{t.intro}</p>
      {products.length === 0 ? (
        <EmptyState icon="keyboard" title={t.empty} />
      ) : (
        <div className="st-grid">
          {products.map((product) => (
            <ProductNames key={product} view={view} product={product} connected={product === connected} />
          ))}
        </div>
      )}
    </div>
  );
}
