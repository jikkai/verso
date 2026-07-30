import type { ServerPlugin } from "fumapress";
import { zhCN } from "@fumapress/language/zh-cn";
import { defineI18n } from "fumadocs-core/i18n";
import { defineConfig } from "fumapress";
import { fumadocsMdx } from "fumapress/adapters/mdx";
import { flexsearchPlugin } from "fumapress/plugins/flexsearch";
import { notFound } from "fumapress/router";
import { docs } from "./.source/server";

const defaultLanguage = "en";

const i18n = defineI18n({
  languages: ["en", "zh"],
  defaultLanguage,
  hideLocale: "default-locale",
});

const translations = i18n
  .translations()
  .preset("zh", zhCN())
  .add({
    en: { displayName: "English" },
    zh: { displayName: "简体中文" },
  });

const config = defineConfig({
  content: docs.toFumadocsSource(),
  mode: "static",
  translations,
  site: {
    name: "Verso",
    baseUrl: process.env.DOCS_SITE_URL ?? "http://localhost:3000",
    git: {
      user: "jikkai",
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
});

const defaultLanguageRoutes = {
  name: "default-language-routes",
  enforce: "post",
  async createPages({ createLayout, createPage }) {
    const loader = await this.getLoader();
    const RootLayout = this.layouts.root;
    const PageLayout = this.layouts.page;

    createLayout({
      render: "static",
      path: "/(default-language)",
      component: ({ children }) => <RootLayout lang={defaultLanguage}>{children}</RootLayout>,
    });
    createPage({
      render: "static",
      path: "/(default-language)/[...slugs]",
      staticPaths: loader.getPages(defaultLanguage).map((page) => page.slugs),
      component: ({ slugs }) => {
        const page = loader.getPage(slugs, defaultLanguage);
        if (!page) notFound();
        return <PageLayout lang={defaultLanguage} slugs={slugs} page={page} />;
      },
    });
  },
  resolvePage(page) {
    if (page.locale === defaultLanguage) return false;
  },
} satisfies ServerPlugin<(typeof config)["$context"]>;

export default config
  .layouts({
    defaultProps({ lang }) {
      return {
        githubUrl: "https://github.com/jikkai/verso",
        links: [
          {
            text: lang === "zh" ? "npm 软件包" : "npm package",
            url: "https://www.npmjs.com/package/@amamo/verso",
            external: true,
          },
        ],
        nav: {
          title: "Verso",
          url: lang === "zh" ? "/zh/" : "/",
        },
      };
    },
  })
  .plugins(defaultLanguageRoutes, flexsearchPlugin())
  .adapters(fumadocsMdx());
