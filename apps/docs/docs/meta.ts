import { defineDirectory } from "@amamo/doctrine";

export default defineDirectory({
  items: [
    { page: "index", title: "Overview" },
    { page: "getting-started", title: "Getting started" },
    { page: "configuration", title: "Configuration" },
    { page: "cli-reference", title: "CLI reference" },
    { page: "release-workflow", title: "Release workflow" },
  ],
});
