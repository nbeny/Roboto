import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["test/**/*.test.ts"],
    environment: "node",
    // Les essais reseau ouvrent de vrais serveurs sur des ports ephemeres : les faire
    // tourner en parallele dans le meme processus rend les diagnostics illisibles.
    fileParallelism: false,
  },
});
