import { defineDirectory } from "@amamo/doctrine";

export default defineDirectory({
  items: [
    { page: "index", title: "概览" },
    { page: "getting-started", title: "快速开始" },
    { page: "release-workflow", title: "发布流程" },
    { page: "configuration", title: "配置参考" },
    { page: "cli-reference", title: "CLI 参考" },
  ],
});
