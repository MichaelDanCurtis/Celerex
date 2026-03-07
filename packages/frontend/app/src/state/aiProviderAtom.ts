import { atom } from "jotai";

export interface AIProvider {
  id: string;
  name: string;
  description: string;
  defaultModel: string;
  models: string[];
}

export const AI_PROVIDERS: AIProvider[] = [
  {
    id: "anthropic",
    name: "Anthropic",
    description: "Claude models",
    defaultModel: "claude-sonnet-4-20250514",
    models: ["claude-sonnet-4-20250514", "claude-haiku-4-5-20251001", "claude-opus-4-6"],
  },
  {
    id: "github-copilot",
    name: "GitHub Copilot",
    description: "GitHub AI models",
    defaultModel: "gpt-4o",
    models: ["gpt-4o", "gpt-4o-mini", "o3-mini"],
  },
  {
    id: "openrouter",
    name: "OpenRouter",
    description: "Multi-model router",
    defaultModel: "anthropic/claude-sonnet-4-20250514",
    models: ["anthropic/claude-sonnet-4-20250514", "openai/gpt-4o", "google/gemini-2.0-flash-001", "meta-llama/llama-3.1-70b-instruct"],
  },
  {
    id: "zhipu",
    name: "Z.ai (GLM)",
    description: "GLM-5 & GLM-4.7 by Zhipu AI",
    defaultModel: "glm-5",
    models: ["glm-5", "glm-4.7", "glm-4-plus", "glm-4-flash"],
  },
  {
    id: "xai",
    name: "xAI (Grok)",
    description: "Grok models via z.ai",
    defaultModel: "grok-3",
    models: ["grok-3", "grok-3-mini", "grok-2"],
  },
  {
    id: "kimi",
    name: "Kimi K2.5",
    description: "Moonshot AI",
    defaultModel: "kimi-k2-0711",
    models: ["kimi-k2-0711", "moonshot-v1-8k", "moonshot-v1-32k"],
  },
];

// Selected provider ID
export const selectedProviderIdAtom = atom<string>(
  localStorage.getItem("celerex-ai-provider") || "anthropic"
);

// Selected model per provider
export const selectedModelAtom = atom<string>(
  localStorage.getItem("celerex-ai-model") || "claude-sonnet-4-20250514"
);

// API keys stored per provider: Record<providerId, apiKey>
export const apiKeysAtom = atom<Record<string, string>>(() => {
  try {
    return JSON.parse(localStorage.getItem("celerex-ai-keys") || "{}");
  } catch {
    return {};
  }
});

// Derived: get current provider object
export const selectedProviderAtom = atom((get) => {
  const id = get(selectedProviderIdAtom);
  return AI_PROVIDERS.find((p) => p.id === id) || AI_PROVIDERS[0];
});

// Whether GitHub Copilot is authenticated via device flow
export const copilotAuthenticatedAtom = atom<boolean>(false);
