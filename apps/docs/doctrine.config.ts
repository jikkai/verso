import { defineConfig } from "@amamo/doctrine";

export default defineConfig({
  copyright: "Copyright © 2026 白熱.",
  description: "Version and release JavaScript workspaces as one atomic unit.",
  githubUrl: "https://github.com/jikkai/verso",
  locales: {
    default: "en",
    labels: { en: "English", "zh-CN": "简体中文" },
    names: ["en", "zh-CN"],
  },
  siteUrl: process.env.DOCS_SITE_URL ?? "http://localhost/",
  title: "Verso",
});
