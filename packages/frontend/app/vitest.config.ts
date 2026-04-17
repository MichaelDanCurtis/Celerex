import { defineConfig } from "vitest/config";
import reactSWC from "@vitejs/plugin-react-swc";
import path from "path";

export default defineConfig({
  plugins: [reactSWC()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  test: {
    environment: "jsdom",
    globals: true,
    include: ["src/**/__tests__/**/*.test.{ts,tsx}"],
  },
});
