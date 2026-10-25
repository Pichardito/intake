import { describe, expect, it } from "vitest";
import { countsFromSpans, formatRedactions, serverRedactedMore } from "../src/scrub/summary";

describe("countsFromSpans", () => {
  it("counts spans per label", () => {
    expect(
      countsFromSpans([{ label: "SSN" }, { label: "EMAIL" }, { label: "SSN" }]),
    ).toEqual({ SSN: 2, EMAIL: 1 });
  });

  it("returns an empty object for no spans", () => {
    expect(countsFromSpans([])).toEqual({});
  });
});

describe("formatRedactions", () => {
  it("renders the 'redacted: SSN ×1' line", () => {
    expect(formatRedactions({ SSN: 1 })).toBe("redacted: SSN ×1");
  });

  it("lists several labels, highest count first, then alphabetical", () => {
    expect(formatRedactions({ EMAIL: 1, SSN: 2, PHONE: 1 })).toBe(
      "redacted: SSN ×2, EMAIL ×1, PHONE ×1",
    );
  });

  it("returns an empty string when nothing was redacted", () => {
    expect(formatRedactions({})).toBe("");
  });
});

describe("serverRedactedMore", () => {
  it("is false when the counts match", () => {
    expect(serverRedactedMore({ SSN: 1 }, { SSN: 1 })).toBe(false);
  });

  it("is false when the server found fewer (client was stricter)", () => {
    expect(serverRedactedMore({ SSN: 1, EMAIL: 1 }, { SSN: 1 })).toBe(false);
  });

  it("is true when the server found a label the client missed", () => {
    expect(serverRedactedMore({ SSN: 1 }, { SSN: 1, EMAIL: 1 })).toBe(true);
  });

  it("is true when the server found more of the same label", () => {
    expect(serverRedactedMore({ SSN: 1 }, { SSN: 2 })).toBe(true);
  });

  it("tolerates a missing server map", () => {
    expect(serverRedactedMore({ SSN: 1 }, undefined)).toBe(false);
  });
});
