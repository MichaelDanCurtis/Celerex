/** Accent-bloom field that Liquid Glass surfaces refract over. */
export function BloomBackdrop() {
  return (
    <div className="ov-bloom-root" data-testid="ov-bloom-backdrop" aria-hidden="true">
      <span
        className="ov-bloom"
        style={{ width: 460, height: 460, top: -180, left: -120, background: "var(--ov-bloom-1)", opacity: 0.42 }}
      />
      <span
        className="ov-bloom"
        style={{ width: 380, height: 380, bottom: -200, right: -120, background: "var(--ov-bloom-2)", opacity: 0.24 }}
      />
    </div>
  );
}
