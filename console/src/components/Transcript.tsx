import { splitPlaceholders } from "../lib/markers";
import type { TranscriptMessage } from "../types";

function formatRedactions(counts: Record<string, number>): string {
  const entries = Object.entries(counts).filter(([, n]) => n > 0);
  if (entries.length === 0) return "";
  entries.sort(([la, a], [lb, b]) => b - a || la.localeCompare(lb));
  return `redacted: ${entries.map(([label, n]) => `${label} ×${n}`).join(", ")}`;
}

export function ScrubbedText({ text }: { text: string }) {
  return (
    <>
      {splitPlaceholders(text).map((part, i) =>
        part.type === "marker" ? (
          <mark key={i} className="marker" title="Removed before storage">
            {part.label}
          </mark>
        ) : (
          <span key={i}>{part.value}</span>
        ),
      )}
    </>
  );
}

export function Transcript({ messages }: { messages: TranscriptMessage[] }) {
  return (
    <ol className="transcript" aria-label="Transcript">
      {messages.map((m, i) => {
        const line = formatRedactions(m.redactions);
        return (
          <li key={i} className={`turn ${m.role}`}>
            <span className="role">{m.role === "visitor" ? "Visitor" : "Persona"}</span>
            <p>
              <ScrubbedText text={m.text} />
            </p>
            {line && <span className="redacted">{line}</span>}
          </li>
        );
      })}
    </ol>
  );
}
