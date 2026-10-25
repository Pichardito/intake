import { useCallback, useEffect, useState, type ReactNode } from "react";
import { useApi } from "../ApiContext";
import { ApproveDialog, DismissDialog, MergeDialog } from "../components/ActionDialogs";
import type { Duplicate, Ticket, TriageFields } from "../types";

export type Action = "approve" | "merge" | "dismiss";

interface Options {
  ticket: Ticket | null;
  /** Edited fields to approve with; defaults to the ticket's stored fields. */
  fields?: TriageFields;
  /** Duplicates when already loaded (detail page); the hook fetches them otherwise. */
  duplicates?: Duplicate[];
  onDone: (updated: Ticket) => void;
}

export function queueUrlFor(id: number): string {
  return `${window.location.origin}/console/t/${id}`;
}

/** Shared approve / merge / dismiss flow for the queue and the detail page. */
export function useTicketActions({ ticket, fields, duplicates, onDone }: Options): { open: (a: Action) => void; dialog: ReactNode } {
  const api = useApi();
  const [action, setAction] = useState<Action | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [fetched, setFetched] = useState<Duplicate[] | null>(null);

  const close = useCallback(() => {
    setAction(null);
    setError(null);
  }, []);

  const open = useCallback(
    (a: Action) => {
      if (!ticket || ticket.status !== "draft") return;
      setError(null);
      setFetched(null);
      setAction(a);
    },
    [ticket],
  );

  // The queue has no duplicates loaded; fetch them when the merge dialog opens.
  useEffect(() => {
    if (action !== "merge" || duplicates || !ticket) return;
    let cancelled = false;
    api
      .getTicket(ticket.id)
      .then((d) => !cancelled && setFetched(d.duplicates))
      .catch((e: unknown) => !cancelled && setError(e instanceof Error ? e.message : "could not load duplicates"));
    return () => {
      cancelled = true;
    };
  }, [action, duplicates, ticket, api]);

  const run = async (fn: () => Promise<Ticket>) => {
    setBusy(true);
    setError(null);
    try {
      const updated = await fn();
      close();
      onDone(updated);
    } catch (e) {
      setError(e instanceof Error ? e.message : "request failed");
    } finally {
      setBusy(false);
    }
  };

  if (!ticket || !action) return { open, dialog: null };
  const current: TriageFields = fields ?? ticket;
  const common = { busy, error, onClose: close, ticket };

  const dialog =
    action === "approve" ? (
      <ApproveDialog {...common} fields={current} queueUrl={queueUrlFor(ticket.id)} onConfirm={() => run(() => api.approve(ticket.id, current))} />
    ) : action === "merge" ? (
      <MergeDialog {...common} duplicates={duplicates ?? fetched} onConfirm={(into) => run(() => api.merge(ticket.id, into))} />
    ) : (
      <DismissDialog {...common} onConfirm={(reason, note) => run(() => api.dismiss(ticket.id, reason, note))} />
    );

  return { open, dialog };
}
