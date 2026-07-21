import { zhCN } from "@fumapress/language/zh-cn";
import { defineI18n } from "fumadocs-core/i18n";
import { defineConfig } from "fumapress";
import { fumadocsMdx } from "fumapress/adapters/mdx";
import { flexsearchPlugin } from "fumapress/plugins/flexsearch";
import { llmsPlugin } from "fumapress/plugins/llms.txt";
import { docs } from "./.source/server";

const i18n = defineI18n({
  languages: ["en", "zh"],
  defaultLanguage: "en",
});

const translations = i18n
  .translations()
  .preset("zh", zhCN())
  .add({
    en: { displayName: "English" },
    zh: { displayName: "简体中文" },
  });

export default defineConfig({
  content: docs.toFumadocsSource(),
  mode: "static",
  translations,
  site: {
    name: "Verso",
    baseUrl: process.env.DOCS_SITE_URL ?? "http://localhost:3000",
    git: {
      user: "dream-num",
      repo: "verso",
      branch: "main",
      rootDir: "apps/docs",
    },
  },
  meta: {
    root() {
      return (
        <>
          <meta
            name="description"
            content="Verso documentation for versioning and releasing JavaScript workspaces."
          />
        </>
      );
    },
  },
})
  .layouts({
    defaultProps({ lang }) {
      return {
        githubUrl: "https://github.com/dream-num/verso",
        links: [
          {
            text: lang === "zh" ? "npm 软件包" : "npm package",
            url: "https://www.npmjs.com/package/@univerkit/verso",
            external: true,
          },
        ],
        nav: {
          title: "Verso",
          url: lang === "zh" ? "/zh/" : "/en/",
        },
      };
    },
  })
  .plugins(flexsearchPlugin(), llmsPlugin())
  .adapters(fumadocsMdx());
