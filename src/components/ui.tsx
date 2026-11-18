import type { ReactNode } from "react";
import { useState } from "react";
import { RANGE_LABELS, type RangeKey } from "../lib/format";

export function cx(...c: (string | false | null | undefined)[]): string {
  return c.filter(Boolean).join(" ");
}

export function Card({
  title,
  action,
  children,
  className,
}: {
  title?: ReactNode;
  action?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section
      className={cx(
        "rounded-xl border border-slate-200 bg-white p-5 shadow-sm dark:border-slate-800 dark:bg-slate-900",
        className,
      )}
    >
      {(title || action) && (
        <header className="mb-4 flex items-center justify-between gap-3">
          {title && <h2 className="text-sm font-semibold text-slate-700 dark:text-slate-200">{title}</h2>}
          {action}
        </header>
      )}
      {children}
    </section>
  );
}

export function PageHeader({ title, subtitle, actions }: { title: string; subtitle?: ReactNode; actions?: ReactNode }) {
  return (
    <div className="mb-6 flex flex-wrap items-end justify-between gap-4">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">{title}</h1>
        {subtitle && <p className="mt-1 text-sm text-slate-500 dark:text-slate-400">{subtitle}</p>}
      </div>
      {actions && <div className="flex flex-wrap items-center gap-2">{actions}</div>}
    </div>
  );
}

export function Stat({
  label,
  value,
  hint,
  tone = "default",
}: {
  label: string;
  value: ReactNode;
  hint?: ReactNode;
  tone?: "default" | "good" | "bad";
}) {
  return (
    <div className="rounded-xl border border-slate-200 bg-white p-4 shadow-sm dark:border-slate-800 dark:bg-slate-900">
      <div className="text-xs font-medium uppercase tracking-wide text-slate-500 dark:text-slate-400">{label}</div>
      <div className="tabular mt-1.5 text-2xl font-semibold">{value}</div>
      {hint && (
        <div
          className={cx(
            "mt-1 text-xs",
            tone === "good" && "text-emerald-600 dark:text-emerald-400",
            tone === "bad" && "text-rose-600 dark:text-rose-400",
            tone === "default" && "text-slate-500 dark:text-slate-400",
          )}
        >
          {hint}
        </div>
      )}
    </div>
  );
}

export function Segmented<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
}) {
  return (
    <div className="inline-flex rounded-lg border border-slate-200 bg-white p-0.5 dark:border-slate-700 dark:bg-slate-900">
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          onClick={() => onChange(o.value)}
          className={cx(
            "rounded-md px-3 py-1.5 text-sm transition-colors",
            o.value === value
              ? "bg-indigo-600 text-white shadow-sm"
              : "text-slate-600 hover:bg-slate-100 dark:text-slate-300 dark:hover:bg-slate-800",
          )}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function RangePicker({
  value,
  onChange,
  keys = ["today", "yesterday", "7d", "30d"],
}: {
  value: RangeKey;
  onChange: (v: RangeKey) => void;
  keys?: RangeKey[];
}) {
  return <Segmented value={value} onChange={onChange} options={keys.map((k) => ({ value: k, label: RANGE_LABELS[k] }))} />;
}

export function DateInput({ value, onChange, max }: { value: string; onChange: (v: string) => void; max?: string }) {
  return (
    <input
      type="date"
      value={value}
      max={max}
      onChange={(e) => e.target.value && onChange(e.target.value)}
      className="rounded-lg border border-slate-200 bg-white px-2.5 py-1.5 text-sm dark:border-slate-700 dark:bg-slate-900"
    />
  );
}

export function Toggle({ checked, onChange, label }: { checked: boolean; onChange: (v: boolean) => void; label: string }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
      className={cx(
        "relative inline-flex h-6 w-11 shrink-0 items-center rounded-full transition-colors",
        checked ? "bg-indigo-600" : "bg-slate-300 dark:bg-slate-700",
      )}
    >
      <span
        className={cx(
          "inline-block h-5 w-5 rounded-full bg-white shadow transition-transform",
          checked ? "translate-x-5" : "translate-x-0.5",
        )}
      />
    </button>
  );
}

export function Button({
  children,
  onClick,
  variant = "secondary",
  disabled,
  title,
}: {
  children: ReactNode;
  onClick?: () => void;
  variant?: "primary" | "secondary" | "danger";
  disabled?: boolean;
  title?: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      title={title}
      className={cx(
        "inline-flex items-center gap-2 rounded-lg px-3 py-1.5 text-sm font-medium transition-colors disabled:opacity-50",
        variant === "primary" && "bg-indigo-600 text-white hover:bg-indigo-700",
        variant === "secondary" &&
          "border border-slate-200 bg-white text-slate-700 hover:bg-slate-50 dark:border-slate-700 dark:bg-slate-900 dark:text-slate-200 dark:hover:bg-slate-800",
        variant === "danger" && "bg-rose-600 text-white hover:bg-rose-700",
      )}
    >
      {children}
    </button>
  );
}

/** Two-step button: first click arms it, second click confirms. */
export function ConfirmButton({ label, confirmLabel, onConfirm }: { label: string; confirmLabel: string; onConfirm: () => void }) {
  const [armed, setArmed] = useState(false);
  return armed ? (
    <div className="flex items-center gap-2">
      <Button
        variant="danger"
        onClick={() => {
          setArmed(false);
          onConfirm();
        }}
      >
        {confirmLabel}
      </Button>
      <Button onClick={() => setArmed(false)}>Cancel</Button>
    </div>
  ) : (
    <Button onClick={() => setArmed(true)}>{label}</Button>
  );
}

export function AppBadge({ name, color, size = 32 }: { name: string; color: string; size?: number }) {
  return (
    <span
      className="inline-flex shrink-0 items-center justify-center rounded-lg font-semibold text-white"
      style={{ background: color, width: size, height: size, fontSize: size * 0.42 }}
      aria-hidden
    >
      {name.trim().charAt(0).toUpperCase() || "?"}
    </span>
  );
}

export function Bar({ value, max, color }: { value: number; max: number; color: string }) {
  const pct = max > 0 ? Math.max(1.5, (value / max) * 100) : 0;
  return (
    <div className="h-1.5 w-full overflow-hidden rounded-full bg-slate-100 dark:bg-slate-800">
      <div className="h-full rounded-full" style={{ width: `${pct}%`, background: color }} />
    </div>
  );
}

export function Empty({ children }: { children: ReactNode }) {
  return <div className="py-10 text-center text-sm text-slate-500 dark:text-slate-400">{children}</div>;
}

export function ErrorNote({ error }: { error: string | null }) {
  if (!error) return null;
  return (
    <div className="mb-4 rounded-lg border border-rose-200 bg-rose-50 px-4 py-3 text-sm text-rose-700 dark:border-rose-900 dark:bg-rose-950 dark:text-rose-300">
      {error}
    </div>
  );
}
