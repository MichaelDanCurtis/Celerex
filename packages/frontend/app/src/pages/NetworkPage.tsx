import { lazy, Suspense, useEffect, useRef, useState } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import {
  activeMemberIdAtom,
  activeViewAtom,
  authEngagementAtom,
} from "../state/atoms";
import { listMembers, listProjects, type ApiProjectMember } from "../api/client";
import { buildNetworkGraph, type NetGraph, type NetNode } from "../canvas/networkGraph";

// three.js is heavy — load the 3D renderer only when this page mounts.
const ForceGraph3D = lazy(() => import("react-force-graph-3d"));

const LEGEND: Array<{ color: string; label: string }> = [
  { color: "#f8fafc", label: "Workspace" },
  { color: "#60a5fa", label: "Flow (click to open)" },
  { color: "#22d3ee", label: "Trigger step" },
  { color: "#a78bfa", label: "Action step" },
  { color: "#34d399", label: "Output step" },
  { color: "#2dd4bf", label: "galvo-memory" },
  { color: "#38bdf8", label: "Microsoft 365" },
  { color: "#818cf8", label: "Galvo" },
];

export function NetworkPage({ token }: { token: string }) {
  const engagement = useAtomValue(authEngagementAtom);
  const setMemberId = useSetAtom(activeMemberIdAtom);
  const setView = useSetAtom(activeViewAtom);
  const containerRef = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ w: 800, h: 600 });
  const [graph, setGraph] = useState<NetGraph>({ nodes: [], links: [] });
  const [loading, setLoading] = useState(true);
  const [flowCount, setFlowCount] = useState(0);

  // Measure the container so the WebGL canvas fills it exactly.
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const measure = () => {
      if (el.clientWidth > 0 && el.clientHeight > 0) {
        setSize({ w: el.clientWidth, h: el.clientHeight });
      }
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // Aggregate every flow across every project into one network.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const projects = await listProjects(token);
        const all: ApiProjectMember[] = (
          await Promise.all(projects.map((p) => listMembers(token, p.id).catch(() => [])))
        ).flat();
        if (cancelled) return;
        setFlowCount(all.length);
        setGraph(buildNetworkGraph(engagement?.name ?? "Workspace", all));
      } catch {
        /* graph stays empty; the hint below explains */
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [token, engagement?.name]);

  return (
    <div
      ref={containerRef}
      style={{ flex: 1, position: "relative", minWidth: 0, minHeight: 0, background: "#0a0c12" }}
    >
      <Suspense fallback={<CenterHint>Loading 3D renderer…</CenterHint>}>
        {graph.nodes.length > 1 && (
          <ForceGraph3D
            width={size.w}
            height={size.h}
            graphData={graph}
            backgroundColor="#0a0c12"
            nodeLabel={(n) => (n as NetNode).name}
            nodeVal={(n) => (n as NetNode).val}
            nodeColor={(n) => (n as NetNode).color}
            nodeOpacity={0.92}
            nodeResolution={16}
            linkColor={(l) =>
              (l as { kind?: string }).kind === "uses" ? "#2dd4bf55" : "#3a4258"
            }
            linkWidth={(l) => ((l as { kind?: string }).kind === "dataflow" ? 1.4 : 0.6)}
            linkOpacity={0.45}
            linkDirectionalParticles={(l) =>
              (l as { kind?: string }).kind === "dataflow" ? 2 : 0
            }
            linkDirectionalParticleSpeed={0.005}
            linkDirectionalParticleWidth={1.6}
            onNodeClick={(n) => {
              const node = n as NetNode;
              if (node.flowId) {
                setMemberId(node.flowId);
                setView("builder");
              }
            }}
          />
        )}
      </Suspense>

      {loading && <CenterHint>Mapping your agent network…</CenterHint>}
      {!loading && graph.nodes.length <= 1 && (
        <CenterHint>
          No flows yet — build one in <b>⚡ Flows</b> and it will appear here as a living graph.
        </CenterHint>
      )}

      {/* Header overlay */}
      <div style={{ position: "absolute", top: 16, left: 20, color: "#e6e9f0", pointerEvents: "none" }}>
        <div style={{ fontSize: 18, fontWeight: 700 }}>Agent Network</div>
        <div style={{ fontSize: 12.5, color: "#6b7488", marginTop: 2 }}>
          {flowCount} flow{flowCount === 1 ? "" : "s"} · drag to orbit · scroll to zoom · click a flow to open it
        </div>
      </div>

      {/* Legend */}
      <div
        style={{
          position: "absolute",
          bottom: 16,
          left: 20,
          padding: "10px 12px",
          borderRadius: 10,
          border: "1px solid #1d2230",
          background: "rgba(12,14,21,0.85)",
          fontSize: 11.5,
          color: "#aab2c6",
          lineHeight: 1.9,
        }}
      >
        {LEGEND.map((item) => (
          <div key={item.label} style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span
              style={{
                width: 10,
                height: 10,
                borderRadius: "50%",
                background: item.color,
                boxShadow: `0 0 6px ${item.color}88`,
              }}
            />
            {item.label}
          </div>
        ))}
      </div>
    </div>
  );
}

function CenterHint({ children }: { children: React.ReactNode }) {
  return (
    <div
      style={{
        position: "absolute",
        inset: 0,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        color: "#6b7488",
        fontSize: 14,
        pointerEvents: "none",
        textAlign: "center",
        padding: 40,
      }}
    >
      <div>{children}</div>
    </div>
  );
}
