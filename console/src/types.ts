// Shapes returned by the admin API (phase2-spec §3.3 / §5.2). Mirrored by widget/mock.
export type Kind = "bug" | "feature" | "feedback" | "praise" | "noise";
export type Severity = "low" | "medium" | "high";
export type TicketStatus = "draft" | "approved" | "merged" | "dismissed";

export const KINDS: readonly Kind[] = ["bug", "feature", "feedback", "praise", "noise"];
export const SEVERITIES: readonly Severity[] = ["low", "medium", "high"];
export const STATUSES: readonly TicketStatus[] = ["draft", "approved", "merged", "dismissed"];

export interface Site {
  id: number;
  name: string;
}

/** The fields the owner may edit before approving. */
export interface TriageFields {
  kind: Kind;
  title: string;
  summary: string;
  steps: string[];
  expected: string | null;
  actual: string | null;
  severity: Severity;
}

export interface Ticket extends TriageFields {
  id: number;
  site_id: number;
  site: string;
  conversation_id: string;
  confidence: number;
  duplicate_of: number | null;
  status: TicketStatus;
  github_issue_url: string | null;
  created_at: string;
  decided_at: string | null;
  decided_reason: string | null;
  page_url: string | null;
  ua: string | null;
  viewport: string | null;
}

export interface TranscriptMessage {
  role: "visitor" | "assistant";
  /** Scrubbed text; identifiers appear as `[LABEL]` placeholders. */
  text: string;
  redactions: Record<string, number>;
  created_at: string;
}

export interface Duplicate {
  id: number;
  title: string;
  status: TicketStatus;
  github_issue_url: string | null;
  score: number;
}

export interface TicketDetail extends Ticket {
  transcript: TranscriptMessage[];
  duplicates: Duplicate[];
}

export interface QueueFilters {
  site?: string;
  status?: string;
  kind?: string;
  min_confidence?: string;
}

export interface QueueResponse {
  tickets: Ticket[];
  sites: Site[];
}
