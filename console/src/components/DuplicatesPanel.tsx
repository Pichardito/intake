import { Link } from "react-router";
import type { Duplicate } from "../types";

export function DuplicatesPanel({ duplicates, suggested }: { duplicates: Duplicate[]; suggested: number | null }) {
  return (
    <section className="panel" aria-labelledby="dups-heading">
      <h3 id="dups-heading">Possible duplicates</h3>
      {duplicates.length === 0 ? (
        <p className="muted">None found among open tickets for this site.</p>
      ) : (
        <ul className="dups">
          {duplicates.map((d) => (
            <li key={d.id}>
              <Link to={`/t/${d.id}`}>#{d.id}</Link> <span>{d.title}</span>{" "}
              <span className="pill">{d.status}</span> <span className="muted">{Math.round(d.score * 100)}%</span>
              {suggested === d.id && <span className="pill suggested">suggested</span>}
              {d.github_issue_url && (
                <a href={d.github_issue_url} target="_blank" rel="noreferrer">
                  issue
                </a>
              )}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
