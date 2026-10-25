import { useState } from "react";
import { DISMISS_REASONS, type DismissReason } from "../lib/dismissReasons";
import { buildIssuePreview } from "../lib/issuePreview";
import type { Duplicate, Ticket, TriageFields } from "../types";
import { Dialog } from "./Dialog";

interface Common {
  busy: boolean;
  error: string | null;
  onClose: () => void;
}

export function ApproveDialog({
  ticket,
  fields,
  queueUrl,
  onConfirm,
  ...rest
}: Common & { ticket: Ticket; fields: TriageFields; queueUrl: string; onConfirm: () => void }) {
  const preview = buildIssuePreview(ticket, fields, queueUrl);
  return (
    <Dialog title={`Approve #${ticket.id}`} onClose={rest.onClose}>
      <p className="muted">This creates a GitHub issue in the site's repository. Review what will be posted.</p>
      <div className="preview">
        <div className="preview-title">{preview.title}</div>
        <div className="labels">
          {preview.labels.map((l) => (
            <span key={l} className="pill">
              {l}
            </span>
          ))}
        </div>
        <pre>{preview.body}</pre>
      </div>
      <Footer {...rest} confirmLabel="Create issue" onConfirm={onConfirm} disabled={!fields.title.trim()} />
    </Dialog>
  );
}

export function MergeDialog({
  ticket,
  duplicates,
  onConfirm,
  ...rest
}: Common & { ticket: Ticket; duplicates: Duplicate[] | null; onConfirm: (into: number) => void }) {
  const [into, setInto] = useState<number | null>(ticket.duplicate_of);
  const [manual, setManual] = useState("");
  const target = manual.trim() ? Number(manual) : into;
  const valid = target !== null && Number.isInteger(target) && target > 0 && target !== ticket.id;
  return (
    <Dialog title={`Merge #${ticket.id} into…`} onClose={rest.onClose}>
      <p className="muted">Adds a comment on the existing issue and closes this ticket as merged.</p>
      {duplicates === null ? (
        <p className="muted">Loading duplicates…</p>
      ) : duplicates.length === 0 ? (
        <p className="muted">No candidates found. Enter a ticket number below.</p>
      ) : (
        <ul className="choices">
          {duplicates.map((d) => (
            <li key={d.id}>
              <label>
                <input type="radio" name="into" checked={into === d.id && !manual.trim()} onChange={() => { setInto(d.id); setManual(""); }} />
                #{d.id} {d.title} <span className="muted">({Math.round(d.score * 100)}%, {d.status})</span>
              </label>
            </li>
          ))}
        </ul>
      )}
      <label>
        Or a ticket number
        <input inputMode="numeric" value={manual} onChange={(e) => setManual(e.target.value)} placeholder="e.g. 12" />
      </label>
      <Footer {...rest} confirmLabel="Merge" onConfirm={() => valid && onConfirm(target)} disabled={!valid} />
    </Dialog>
  );
}

export function DismissDialog({ ticket, onConfirm, ...rest }: Common & { ticket: Ticket; onConfirm: (reason: DismissReason, note?: string) => void }) {
  const [reason, setReason] = useState<DismissReason | null>(null);
  const [note, setNote] = useState("");
  return (
    <Dialog title={`Dismiss #${ticket.id}`} onClose={rest.onClose}>
      <p className="muted">Nothing is created. The reason is kept to tune the prompts later.</p>
      <ul className="choices">
        {DISMISS_REASONS.map((r) => (
          <li key={r.id}>
            <label>
              <input type="radio" name="reason" checked={reason === r.id} onChange={() => setReason(r.id)} />
              {r.label}
            </label>
          </li>
        ))}
      </ul>
      <label>
        Note (optional)
        <input value={note} onChange={(e) => setNote(e.target.value)} />
      </label>
      <Footer {...rest} confirmLabel="Dismiss" onConfirm={() => reason && onConfirm(reason, note.trim() || undefined)} disabled={!reason} />
    </Dialog>
  );
}

function Footer({ busy, error, onClose, onConfirm, confirmLabel, disabled }: Common & { onConfirm: () => void; confirmLabel: string; disabled: boolean }) {
  return (
    <>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      <div className="actions">
        <button type="button" className="secondary" onClick={onClose} disabled={busy}>
          Cancel
        </button>
        <button type="button" className="primary" onClick={onConfirm} disabled={busy || disabled}>
          {busy ? "Working…" : confirmLabel}
        </button>
      </div>
    </>
  );
}
