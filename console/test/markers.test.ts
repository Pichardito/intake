import { describe, expect, it } from "vitest";
import { splitPlaceholders } from "../src/lib/markers";

describe("splitPlaceholders", () => {
  it("splits scrubbed text into text and marker parts", () => {
    expect(splitPlaceholders("reach me at [EMAIL] or [PHONE].")).toEqual([
      { type: "text", value: "reach me at " },
      { type: "marker", label: "EMAIL" },
      { type: "text", value: " or " },
      { type: "marker", label: "PHONE" },
      { type: "text", value: "." },
    ]);
  });

  it("recognises numbered placeholders", () => {
    expect(splitPlaceholders("[SSN_1] and [SSN_2]")).toEqual([
      { type: "marker", label: "SSN_1" },
      { type: "text", value: " and " },
      { type: "marker", label: "SSN_2" },
    ]);
  });

  it("leaves ordinary brackets alone", () => {
    expect(splitPlaceholders("array[0] and [lowercase]")).toEqual([
      { type: "text", value: "array[0] and [lowercase]" },
    ]);
  });

  it("returns a single text part for plain text", () => {
    expect(splitPlaceholders("hello")).toEqual([{ type: "text", value: "hello" }]);
  });
});
