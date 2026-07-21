import tailwindcss from "@tailwindcss/vite";
import mdx from "fumadocs-mdx/vite";
import press from "fumapress/vite";
import { defineConfig } from "waku/config";

export default defineConfig({
  basePath: process.env.DOCS_BASE_PATH ?? "/",
  vite: {
    plugins: [press(), mdx(), tailwindcss()],
  },
});
