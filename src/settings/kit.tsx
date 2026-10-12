// Small building blocks for the settings window: buttons, fields, notes.
// Ink on paper; colour only for state (UI.md §3). Field errors come from the
// backend's VALIDATION_FAILED `fields[]`, keyed by the form field name.
import { forwardRef, useId, useState, type ButtonHTMLAttributes, type InputHTMLAttributes, type ReactNode } from "react";
import type { AppError, FieldError } from "../api/bindings";

type Variant = "solid" | "outline" | "ghost";

export const Button = forwardRef<HTMLButtonElement, ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant; busy?: boolean }>(
  function Button({ variant = "outline", busy = false, className = "", children, disabled, ...rest }, ref) {
    const v =
      variant === "solid"
        ? "border-ink bg-ink text-paper hover:bg-ink/90 disabled:border-ink/30 disabled:bg-ink/30"
        : variant === "outline"
          ? "border-ink text-ink hover:bg-ink/5 disabled:border-ink/30 disabled:text-ink-muted"
          : "border-transparent text-ink underline-offset-4 hover:underline disabled:text-ink-muted";
    return (
      <button
        ref={ref}
        type="button"
        disabled={disabled || busy}
        aria-busy={busy || undefined}
        className={`label inline-flex h-8 items-center justify-center gap-2 border px-3 transition-[background-color,color,transform] duration-fast active:translate-y-px focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cobalt ${v} ${className}`}
        {...rest}
      >
        {children}
      </button>
    );
  },
);

export function SectionHeader({ index, title, children }: { index: string; title: string; children?: ReactNode }) {
  return (
    <header className="flex flex-col gap-1">
      <span className="label text-ink-muted">{index}</span>
      <div className="flex items-end justify-between gap-4">
        <h2 className="font-display text-[26px] leading-8 font-[850] [font-stretch:82%] uppercase">{title}</h2>
        {children && <div className="flex shrink-0 gap-2 pb-0.5">{children}</div>}
      </div>
    </header>
  );
}

export function fieldErrorFor(error: AppError | null, field: string): FieldError | undefined {
  return error?.fields.find((f) => f.field === field);
}

export function Field({
  label,
  help,
  error,
  children,
  htmlFor,
  optional,
}: {
  label: string;
  help?: ReactNode;
  error?: FieldError;
  children: ReactNode;
  htmlFor?: string;
  optional?: boolean;
}) {
  return (
    <div className="flex flex-col gap-1">
      <label htmlFor={htmlFor} className="label text-ink-muted">
        {label}
        {optional && <span className="normal-case tracking-normal"> · optional</span>}
      </label>
      {children}
      {error ? (
        <p className="text-secondary text-vermilion-text" role="alert">
          {error.message}
        </p>
      ) : (
        help && <p className="text-secondary text-ink-muted">{help}</p>
      )}
    </div>
  );
}

export const TextInput = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement> & { invalid?: boolean; mono?: boolean }>(
  function TextInput({ invalid, mono, className = "", ...rest }, ref) {
    return (
      <input
        ref={ref}
        aria-invalid={invalid || undefined}
        className={`h-8 w-full border bg-paper-raised px-2 text-body outline-none transition-colors duration-fast placeholder:text-ink-muted/70 focus:border-ink focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-cobalt ${
          invalid ? "border-vermilion" : "border-line"
        } ${mono ? "font-mono text-[12px]" : ""} ${className}`}
        {...rest}
      />
    );
  },
);

export function Checkbox({ checked, onChange, label, help }: { checked: boolean; onChange: (v: boolean) => void; label: ReactNode; help?: ReactNode }) {
  const id = useId();
  return (
    <div className="flex gap-2.5">
      <input
        id={id}
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
        className="mt-0.5 size-4 shrink-0 accent-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cobalt"
      />
      <label htmlFor={id} className="flex flex-col gap-0.5">
        <span className="text-body">{label}</span>
        {help && <span className="text-secondary text-ink-muted">{help}</span>}
      </label>
    </div>
  );
}

/** Bar + ink text on paper (D14). */
export function Note({ tone = "info", title, children }: { tone?: "info" | "warn" | "error" | "ok"; title?: ReactNode; children?: ReactNode }) {
  const bar = tone === "error" ? "bg-vermilion" : tone === "warn" ? "bg-mustard" : tone === "ok" ? "bg-cobalt" : "bg-ink";
  return (
    <div className="flex gap-3" role={tone === "error" ? "alert" : "status"}>
      <span aria-hidden className={`w-[3px] shrink-0 ${bar}`} />
      <div className="flex min-w-0 flex-col gap-1 py-0.5">
        {title && <p className="text-body font-medium">{title}</p>}
        {children && <div className="text-secondary text-ink">{children}</div>}
      </div>
    </div>
  );
}

/** A command error, with its redacted detail behind a disclosure. */
export function ErrorNote({ error, title }: { error: AppError; title?: string }) {
  return (
    <Note tone={error.code === "NOT_IMPLEMENTED" ? "warn" : "error"} title={title ?? error.message}>
      {title && <p>{error.message}</p>}
      {error.detail && (
        <details className="mt-1">
          <summary className="label cursor-pointer text-ink-muted">Details</summary>
          <p className="mt-1 font-mono text-[11px] break-all text-ink-muted">
            {error.code} · {error.detail}
          </p>
        </details>
      )}
    </Note>
  );
}

/** Accessible on/off switch: ink track when on. */
export function Switch({ checked, onChange, label, disabled }: { checked: boolean; onChange: (v: boolean) => void; label: string; disabled?: boolean }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={`relative h-5 w-9 shrink-0 border border-ink transition-colors duration-base ease-enter disabled:opacity-40 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cobalt ${
        checked ? "bg-ink" : "bg-paper-raised"
      }`}
    >
      <span
        aria-hidden
        className={`absolute top-[2px] left-[2px] size-[14px] transition-transform duration-base ease-enter ${checked ? "translate-x-4 bg-paper" : "bg-ink"}`}
      />
    </button>
  );
}

/** One setting: title + explanation on the left, control on the right. */
export function SettingRow({ title, children, control, tone }: { title: ReactNode; children?: ReactNode; control?: ReactNode; tone?: "warn" }) {
  return (
    <div className="flex items-start justify-between gap-6 border-b border-line py-4 last:border-b-0">
      <div className="flex min-w-0 flex-col gap-1">
        <span className={`text-body font-medium ${tone === "warn" ? "text-mustard-text" : ""}`}>{title}</span>
        {children && <div className="text-secondary text-ink-muted">{children}</div>}
      </div>
      {control && <div className="shrink-0 pt-0.5">{control}</div>}
    </div>
  );
}

const UNITS = { s: 1, min: 60, h: 3600 } as const;
type Unit = keyof typeof UNITS;
const bestUnit = (secs: number): Unit => (secs % 3600 === 0 && secs >= 3600 ? "h" : secs % 60 === 0 && secs >= 60 ? "min" : "s");

/** Seconds value edited as number + unit (s / min / h). */
export function DurationInput({ id, value, onChange, invalid, units = ["s", "min", "h"] }: { id?: string; value: number; onChange: (secs: number) => void; invalid?: boolean; units?: Unit[] }) {
  const [unit, setUnit] = useState<Unit>(() => (units.includes(bestUnit(value)) ? bestUnit(value) : units[0]));
  const shown = value / UNITS[unit];
  return (
    <div className="flex">
      <TextInput
        id={id}
        mono
        inputMode="numeric"
        className="w-20"
        invalid={invalid}
        value={Number.isFinite(shown) ? String(+shown.toFixed(2)) : ""}
        onChange={(e) => onChange(Math.round(Number(e.target.value) * UNITS[unit]))}
      />
      <select
        aria-label="Unit"
        value={unit}
        onChange={(e) => setUnit(e.target.value as Unit)}
        className="h-8 border border-l-0 border-line bg-paper-raised px-1 font-mono text-[12px] focus-visible:outline-2 focus-visible:outline-cobalt"
      >
        {units.map((u) => (
          <option key={u}>{u}</option>
        ))}
      </select>
    </div>
  );
}
