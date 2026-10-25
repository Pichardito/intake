// Dismiss reasons are a short fixed list (spec §5) so they can later be
// aggregated to tune the prompts. The id is what the server stores.
export const DISMISS_REASONS = [
  { id: "duplicate", label: "Duplicate" },
  { id: "not-actionable", label: "Not actionable" },
  { id: "noise", label: "Noise or spam" },
  { id: "out-of-scope", label: "Out of scope" },
  { id: "already-fixed", label: "Already fixed" },
  { id: "other", label: "Other" },
] as const;

export type DismissReason = (typeof DISMISS_REASONS)[number]["id"];
