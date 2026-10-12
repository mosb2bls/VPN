// Small building blocks for the settings window: buttons, fields, notes.
// Ink on paper; colour only for state (UI.md §3). Field errors come from the
// backend's VALIDATION_FAILED `fields[]`, keyed by the form field name.
import { forwardRef, useId, type ButtonHTMLAttributes, type InputHTMLAttributes, type ReactNode } from "react";
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
