import { defineConfig } from "@amamo/doctrine";

export default defineConfig({
  description: {
    en: "Verso documentation for versioning and releasing JavaScript workspaces.",
    "zh-CN": "Verso 文档，介绍如何统一管理与发布 JavaScript workspace。",
  },
  locales: {
    default: "en",
    labels: { en: "English", "zh-CN": "简体中文" },
    names: ["en", "zh-CN"],
  },
  siteUrl: process.env.DOCS_SITE_URL ?? "http://localhost/",
  title: { en: "Verso", "zh-CN": "Verso" },
});
