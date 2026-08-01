import amamo from "@amamo/oxlint-config";

export default amamo({
  ignores: [
    "node_modules",
    "target",
    "artifacts",
    "docs/superpowers",
    "packages/*/bin",
    "packages/*/dist",
    "packages/*/test-dist",
    "packages/*/verso",
    "packages/*/verso.exe",
    "*.tgz",
  ],
  node: true,
  react: true,
  rules: {
    "react/react-in-jsx-scope": "off",
  },
  tailwindcss: {
    entryPoint: "apps/docs/src/app.css",
  },
});
