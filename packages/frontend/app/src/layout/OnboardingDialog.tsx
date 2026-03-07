import { useState, useCallback } from "react";

interface OnboardingDialogProps {
  onComplete: () => void;
}

const TOTAL_STEPS = 4;

const shortcuts = [
  { keys: ["\u2318", "K"], label: "Command Palette" },
  { keys: ["\u2318", "N"], label: "New Document" },
  { keys: ["\u2318", "B"], label: "Toggle Sidebar" },
  { keys: ["\u2318", "\\"], label: "Toggle AI Panel" },
  { keys: ["\u2318", "/"], label: "Quick Search" },
];

const overlayStyle: React.CSSProperties = {
  position: "fixed",
  inset: 0,
  zIndex: 9999,
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  background: "rgba(0,0,0,0.55)",
  backdropFilter: "blur(4px)",
};

const dialogStyle: React.CSSProperties = {
  width: 520,
  maxWidth: "90vw",
  maxHeight: "85vh",
  borderRadius: 16,
  background: "var(--celerex-bg-primary, #fff)",
  border: "1px solid var(--celerex-border, #e5e7eb)",
  boxShadow: "0 24px 48px rgba(0,0,0,0.2)",
  display: "flex",
  flexDirection: "column",
  overflow: "hidden",
  animation: "onboarding-fade-in 0.25s ease",
};

const btnBase: React.CSSProperties = {
  padding: "8px 20px",
  borderRadius: 8,
  border: "none",
  cursor: "pointer",
  fontSize: 14,
  fontWeight: 600,
  transition: "background 0.15s, color 0.15s",
};

const kbdStyle: React.CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  justifyContent: "center",
  minWidth: 26,
  height: 26,
  padding: "0 6px",
  borderRadius: 6,
  background: "var(--celerex-bg-secondary, #f3f4f6)",
  border: "1px solid var(--celerex-border, #e5e7eb)",
  fontSize: 12,
  fontWeight: 600,
  fontFamily: "system-ui, sans-serif",
  color: "var(--celerex-text-primary, #1f2937)",
};

function StepWelcome() {
  return (
    <div style={{ textAlign: "center", padding: "8px 0" }}>
      <div
        style={{
          width: "100%",
          height: 160,
          borderRadius: 12,
          background: "linear-gradient(135deg, var(--celerex-accent, #6366f1) 0%, #818cf8 100%)",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          marginBottom: 24,
          color: "#fff",
          fontSize: 48,
          fontWeight: 700,
          letterSpacing: -1,
        }}
      >
        Celerex
      </div>
      <h2 style={{ margin: "0 0 12px", fontSize: 24, fontWeight: 700, color: "var(--celerex-text-primary, #1f2937)" }}>
        Welcome to Celerex
      </h2>
      <p style={{ margin: 0, fontSize: 15, lineHeight: 1.6, color: "var(--celerex-text-secondary, #6b7280)" }}>
        Your all-in-one workspace for managing engagements, collaborating on documents, and working
        smarter with AI-powered tools. Let us show you around.
      </p>
    </div>
  );
}

function StepWorkspace() {
  const features = [
    {
      icon: "\uD83D\uDCC1",
      title: "Engagements",
      desc: "Organize your work into engagements, each containing related documents and tasks.",
    },
    {
      icon: "\uD83D\uDCC4",
      title: "Documents",
      desc: "Rich editor with page and edgeless modes, templates, version history, and more.",
    },
    {
      icon: "\u2728",
      title: "AI Panel",
      desc: "Get AI-powered assistance for writing, analysis, and document generation right in your workflow.",
    },
  ];

  return (
    <div>
      <h2 style={{ margin: "0 0 8px", fontSize: 22, fontWeight: 700, color: "var(--celerex-text-primary, #1f2937)", textAlign: "center" }}>
        Your Workspace
      </h2>
      <p style={{ margin: "0 0 20px", fontSize: 14, color: "var(--celerex-text-secondary, #6b7280)", textAlign: "center" }}>
        Everything you need, organized in one place.
      </p>
      <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
        {features.map((f) => (
          <div
            key={f.title}
            style={{
              display: "flex",
              gap: 14,
              padding: 14,
              borderRadius: 10,
              background: "var(--celerex-bg-secondary, #f9fafb)",
              border: "1px solid var(--celerex-border, #e5e7eb)",
            }}
          >
            <span style={{ fontSize: 28, lineHeight: 1 }}>{f.icon}</span>
            <div>
              <div style={{ fontWeight: 600, fontSize: 14, color: "var(--celerex-text-primary, #1f2937)", marginBottom: 2 }}>
                {f.title}
              </div>
              <div style={{ fontSize: 13, color: "var(--celerex-text-secondary, #6b7280)", lineHeight: 1.4 }}>
                {f.desc}
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

function StepShortcuts() {
  return (
    <div>
      <h2 style={{ margin: "0 0 8px", fontSize: 22, fontWeight: 700, color: "var(--celerex-text-primary, #1f2937)", textAlign: "center" }}>
        Keyboard Shortcuts
      </h2>
      <p style={{ margin: "0 0 20px", fontSize: 14, color: "var(--celerex-text-secondary, #6b7280)", textAlign: "center" }}>
        Work faster with these essential shortcuts.
      </p>
      <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
        {shortcuts.map((s) => (
          <div
            key={s.label}
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
              padding: "10px 14px",
              borderRadius: 8,
              background: "var(--celerex-bg-secondary, #f9fafb)",
              border: "1px solid var(--celerex-border, #e5e7eb)",
            }}
          >
            <span style={{ fontSize: 14, color: "var(--celerex-text-primary, #1f2937)", fontWeight: 500 }}>
              {s.label}
            </span>
            <div style={{ display: "flex", gap: 4 }}>
              {s.keys.map((k, i) => (
                <kbd key={i} style={kbdStyle}>{k}</kbd>
              ))}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

function StepReady() {
  return (
    <div style={{ textAlign: "center", padding: "20px 0" }}>
      <div style={{ fontSize: 64, marginBottom: 16 }}>{"\uD83D\uDE80"}</div>
      <h2 style={{ margin: "0 0 12px", fontSize: 24, fontWeight: 700, color: "var(--celerex-text-primary, #1f2937)" }}>
        You're Ready!
      </h2>
      <p style={{ margin: 0, fontSize: 15, lineHeight: 1.6, color: "var(--celerex-text-secondary, #6b7280)", maxWidth: 360, marginInline: "auto" }}>
        You're all set to start working. Create your first document, explore your workspace, or open the command palette with <kbd style={kbdStyle}>{"\u2318"}</kbd>{" "}<kbd style={kbdStyle}>K</kbd> anytime.
      </p>
    </div>
  );
}

export function OnboardingDialog({ onComplete }: OnboardingDialogProps) {
  const [step, setStep] = useState(0);

  const handleComplete = useCallback(() => {
    localStorage.setItem("celerex-onboarded", "true");
    onComplete();
  }, [onComplete]);

  const handleSkip = useCallback(() => {
    handleComplete();
  }, [handleComplete]);

  const handleNext = useCallback(() => {
    if (step < TOTAL_STEPS - 1) {
      setStep((s) => s + 1);
    } else {
      handleComplete();
    }
  }, [step, handleComplete]);

  const handleBack = useCallback(() => {
    setStep((s) => Math.max(0, s - 1));
  }, []);

  const steps = [<StepWelcome />, <StepWorkspace />, <StepShortcuts />, <StepReady />];

  return (
    <>
      <style>{`
        @keyframes onboarding-fade-in {
          from { opacity: 0; transform: scale(0.96) translateY(8px); }
          to { opacity: 1; transform: scale(1) translateY(0); }
        }
      `}</style>
      <div style={overlayStyle} onClick={handleSkip}>
        <div style={dialogStyle} onClick={(e) => e.stopPropagation()}>
          {/* Content */}
          <div style={{ flex: 1, padding: "32px 32px 20px", overflowY: "auto" }}>
            {steps[step]}
          </div>

          {/* Footer */}
          <div
            style={{
              padding: "16px 32px 24px",
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
            }}
          >
            {/* Step dots */}
            <div style={{ display: "flex", gap: 8 }}>
              {Array.from({ length: TOTAL_STEPS }).map((_, i) => (
                <div
                  key={i}
                  style={{
                    width: 8,
                    height: 8,
                    borderRadius: "50%",
                    background:
                      i === step
                        ? "var(--celerex-accent, #6366f1)"
                        : "var(--celerex-border, #d1d5db)",
                    transition: "background 0.2s",
                  }}
                />
              ))}
            </div>

            {/* Buttons */}
            <div style={{ display: "flex", gap: 8 }}>
              {step > 0 && (
                <button
                  onClick={handleBack}
                  style={{
                    ...btnBase,
                    background: "transparent",
                    color: "var(--celerex-text-secondary, #6b7280)",
                    border: "1px solid var(--celerex-border, #e5e7eb)",
                  }}
                >
                  Back
                </button>
              )}
              {step < TOTAL_STEPS - 1 && (
                <button
                  onClick={handleSkip}
                  style={{
                    ...btnBase,
                    background: "transparent",
                    color: "var(--celerex-text-secondary, #6b7280)",
                  }}
                >
                  Skip
                </button>
              )}
              <button
                onClick={handleNext}
                style={{
                  ...btnBase,
                  background: "var(--celerex-accent, #6366f1)",
                  color: "#fff",
                }}
              >
                {step === TOTAL_STEPS - 1 ? "Start Working" : "Next"}
              </button>
            </div>
          </div>
        </div>
      </div>
    </>
  );
}
