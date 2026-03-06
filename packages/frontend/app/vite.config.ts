import { defineConfig, type Plugin } from "vite";
import { vanillaExtractPlugin } from "@vanilla-extract/vite-plugin";
import reactSWC from "@vitejs/plugin-react-swc";
import swc from "@swc/core";
import path from "path";
import fs from "fs";

/**
 * Custom Vite plugin to transform BlockSuite TypeScript files with SWC.
 *
 * BlockSuite publishes raw TypeScript with TC39 decorators (accessor keyword).
 * Vite's esbuild can't handle these. This plugin:
 * 1. In dev: intercepts all .ts file loads from @blocksuite and transforms with SWC
 * 2. Works for both direct imports and @fs/ served files
 */
function blocksuiteTransformPlugin(): Plugin {
  return {
    name: "blocksuite-swc-transform",
    enforce: "pre",

    // Transform all @blocksuite .ts files that Vite serves
    async transform(code, id) {
      if (!id.includes("blocksuite") && !id.includes("@blocksuite")) return null;
      if (!id.endsWith(".ts")) return null;

      const result = await swc.transform(code, {
        filename: id,
        jsc: {
          parser: {
            syntax: "typescript",
            tsx: false,
            decorators: true,
          },
          target: "es2022",
          transform: {
            useDefineForClassFields: false,
            decoratorVersion: "2022-03",
          },
        },
        sourceMaps: true,
      });

      return {
        code: result.code,
        map: result.map ? JSON.parse(result.map) : null,
      };
    },
  };
}

/**
 * Esbuild plugin for dep optimization that uses SWC for @blocksuite .ts files.
 * This handles the optimizeDeps pre-bundling step where esbuild would choke.
 */
function blocksuiteEsbuildPlugin(): import("esbuild").Plugin {
  return {
    name: "blocksuite-swc-esbuild",
    setup(build) {
      // Stub out .css.ts files so vanilla-extract style() calls don't execute
      // during esbuild pre-bundling (they need the compile-time transform).
      // Return empty exports — the styles will be handled by vanilla-extract's
      // Vite plugin when imported at runtime.
      build.onLoad({ filter: /\.css\.ts$/ }, async (args) => {
        // Read the file to find exported variable names, then export empty strings
        const source = await fs.promises.readFile(args.path, "utf-8");
        const exportNames = [...source.matchAll(/export\s+(?:const|let|var)\s+(\w+)/g)].map(m => m[1]);
        const stub = exportNames.map(name => `export const ${name} = "";`).join("\n");
        return { contents: stub || "export {};", loader: "js" };
      });

      // Intercept .ts files from @blocksuite packages and transform with SWC
      build.onLoad({ filter: /\.ts$/ }, async (args) => {
        if (!args.path.includes("blocksuite") && !args.path.includes("@blocksuite")) {
          return undefined;
        }

        const code = await fs.promises.readFile(args.path, "utf-8");
        const result = await swc.transform(code, {
          filename: args.path,
          jsc: {
            parser: {
              syntax: "typescript",
              tsx: false,
              decorators: true,
            },
            target: "es2022",
            transform: {
              useDefineForClassFields: false,
              decoratorVersion: "2022-03",
            },
          },
          sourceMaps: false,
        });

        return {
          contents: result.code,
          loader: "js",
        };
      });
    },
  };
}

export default defineConfig({
  plugins: [
    blocksuiteTransformPlugin(),
    vanillaExtractPlugin(),
    reactSWC({
      tsDecorators: true,
    }),
  ],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  optimizeDeps: {
    esbuildOptions: {
      target: "esnext",
      plugins: [blocksuiteEsbuildPlugin()],
    },
  },
  esbuild: {
    target: "esnext",
  },
  build: {
    target: "esnext",
  },
});
