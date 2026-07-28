// @lovable.dev/vite-tanstack-config supplies the TanStack Start, React, Tailwind,
// Nitro, preview, environment, dedupe, and diagnostics configuration used by the app.
import { defineConfig } from "@lovable.dev/vite-tanstack-config";
import type { PluginOption, UserConfig } from "vite";

function removeLegacyTsconfigPaths(
  plugins: PluginOption[] | undefined,
): PluginOption[] | undefined {
  if (!plugins) return undefined;

  return plugins.flatMap((plugin): PluginOption[] => {
    if (!plugin) return [];
    if (Array.isArray(plugin)) {
      return removeLegacyTsconfigPaths(plugin) ?? [];
    }
    if (
      typeof plugin === "object" &&
      "name" in plugin &&
      plugin.name === "vite-tsconfig-paths"
    ) {
      return [];
    }
    return [plugin];
  });
}

const lovableConfig = defineConfig({
  tanstackStart: {
    // Redirect TanStack Start's bundled server entry to src/server.ts (our SSR error wrapper).
    // nitro/vite builds from this.
    server: { entry: "server" },
  },
});

export default async (
  ...args: Parameters<typeof lovableConfig>
): Promise<UserConfig> => {
  const config = await lovableConfig(...args);

  return {
    ...config,
    // Vite 8 resolves tsconfig aliases natively. The Lovable preset still injects
    // the older plugin, so remove only that plugin while retaining the rest of the preset.
    plugins: removeLegacyTsconfigPaths(config.plugins),
    resolve: {
      ...config.resolve,
      tsconfigPaths: true,
    },
  };
};
