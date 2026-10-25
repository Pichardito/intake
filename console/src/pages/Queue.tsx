import { useCallback, useEffect, useMemo, useState } from "react";
import { Link, useNavigate, useSearchParams } from "react-router";
import { useApi } from "../ApiContext";
import { useKeys, type KeyHandlers } from "../hooks/useKeys";
import { useTicketActions } from "../hooks/useTicketActions";
import { KINDS, STATUSES, type QueueFilters, type Site, type Ticket } from "../types";

const CONFIDENCE_OPTIONS = [
  { value: "", label: "any" },
  { value: "0.5", label: "≥ 50%" },
  { value: "0.75", label: "≥ 75%" },
  { value: "0.9", label: "≥ 90%" },
];

export function age(iso: string, now = Date.now()): string {
  const h = Math.max(0, Math.round((now - new Date(iso).getTime()) / 3_600_000));
  if (h < 1) return "just now";
  if (h < 48) return `${h}h`;
  return `${Math.round(h / 24)}d`;
}

export function Queue() {
  const api = useApi();
  const navigate = useNavigate();
  const [params, setParams] = useSearchParams();
  const filters: QueueFilters = useMemo(
    () => ({
      status: params.get("status") ?? "draft",
      kind: params.get("kind") ?? "",
      site: params.get("site") ?? "",
      min_confidence: params.get("min_confidence") ?? "",
    }),
    [params],
  );

  const [tickets, setTickets] = useState<Ticket[] | null>(null);
  const [sites, setSites] = useState<Site[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState(0);
  const [lastIssue, setLastIssue] = useState<Ticket | null>(null);

  const load = useCallback(() => {
    let cancelled = false;
    api
      .listTickets(filters)
      .then((res) => {
        if (cancelled) return;
        setTickets(res.tickets);
        setSites(res.sites);
        setSelected((s) => Math.min(s, Math.max(0, res.tickets.length - 1)));
        setError(null);
      })
      .catch((e: unknown) => !cancelled && setError(e instanceof Error ? e.message : "could not load"));
    return () => {
      cancelled = true;
    };
  }, [api, filters]);

  useEffect(load, [load]);

  const setFilter = (key: keyof QueueFilters, value: string) => {
    const next = new URLSearchParams(params);
    if (value) next.set(key, value);
    else next.delete(key);
    setParams(next);
  };

  const current = tickets?.[selected] ?? null;
  const { open, dialog } = useTicketActions({
    ticket: current,
    onDone: (updated) => {
      if (updated.github_issue_url) setLastIssue(updated);
      load();
    },
  });

  const handlers = useMemo<KeyHandlers>(
    () => ({
      j: () => setSelected((s) => Math.min(s + 1, (tickets?.length ?? 1) - 1)),
      k: () => setSelected((s) => Math.max(s - 1, 0)),
      Enter: () => current && navigate(`/t/${current.id}`),
      a: () => open("approve"),
      m: () => open("merge"),
      d: () => open("dismiss"),
    }),
    [tickets, current, navigate, open],
  );
  useKeys(handlers);

  return (
    <main>
      <div className="filters">
        <label>
          Status
          <select value={filters.status} onChange={(e) => setFilter("status", e.target.value)}>
            <option value="">any</option>
            {STATUSES.map((s) => (
              <option key={s} value={s}>
                {s}
              </option>
            ))}
          </select>
        </label>
        <label>
          Kind
          <select value={filters.kind} onChange={(e) => setFilter("kind", e.target.value)}>
            <option value="">any</option>
            {KINDS.map((k) => (
              <option key={k} value={k}>
                {k}
              </option>
            ))}
          </select>
        </label>
        <label>
          Site
          <select value={filters.site} onChange={(e) => setFilter("site", e.target.value)}>
            <option value="">all</option>
            {sites.map((s) => (
              <option key={s.id} value={String(s.id)}>
                {s.name}
              </option>
            ))}
          </select>
        </label>
        <label>
          Confidence
          <select value={filters.min_confidence} onChange={(e) => setFilter("min_confidence", e.target.value)}>
            {CONFIDENCE_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </select>
        </label>
        <span className="muted keys">
          <kbd>j</kbd>/<kbd>k</kbd> move · <kbd>Enter</kbd> open · <kbd>a</kbd> approve · <kbd>m</kbd> merge · <kbd>d</kbd> dismiss
        </span>
      </div>

      {lastIssue?.github_issue_url && (
        <p className="notice">
          Approved #{lastIssue.id}:{" "}
          <a href={lastIssue.github_issue_url} target="_blank" rel="noreferrer">
            {lastIssue.github_issue_url}
          </a>
        </p>
      )}
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}

      {tickets === null ? (
        <p className="muted">Loading…</p>
      ) : tickets.length === 0 ? (
        <p className="muted">Nothing in the queue for these filters.</p>
      ) : (
        <table className="queue">
          <thead>
            <tr>
              <th>#</th>
              <th>Kind</th>
              <th>Title</th>
              <th>Site</th>
              <th>Confidence</th>
              <th>Status</th>
              <th>Age</th>
            </tr>
          </thead>
          <tbody>
            {tickets.map((t, i) => (
              <tr
                key={t.id}
                aria-selected={i === selected}
                className={i === selected ? "selected" : undefined}
                onClick={() => setSelected(i)}
                onDoubleClick={() => navigate(`/t/${t.id}`)}
              >
                <td className="muted">{t.id}</td>
                <td>
                  <span className={`pill kind-${t.kind}`}>{t.kind}</span>
                </td>
                <td>
                  <Link to={`/t/${t.id}`}>{t.title}</Link>
                  {t.duplicate_of !== null && <span className="muted"> · dup of #{t.duplicate_of}</span>}
                </td>
                <td>{t.site}</td>
                <td>{Math.round(t.confidence * 100)}%</td>
                <td>
                  <span className={`pill status-${t.status}`}>{t.status}</span>
                </td>
                <td className="muted">{age(t.created_at)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {dialog}
    </main>
  );
}
