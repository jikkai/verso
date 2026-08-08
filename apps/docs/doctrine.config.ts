import { defineConfig } from "@amamo/doctrine";

export default defineConfig({
  copyright: {
    en: "Released under the MIT License.",
    "zh-CN": "基于 MIT License 发布。",
  },
  description: {
    en: "Version and release JavaScript workspaces as one atomic unit.",
    "zh-CN": "把 JavaScript workspace 作为一个原子单元统一版本和发布。",
  },
  githubUrl: "https://github.com/jikkai/verso",
  locales: {
    default: "en",
    labels: { en: "English", "zh-CN": "简体中文" },
    names: ["en", "zh-CN"],
  },
  siteUrl: process.env.DOCS_SITE_URL ?? "http://localhost/",
  title: { en: "Verso", "zh-CN": "Verso" },
});
