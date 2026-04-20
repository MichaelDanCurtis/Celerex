class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}

if (typeof globalThis.ResizeObserver === "undefined") {
  (globalThis as unknown as { ResizeObserver: typeof ResizeObserverStub }).ResizeObserver =
    ResizeObserverStub;
}

class DOMMatrixReadOnlyStub {
  m22 = 1;
  constructor(_init?: string) {}
}

if (typeof globalThis.DOMMatrixReadOnly === "undefined") {
  (
    globalThis as unknown as { DOMMatrixReadOnly: typeof DOMMatrixReadOnlyStub }
  ).DOMMatrixReadOnly = DOMMatrixReadOnlyStub;
}
