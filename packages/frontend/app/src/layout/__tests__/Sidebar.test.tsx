import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { Provider, createStore } from "jotai";
import { Sidebar } from "../Sidebar";
import {
  projectsAtom,
  activeProjectIdAtom,
  activeViewAtom,
} from "../../state/atoms";

describe("Sidebar + Add agent", () => {
  it("renders the add-agent affordance per project and wires it to builder view", () => {
    const store = createStore();
    store.set(projectsAtom, [
      { id: "p1", name: "Jean", engagementId: "eng:personal" },
    ]);

    render(
      <Provider store={store}>
        <Sidebar collapsed={false} onToggleCollapse={() => {}} />
      </Provider>,
    );

    const btn = screen.getByTestId("add-agent-p1");
    expect(btn).toBeTruthy();
    expect(btn.textContent).toContain("Add agent");

    fireEvent.click(btn);
    expect(store.get(activeProjectIdAtom)).toBe("p1");
    expect(store.get(activeViewAtom)).toBe("builder");
  });

  it("renders no add-agent button when there are no projects", () => {
    render(
      <Provider>
        <Sidebar collapsed={false} onToggleCollapse={() => {}} />
      </Provider>,
    );
    expect(screen.queryByTestId(/^add-agent-/)).toBeNull();
  });
});
