import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes } from "react-router";
import { describe, expect, it } from "vitest";
import { ApiProvider } from "../src/ApiContext";
import { TicketDetail } from "../src/pages/TicketDetail";
import { detail, fakeApi } from "./fixtures";

function renderDetail(api = fakeApi(), id = 1) {
  render(
    <ApiProvider api={api}>
      <MemoryRouter initialEntries={[`/t/${id}`]}>
        <Routes>
          <Route path="/" element={<div data-testid="queue-probe">queue</div>} />
          <Route path="/t/:id" element={<TicketDetail />} />
        </Routes>
      </MemoryRouter>
    </ApiProvider>,
  );
  return api;
}

describe("TicketDetail", () => {
  it("shows the transcript with redaction markers and per-message counts", async () => {
    renderDetail();
    const transcript = await screen.findByRole("list", { name: /transcript/i });
    const items = within(transcript).getAllByRole("listitem");
    expect(items).toHaveLength(3);
    const marker = within(items[0]!).getByText("EMAIL");
    expect(marker.tagName).toBe("MARK");
    expect(within(items[0]!).getByText(/redacted: EMAIL ×1/)).toBeInTheDocument();
    expect(within(items[1]!).queryByText(/redacted/)).not.toBeInTheDocument();
  });

  it("lists duplicates with their score", async () => {
    renderDetail();
    const panel = await screen.findByRole("region", { name: /duplicates/i });
    expect(within(panel).getByText("Pricing toggle shows the yearly amount twice")).toBeInTheDocument();
    expect(within(panel).getByText("42%")).toBeInTheDocument();
  });

  it("lets the owner edit triage fields and approves with the edited values after a preview", async () => {
    const api = renderDetail();
    const title = await screen.findByLabelText("Title");
    const user = userEvent.setup();
    await user.clear(title);
    await user.type(title, "Billing toggle duplicates yearly total");
    await user.selectOptions(screen.getByLabelText("Severity"), "high");

    await user.click(screen.getByRole("button", { name: /^approve/i }));
    const dialog = await screen.findByRole("dialog", { name: /approve/i });
    expect(within(dialog).getByText("[bug] Billing toggle duplicates yearly total")).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: /create issue/i }));

    await waitFor(() =>
      expect(api.approve).toHaveBeenCalledWith(1, expect.objectContaining({ title: "Billing toggle duplicates yearly total", severity: "high" })),
    );
    expect(await screen.findByRole("link", { name: /issues\/101/ })).toBeInTheDocument();
    expect(screen.getByText("approved")).toBeInTheDocument();
  });

  it("merges into a chosen duplicate", async () => {
    const api = renderDetail();
    await screen.findByLabelText("Title");
    const user = userEvent.setup();
    await user.keyboard("m");
    const dialog = await screen.findByRole("dialog", { name: /merge/i });
    const confirm = within(dialog).getByRole("button", { name: /^merge$/i });
    expect(confirm).toBeDisabled();
    await user.click(within(dialog).getByLabelText(/#3/));
    await user.click(confirm);
    await waitFor(() => expect(api.merge).toHaveBeenCalledWith(1, 3));
    expect(await screen.findByText("merged")).toBeInTheDocument();
  });

  it("dismisses with a reason and optional note", async () => {
    const api = renderDetail();
    await screen.findByLabelText("Title");
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /^dismiss/i }));
    const dialog = await screen.findByRole("dialog", { name: /dismiss/i });
    await user.click(within(dialog).getByLabelText("Not actionable"));
    await user.type(within(dialog).getByLabelText(/note/i), "too vague");
    await user.click(within(dialog).getByRole("button", { name: /^dismiss$/i }));
    await waitFor(() => expect(api.dismiss).toHaveBeenCalledWith(1, "not-actionable", "too vague"));
  });

  it("shows decided tickets read-only", async () => {
    renderDetail(fakeApi({ getTicket: async () => detail({ status: "approved", github_issue_url: "https://github.com/example/repo/issues/42" }) }));
    expect(await screen.findByRole("link", { name: /issues\/42/ })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^approve/i })).not.toBeInTheDocument();
    expect(screen.getByLabelText("Title")).toBeDisabled();
  });

  it("shows the server error when an action fails", async () => {
    renderDetail(fakeApi({ dismiss: async () => { throw new Error("ticket is approved"); } }));
    await screen.findByLabelText("Title");
    const user = userEvent.setup();
    await user.keyboard("d");
    const dialog = await screen.findByRole("dialog", { name: /dismiss/i });
    await user.click(within(dialog).getByLabelText("Noise or spam"));
    await user.click(within(dialog).getByRole("button", { name: /^dismiss$/i }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("ticket is approved");
  });
});
