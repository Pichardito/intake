import { useCallback, useEffect, useMemo, useState } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { useApi } from "../ApiContext";
import { DuplicatesPanel } from "../components/DuplicatesPanel";
import { Transcript } from "../components/Transcript";
import { TriageForm } from "../components/TriageForm";
import { useKeys, type KeyHandlers } from "../hooks/useKeys";
import { useTicketActions } from "../hooks/useTicketActions";
import type { Ticket, TicketDetail as TicketDetailData, TriageFields } from "../types";

const pick = (t: TriageFields): TriageFields => ({
  kind: t.kind,
  title: t.title,
  summary: t.summary,
  steps: t.steps,
  expected: t.expected,
  actual: t.actual,
  severity: t.severity,
});

export function TicketDetail() {
  const api = useApi();
  const navigate = useNavigate();
  const { id: idParam } = useParams();
  const id = Number(idParam);

  const [ticket, setTicket] = useState<TicketDetailData | null>(null);
  const [fields, setFields] = useState<TriageFields | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [neighbours, setNeighbours] = useState<number[]>([]);

  useEffect(() => {
    let cancelled = false;
    api
      .getTicket(id)
      .then((t) => {
        if (cancelled) return;
        setTicket(t);
        setFields(pick(t));
        setError(null);
      })
      .catch((e: unknown) => !cancelled && setError(e instanceof Error ? e.message : "could not load"));
    // Draft tickets of the same list, for j/k navigation.
    api
      .listTickets({ status: "draft" })
      .then((res) => !cancelled && setNeighbours(res.tickets.map((t) => t.id)))
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [api, id]);

  const onDone = useCallback((updated: Ticket) => {
    setTicket((prev) => (prev ? { ...prev, ...updated } : prev));
  }, []);

  const { open, dialog } = useTicketActions({
    ticket,
    ...(fields ? { fields } : {}),
    ...(ticket ? { duplicates: ticket.duplicates } : {}),
    onDone,
  });

  const step = useCallback(
    (delta: number) => {
      const i = neighbours.indexOf(id);
      const next = neighbours[i + delta];
      if (next !== undefined) navigate(`/t/${next}`);
    },
    [neighbours, id, navigate],
  );

  const handlers = useMemo<KeyHandlers>(
    () => ({
      j: () => step(1),
      k: () => step(-1),
      a: () => open("approve"),
      m: () => open("merge"),
      d: () => open("dismiss"),
    }),
    [step, open],
  );
  useKeys(handlers);

  if (error) {
    return (
      <main>
        <p className="error" role="alert">
          {error}
        </p>
        <Link to="/">Back to the queue</Link>
      </main>
    );
  }
  // While navigating j/k between tickets the previous one stays in state until the new one lands.
  if (!ticket || !fields || ticket.id !== id) return <main className="muted">Loading…</main>;

  const isDraft = ticket.status === "draft";

  return (
    <main className="detail">
      <header className="detail-head">
        <Link to="/">← Queue</Link>
        <h1>
          <span className="muted">#{ticket.id}</span> {ticket.title}
        </h1>
        <div className="meta">
          <span className={`pill kind-${ticket.kind}`}>{ticket.kind}</span>
          <span className={`pill status-${ticket.status}`}>{ticket.status}</span>
          <span>{ticket.site}</span>
          <span>confidence {Math.round(ticket.confidence * 100)}%</span>
          <span>severity {ticket.severity}</span>
          {ticket.github_issue_url && (
            <a href={ticket.github_issue_url} target="_blank" rel="noreferrer">
              {ticket.github_issue_url}
            </a>
          )}
          {ticket.status === "merged" && ticket.duplicate_of !== null && <Link to={`/t/${ticket.duplicate_of}`}>merged into #{ticket.duplicate_of}</Link>}
          {ticket.decided_reason && <span className="muted">reason: {ticket.decided_reason}</span>}
        </div>
      </header>

      <div className="columns">
        <section aria-labelledby="transcript-heading">
          <h3 id="transcript-heading">Transcript</h3>
          <Transcript messages={ticket.transcript} />
          <dl className="context">
            <dt>Page</dt>
            <dd>{ticket.page_url ?? <span className="muted">not provided</span>}</dd>
            <dt>Browser</dt>
            <dd>{ticket.ua ?? <span className="muted">not provided</span>}</dd>
            <dt>Viewport</dt>
            <dd>{ticket.viewport ?? <span className="muted">not provided</span>}</dd>
          </dl>
        </section>

        <section>
          <h3>Triage</h3>
          <TriageForm value={fields} onChange={setFields} disabled={!isDraft} />
          <DuplicatesPanel duplicates={ticket.duplicates} suggested={ticket.duplicate_of} />
          {isDraft && (
            <div className="actions">
              <button type="button" className="secondary" onClick={() => open("dismiss")}>
                Dismiss <kbd>d</kbd>
              </button>
              <button type="button" className="secondary" onClick={() => open("merge")}>
                Merge <kbd>m</kbd>
              </button>
              <button type="button" className="primary" onClick={() => open("approve")}>
                Approve <kbd>a</kbd>
              </button>
            </div>
          )}
        </section>
      </div>
      {dialog}
    </main>
  );
}
