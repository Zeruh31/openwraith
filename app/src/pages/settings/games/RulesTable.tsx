// The games table: Game | Program | Profile | On | Remove. Below 900 px of
// width it is laid out as cards (CSS container query); the explicit table
// roles keep the semantics when the rows are restyled, and each cell
// carries its column name (`data-label`) for the card layout.

import type { Ref } from "react";

import { Button, IconButton } from "../../../components/Button";
import { Switch } from "../../../components/Controls";
import { Icon } from "../../../components/Icon";
import { useDict } from "../../../i18n";
import type { AutoProfileRuleView } from "../../../ipc/bindings";
import { PROFILES, profileLabel, useProfileNames } from "../../../state/profileNames";
import type { RulePatch } from "./model";

/** Props of {@link RulesTable}. */
export interface RulesTableProps {
  rules: readonly AutoProfileRuleView[];
  /** A save runs: the controls wait. */
  disabled: boolean;
  onChange: (id: string, patch: RulePatch) => void;
  onRemove: (rule: AutoProfileRuleView) => void;
}

/** The stored rules. */
export function RulesTable({ rules, disabled, onChange, onRemove }: RulesTableProps) {
  const dict = useDict();
  const t = dict.settings.games.rules;
  const names = useProfileNames();
  return (
    <div className="st-games">
      <table className="st-games__table" role="table">
        <caption className="sr-only">{t.caption}</caption>
        <thead role="rowgroup">
          <tr role="row">
            <th role="columnheader" scope="col">
              {t.game}
            </th>
            <th role="columnheader" scope="col">
              {t.program}
            </th>
            <th role="columnheader" scope="col">
              {t.profile}
            </th>
            <th role="columnheader" scope="col">
              {t.on}
            </th>
            <th role="columnheader" scope="col">
              <span className="sr-only">{t.remove}</span>
            </th>
          </tr>
        </thead>
        <tbody role="rowgroup">
          {rules.map((rule) => (
            <tr key={rule.id} role="row" className={rule.enabled ? undefined : "st-games__row--off"}>
              <th role="rowheader" scope="row" className="st-games__game" data-label={t.game}>
                {rule.label}
              </th>
              <td role="cell" data-label={t.program}>
                <span className="ow-mono ow-break">{rule.exe}</span>
                <span className="st-games__folder ow-muted ow-small ow-break">{rule.folder ?? t.anyFolder}</span>
                {rule.generic_name ? (
                  <span className="st-games__warn ow-small">
                    <Icon name="warning" size={14} />
                    <span>{t.generic}</span>
                  </span>
                ) : null}
              </td>
              <td role="cell" data-label={t.profile}>
                <select
                  className="ow-input st-games__profile"
                  aria-label={t.profileFor({ label: rule.label })}
                  value={rule.profile}
                  disabled={disabled}
                  onChange={(event) => {
                    onChange(rule.id, { profile: Number(event.currentTarget.value) });
                  }}
                >
                  {PROFILES.map((n) => (
                    <option key={n} value={n}>
                      {profileLabel(dict, n, names[n - 1] ?? null)}
                    </option>
                  ))}
                </select>
              </td>
              <td role="cell" data-label={t.on}>
                <Switch
                  checked={rule.enabled}
                  disabled={disabled}
                  label={<span className="sr-only">{t.ruleOn({ label: rule.label })}</span>}
                  onChange={(enabled) => {
                    onChange(rule.id, { enabled });
                  }}
                />
              </td>
              <td role="cell" className="st-games__remove" data-label={t.remove}>
                <IconButton
                  icon="trash"
                  size="sm"
                  label={t.removeRule({ label: rule.label })}
                  disabled={disabled}
                  onClick={() => {
                    onRemove(rule);
                  }}
                />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/** Props of {@link AddGameButton}. */
export interface AddGameButtonProps {
  disabled: boolean;
  onClick: () => void;
  buttonRef?: Ref<HTMLButtonElement> | undefined;
}

/** "Add a game…". */
export function AddGameButton({ disabled, onClick, buttonRef }: AddGameButtonProps) {
  const dict = useDict();
  return (
    <Button ref={buttonRef} icon="plus" disabled={disabled} onClick={onClick}>
      {dict.settings.games.rules.add}
    </Button>
  );
}
