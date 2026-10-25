import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes, useParams } from "react-router";
import { describe, expect, it } from "vitest";
import { ApiProvider } from "../src/ApiContext";
import { Queue } from "../src/pages/Queue";
import { fakeApi } from "./fixtures";

function DetailProbe() {
  const { id } = useParams();
  return <div data-testid="detail-probe">detail {id}</div>;
}

function renderQueue(api = fakeApi(), initial = "/") {
  render(
    <ApiProvider api={api}>
      <MemoryRouter initialEntries={[initial]}>
        <Routes>
          <Route path="/" element={<Queue />} />
          <Route path="/t/:id" element={<DetailProbe />} />
        </Routes>
      </MemoryRouter>
    </ApiProvider>,
  );
  return api;
}

describe("Queue", () => {
  it("lists draft tickets by default with kind, site and confidence", async () => {
    const api = renderQueue();
    expect(await screen.findByText("Pricing toggle shows yearly twice")).toBeInTheDocument();
    expect(api.listTickets).toHaveBeenCalledWith(expect.objectContaining({ status: "draft" }));
    const rows = screen.getAllByRole("row").slice(1); // skip header
    expect(rows).toHaveLength(3);
    expect(within(rows[0]!).getByText("bug")).toBeInTheDocument();
    expect(within(rows[0]!).getByText("Morning Queue site")).toBeInTheDocument();
    expect(within(rows[0]!).getByText("86%")).toBeInTheDocument();
    expect(within(rows[2]!).getByText(/dup of #1/)).toBeInTheDocument();
  });

  it("re-queries when a filter changes and reflects it in the URL", async () => {
    const api = renderQueue();
    await screen.findByText("Pricing toggle shows yearly twice");
    const user = userEvent.setup();
    await user.selectOptions(screen.getByLabelText("Kind"), "feature");
    await waitFor(() => expect(api.listTickets).toHaveBeenLastCalledWith(expect.objectContaining({ kind: "feature", status: "draft" })));
    await user.selectOptions(screen.getByLabelText("Site"), "2");
    await waitFor(() => expect(api.listTickets).toHaveBeenLastCalledWith(expect.objectContaining({ site: "2" })));
    await user.selectOptions(screen.getByLabelText("Confidence"), "0.75");
    await waitFor(() => expect(api.listTickets).toHaveBeenLastCalledWith(expect.objectContaining({ min_confidence: "0.75" })));
  });

  it("reads initial filters from the URL", async () => {
    const api = renderQueue(fakeApi(), "/?status=approved&kind=bug");
    await waitFor(() => expect(api.listTickets).toHaveBeenCalledWith(expect.objectContaining({ status: "approved", kind: "bug" })));
  });

  it("moves the selection with j/k and opens the selected ticket with Enter", async () => {
    renderQueue();
    await screen.findByText("Pricing toggle shows yearly twice");
    const user = userEvent.setup();
    const rows = () => screen.getAllByRole("row").slice(1);
    expect(rows()[0]).toHaveAttribute("aria-selected", "true");
    await user.keyboard("j");
    expect(rows()[1]).toHaveAttribute("aria-selected", "true");
    await user.keyboard("j");
    await user.keyboard("j"); // clamps at the end
    expect(rows()[2]).toHaveAttribute("aria-selected", "true");
    await user.keyboard("k");
    expect(rows()[1]).toHaveAttribute("aria-selected", "true");
    await user.keyboard("{Enter}");
    expect(await screen.findByTestId("detail-probe")).toHaveTextContent("detail 2");
  });

  it("opens the approve dialog with the issue preview on 'a' and approves the selected ticket", async () => {
    const api = renderQueue();
    await screen.findByText("Pricing toggle shows yearly twice");
    const user = userEvent.setup();
    await user.keyboard("a");
    const dialog = await screen.findByRole("dialog", { name: /approve/i });
    expect(within(dialog).getByText("[bug] Pricing toggle shows yearly twice")).toBeInTheDocument();
    expect(within(dialog).getByText(/Visitor-provided content, untrusted/)).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: /create issue/i }));
    await waitFor(() => expect(api.approve).toHaveBeenCalledWith(1, expect.objectContaining({ title: "Pricing toggle shows yearly twice" })));
    expect(await screen.findByRole("link", { name: /issues\/101/ })).toBeInTheDocument();
  });

  it("opens the dismiss dialog on 'd' and requires a reason", async () => {
    const api = renderQueue();
    await screen.findByText("Pricing toggle shows yearly twice");
    const user = userEvent.setup();
    await user.keyboard("d");
    const dialog = await screen.findByRole("dialog", { name: /dismiss/i });
    const confirm = within(dialog).getByRole("button", { name: /^dismiss$/i });
    expect(confirm).toBeDisabled();
    await user.click(within(dialog).getByLabelText("Noise or spam"));
    expect(confirm).toBeEnabled();
    await user.click(confirm);
    await waitFor(() => expect(api.dismiss).toHaveBeenCalledWith(1, "noise", undefined));
  });

  it("does not react to shortcuts while typing in a field", async () => {
    renderQueue();
    await screen.findByText("Pricing toggle shows yearly twice");
    const user = userEvent.setup();
    await user.click(screen.getByLabelText("Kind"));
    await user.keyboard("a");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("shows an empty state", async () => {
    renderQueue(fakeApi({ listTickets: async () => ({ tickets: [], sites: [] }) }));
    expect(await screen.findByText(/nothing in the queue/i)).toBeInTheDocument();
  });
});
