import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { Provider, createStore } from "jotai";
import { tokenAtom } from "../../state/atoms";
import { ApproveDeviceSettings } from "../ApproveDeviceSettings";
import { approveDevice } from "../../api/client";

vi.mock("../../api/client", () => ({
  approveDevice: vi.fn(async () => ({ ok: true, label: "Design on MacBook" })),
}));

describe("ApproveDeviceSettings", () => {
  it("approves a device with the trimmed/uppercased code and shows success", async () => {
    const store = createStore();
    store.set(tokenAtom, "tok");

    render(
      <Provider store={store}>
        <ApproveDeviceSettings />
      </Provider>,
    );

    const input = screen.getByTestId("approve-device-input") as HTMLInputElement;
    fireEvent.change(input, { target: { value: " abcd-efgh " } });
    fireEvent.click(screen.getByTestId("approve-device-submit"));

    await waitFor(() =>
      expect(approveDevice).toHaveBeenCalledWith("tok", "ABCD-EFGH"),
    );
    await waitFor(() =>
      expect(screen.getByTestId("approve-device-status").textContent).toContain(
        "Design on MacBook",
      ),
    );
  });
});
