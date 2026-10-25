import { KINDS, SEVERITIES, type TriageFields } from "../types";

interface TriageFormProps {
  value: TriageFields;
  onChange: (next: TriageFields) => void;
  disabled: boolean;
}

/** Editable triage fields. Steps are one per line. */
export function TriageForm({ value, onChange, disabled }: TriageFormProps) {
  const set = <K extends keyof TriageFields>(key: K, v: TriageFields[K]) => onChange({ ...value, [key]: v });
  const nullable = (s: string) => (s.trim() ? s : null);

  return (
    <form className="triage" onSubmit={(e) => e.preventDefault()}>
      <div className="row">
        <label>
          Kind
          <select value={value.kind} disabled={disabled} onChange={(e) => set("kind", e.target.value as TriageFields["kind"])}>
            {KINDS.map((k) => (
              <option key={k} value={k}>
                {k}
              </option>
            ))}
          </select>
        </label>
        <label>
          Severity
          <select value={value.severity} disabled={disabled} onChange={(e) => set("severity", e.target.value as TriageFields["severity"])}>
            {SEVERITIES.map((s) => (
              <option key={s} value={s}>
                {s}
              </option>
            ))}
          </select>
        </label>
      </div>
      <label>
        Title
        <input value={value.title} disabled={disabled} onChange={(e) => set("title", e.target.value)} />
      </label>
      <label>
        Summary
        <textarea rows={3} value={value.summary} disabled={disabled} onChange={(e) => set("summary", e.target.value)} />
      </label>
      {value.kind === "bug" && (
        <>
          <label>
            Steps (one per line)
            <textarea
              rows={3}
              value={value.steps.join("\n")}
              disabled={disabled}
              onChange={(e) => set("steps", e.target.value.split("\n").map((s) => s.trim()).filter(Boolean))}
            />
          </label>
          <div className="row">
            <label>
              Expected
              <textarea rows={2} value={value.expected ?? ""} disabled={disabled} onChange={(e) => set("expected", nullable(e.target.value))} />
            </label>
            <label>
              Actual
              <textarea rows={2} value={value.actual ?? ""} disabled={disabled} onChange={(e) => set("actual", nullable(e.target.value))} />
            </label>
          </div>
        </>
      )}
    </form>
  );
}
