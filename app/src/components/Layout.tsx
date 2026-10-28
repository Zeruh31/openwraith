// Layout and display primitives: page header, card, key-value list,
// disclosure, technical details, table, profile badge, colour swatch.

import { createContext, useContext, useState, type ReactNode } from "react";

import { useDict } from "../i18n";
import { Icon } from "./Icon";

/** Props of {@link PageHeader}. */
export interface PageHeaderProps {
  title: ReactNode;
  description?: ReactNode | undefined;
  actions?: ReactNode | undefined;
}

/**
 * What a page host puts under every {@link PageHeader} inside it: the shell's
 * tab bar of a page with tabs (`?tab=`), so that a tab's own header gets the
 * tabs right below it.
 */
export const PageHeaderSlot = createContext<ReactNode>(null);

/** The title row of a page (the page's only `h1`), followed by the page's tab bar if it has one. */
export function PageHeader({ title, description, actions }: PageHeaderProps) {
  const slot = useContext(PageHeaderSlot);
  return (
    <>
      <header className="ow-page-header">
        <div className="ow-page-header__text">
          <h1>{title}</h1>
          {description === undefined ? null : <p className="ow-muted">{description}</p>}
        </div>
        {actions === undefined ? null : <div className="ow-page-header__actions">{actions}</div>}
      </header>
      {slot}
    </>
  );
}

/** Props of {@link Card}. */
export interface CardProps {
  title?: ReactNode | undefined;
  /** Heading level of the title (default 2). */
  level?: 2 | 3 | undefined;
  /** Controls at the right of the title. */
  actions?: ReactNode | undefined;
  footer?: ReactNode | undefined;
  className?: string | undefined;
  children?: ReactNode | undefined;
}

/** A surface with an optional title row and footer. */
export function Card({ title, level = 2, actions, footer, className, children }: CardProps) {
  const Heading = level === 2 ? "h2" : "h3";
  return (
    <section className={`ow-card${className === undefined ? "" : ` ${className}`}`}>
      {title === undefined && actions === undefined ? null : (
        <div className="ow-card__header">
          {title === undefined ? <span /> : <Heading className="ow-card__title">{title}</Heading>}
          {actions === undefined ? null : <div className="ow-card__actions">{actions}</div>}
        </div>
      )}
      <div className="ow-card__body">{children}</div>
      {footer === undefined ? null : <div className="ow-card__footer">{footer}</div>}
    </section>
  );
}

/** One row of a {@link KeyValueList}. */
export interface KeyValueItem {
  key: string;
  label: ReactNode;
  value: ReactNode;
}

/** A definition list of labels and values. */
export function KeyValueList({ items }: { items: readonly KeyValueItem[] }) {
  return (
    <dl className="ow-kv">
      {items.map((item) => (
        <div className="ow-kv__row" key={item.key}>
          <dt className="ow-kv__label">{item.label}</dt>
          <dd className="ow-kv__value">{item.value}</dd>
        </div>
      ))}
    </dl>
  );
}

/** Props of {@link Disclosure}. */
export interface DisclosureProps {
  summary: ReactNode;
  defaultOpen?: boolean | undefined;
  className?: string | undefined;
  children?: ReactNode | undefined;
}

/** A native expandable section. */
export function Disclosure({ summary, defaultOpen = false, className, children }: DisclosureProps) {
  return (
    <details className={`ow-disclosure${className === undefined ? "" : ` ${className}`}`} open={defaultOpen}>
      <summary className="ow-disclosure__summary">
        <Icon name="chevron" size={16} className="ow-disclosure__chevron" />
        <span>{summary}</span>
      </summary>
      <div className="ow-disclosure__body">{children}</div>
    </details>
  );
}

/** Technical text (English messages, hex) behind a "Technical details" disclosure. */
export function TechnicalDetails({ children }: { children: ReactNode }) {
  const dict = useDict();
  return (
    <Disclosure summary={dict.common.technicalDetails} className="ow-technical">
      <pre className="ow-technical__text ow-selectable">{children}</pre>
    </Disclosure>
  );
}

/** A column of a {@link Table}. */
export interface TableColumn<Row> {
  key: string;
  header: ReactNode;
  render: (row: Row) => ReactNode;
  align?: "start" | "end" | "center" | undefined;
  /** Renders the cell as the row header (`th scope=row`). */
  rowHeader?: boolean | undefined;
}

/** Props of {@link Table}. */
export interface TableProps<Row> {
  columns: readonly TableColumn<Row>[];
  rows: readonly Row[];
  rowKey: (row: Row) => string;
  caption?: ReactNode | undefined;
  /** Rows shown before "Show all" (default: all). */
  limit?: number | undefined;
  /** Extra class per row. */
  rowClass?: ((row: Row) => string | undefined) | undefined;
}

/** A simple data table with an optional "Show all" limit. */
export function Table<Row>({ columns, rows, rowKey, caption, limit, rowClass }: TableProps<Row>) {
  const dict = useDict();
  const [expanded, setExpanded] = useState(false);
  const shown = limit === undefined || expanded ? rows : rows.slice(0, limit);
  const hidden = rows.length - shown.length;
  return (
    <div className="ow-table-wrap">
      <table className="ow-table">
        {caption === undefined ? null : <caption className="ow-table__caption">{caption}</caption>}
        <thead>
          <tr>
            {columns.map((column) => (
              <th key={column.key} scope="col" className={`ow-align-${column.align ?? "start"}`}>
                {column.header}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {shown.map((row) => (
            <tr key={rowKey(row)} className={rowClass?.(row)}>
              {columns.map((column) =>
                column.rowHeader === true ? (
                  <th key={column.key} scope="row" className={`ow-align-${column.align ?? "start"}`}>
                    {column.render(row)}
                  </th>
                ) : (
                  <td key={column.key} className={`ow-align-${column.align ?? "start"}`}>
                    {column.render(row)}
                  </td>
                ),
              )}
            </tr>
          ))}
        </tbody>
      </table>
      {limit === undefined || rows.length <= limit ? null : (
        <button
          type="button"
          className="ow-link-button"
          onClick={() => {
            setExpanded((value) => !value);
          }}
        >
          {hidden > 0 ? dict.common.showMore({ n: hidden }) : dict.common.actions.showLess}
        </button>
      )}
    </div>
  );
}

/** "Profile 2", or "Profile ?" when unknown. */
export function ProfileBadge({ profile }: { profile: number | null }) {
  const dict = useDict();
  return (
    <span className="ow-profile-badge ow-num">
      {profile === null ? dict.common.profileUnknown : dict.common.profile({ n: profile })}
    </span>
  );
}

/** Props of {@link ColorSwatch}. */
export interface ColorSwatchProps {
  /** "#rrggbb". */
  color: string;
  /** The accessible name; without it the swatch is decorative and the hex is shown. */
  label?: string | undefined;
  size?: "sm" | "md" | undefined;
  /** Show the hex value next to the swatch. */
  showHex?: boolean | undefined;
}

/** A small colour square, with the hex value next to it by default. */
export function ColorSwatch({ color, label, size = "sm", showHex = true }: ColorSwatchProps) {
  return (
    <span className="ow-swatch-wrap">
      <span
        className={`ow-swatch ow-swatch--${size}`}
        style={{ backgroundColor: color }}
        role={label === undefined ? undefined : "img"}
        aria-label={label}
        aria-hidden={label === undefined ? true : undefined}
      />
      {showHex ? <span className="ow-mono ow-num">{color}</span> : null}
    </span>
  );
}
