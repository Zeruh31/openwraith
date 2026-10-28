// An ordered list of steps (a macro's key presses and releases) that can be
// reordered: each step has Move up / Move down / Remove buttons, and Alt +
// Up / Alt + Down moves the step that holds the focus (the focus moves with
// it). Every move and removal is announced in a polite live region. The
// caller renders each step's content and keeps the list.

import { useEffect, useId, useRef, useState, type KeyboardEvent, type ReactNode } from "react";

import { useDict } from "../i18n";
import { IconButton } from "./Button";

/** Props of {@link StepList}. */
export interface StepListProps<T> {
  /** The accessible name of the list. */
  label: string;
  items: readonly T[];
  itemKey: (item: T, index: number) => string;
  /** The step's visible content (fields, names). */
  renderItem: (item: T, index: number) => ReactNode;
  /** The step's accessible name ("Step 3: A down, then 30 ms"). */
  itemLabel: (item: T, index: number) => string;
  onMove: (from: number, to: number) => void;
  onRemove?: ((index: number) => void) | undefined;
  disabled?: boolean | undefined;
  /** Shown when there are no steps. */
  emptyText?: ReactNode | undefined;
}

/** A reorderable list of steps. */
export function StepList<T>({
  label,
  items,
  itemKey,
  renderItem,
  itemLabel,
  onMove,
  onRemove,
  disabled = false,
  emptyText,
}: StepListProps<T>) {
  const dict = useDict();
  const t = dict.common.pickers.steps;
  const hintId = useId();
  const refs = useRef(new Map<string, HTMLLIElement>());
  const [announcement, setAnnouncement] = useState("");
  const focusKey = useRef<string | null>(null);

  // After a move, the moved step keeps the focus.
  useEffect(() => {
    const key = focusKey.current;
    if (key === null) return;
    focusKey.current = null;
    refs.current.get(key)?.focus();
  });

  const move = (from: number, to: number) => {
    const item = items[from];
    if (disabled || item === undefined || to < 0 || to >= items.length || to === from) return;
    onMove(from, to);
    setAnnouncement(t.moved({ from: from + 1, to: to + 1 }));
    focusKey.current = itemKey(item, from);
  };

  const remove = (index: number) => {
    if (disabled || onRemove === undefined) return;
    onRemove(index);
    setAnnouncement(t.removed({ n: index + 1 }));
  };

  const onKeyDown = (event: KeyboardEvent<HTMLLIElement>, index: number) => {
    if (!event.altKey) return;
    if (event.key === "ArrowUp") {
      event.preventDefault();
      move(index, index - 1);
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      move(index, index + 1);
    }
  };

  return (
    <div className="ow-steps">
      {items.length === 0 ? (
        <p className="ow-muted ow-steps__empty">{emptyText ?? t.empty}</p>
      ) : (
        <ol className="ow-steps__list" aria-label={label} aria-describedby={hintId}>
          {items.map((item, index) => {
            const key = itemKey(item, index);
            const name = itemLabel(item, index);
            return (
              <li
                key={key}
                ref={(element) => {
                  if (element === null) refs.current.delete(key);
                  else refs.current.set(key, element);
                }}
                className="ow-steps__item"
                tabIndex={-1}
                aria-label={`${name} (${t.position({ n: index + 1, total: items.length })})`}
                onKeyDown={(event) => {
                  onKeyDown(event, index);
                }}
              >
                <span className="ow-steps__number ow-num" aria-hidden="true">
                  {index + 1}
                </span>
                <div className="ow-steps__content">{renderItem(item, index)}</div>
                <div className="ow-steps__actions">
                  <IconButton
                    icon="arrow-up"
                    size="sm"
                    label={`${t.moveUp}: ${name}`}
                    disabled={disabled || index === 0}
                    onClick={() => {
                      move(index, index - 1);
                    }}
                  />
                  <IconButton
                    icon="arrow-down"
                    size="sm"
                    label={`${t.moveDown}: ${name}`}
                    disabled={disabled || index === items.length - 1}
                    onClick={() => {
                      move(index, index + 1);
                    }}
                  />
                  {onRemove === undefined ? null : (
                    <IconButton
                      icon="trash"
                      size="sm"
                      label={`${t.remove}: ${name}`}
                      disabled={disabled}
                      onClick={() => {
                        remove(index);
                      }}
                    />
                  )}
                </div>
              </li>
            );
          })}
        </ol>
      )}
      <p id={hintId} className="ow-small ow-muted">
        {t.hint}
      </p>
      <p className="sr-only" role="status" aria-live="polite">
        {announcement}
      </p>
    </div>
  );
}
