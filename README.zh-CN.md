# Verso

[English](README.md) | [简体中文](README.zh-CN.md)

[![CI](https://github.com/jikkai/verso/actions/workflows/ci.yml/badge.svg)](https://github.com/jikkai/verso/actions/workflows/ci.yml)
[![npm version](https://img.shields.io/npm/v/@amamo/verso.svg)](https://www.npmjs.com/package/@amamo/verso)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Verso 把 JavaScript workspace 作为一个版本发布。它会发现 package manifest，按需更新配置的 Cargo
package 和 Conventional Commit changelog，创建 release commit 与 annotated tag，再原子推送当前
upstream 分支和这个准确的 tag。

Verso 刻意在这里结束。Registry 发布、GitHub Release、二进制构建和部署应交给由 tag 触发的 CI。

## 运行条件

- npm wrapper 要求 Node.js 22.18 或更高版本。
- Git、具名分支和已配置的 upstream。
- 受支持的原生目标：macOS arm64/x64、Linux GNU arm64/x64 或 Windows x64。
- 包含有效 SemVer 的 `package.json`、`package.json5`、`package.yaml` 或 `package.yml`。

## 快速开始

```sh
pnpm add -D @amamo/verso
```

```json
{
  "scripts": {
    "release": "verso"
  }
}
```

单包仓库不需要配置文件。Workspace 可以创建 `verso.toml`：

```toml
[workspaces]
patterns = ["packages/*"]
```

先检查仓库和一个准确的发布计划：

```sh
pnpm release -- doctor
pnpm release -- --dry-run --version 1.4.0
```

计划无误后，使用交互模式，或为自动化提供准确版本：

```sh
pnpm release
pnpm release -- --version 1.4.0 --yes
```

`--yes` 只接受确认，不会选择版本。

## 发布模型

```text
配置 + manifest + Git 历史
  -> 检查项目和统一版本
  -> 确定一个目标 SemVer
  -> 更新 package/Cargo 版本和可选 changelog
  -> commit -> annotated tag
  -> git push --atomic <upstream-branch> <exact-tag>
```

- `verso doctor` 会检查配置、package 发现、版本、changelog 路径、Cargo package 和分支 upstream，
  但不会开始发布。
- `verso --dry-run` 会输出版本文件、hook、警告和 Git 命令，不写文件，也不修改 Git。
- 真正发布默认要求工作区干净。宽松模式仍要求 index 和 release 文件干净。
- 推送前的执行错误会按阶段清理本地状态。用户取消会保留已完成的检查点；根据所处提示，修改后的文件、
  release commit 或本地 tag 可能继续存在。
- 推送失败会保留本地 commit 和 tag；`after_push` hook 失败时，远端已经接收 release ref。

完整状态矩阵见[发布流程](https://jikkai.github.io/verso/zh-CN/release-workflow/)。

## 配置

所有配置项都可选。`verso init` 可以生成初始文件；`--config <PATH>` 会把配置文件所在目录作为发布根目录。
未知配置项会被拒绝；配置路径必须是相对路径、使用正斜杠，并留在发布根目录内。

```toml
[version]
root_package = "package.json"
require_consistent_versions = true
cargo_manifest_paths = ["crates/cli/Cargo.toml"]

[workspaces]
patterns = ["packages/*", "!packages/fixtures"]
include_root = true
ignore = ["examples"]
use_gitignore = true

[changelog]
enabled = true
infile = "CHANGELOG.md"
preset = "angular"

[git]
require_clean_worktree = true
commit_message = "chore(release): release v${version}"
tag_name = "v${version}"
push = "atomic"

[hooks]
before_version = "pnpm test"
before_push = "pnpm run check"
```

Workspace 模式为空时，会先从 `pnpm-workspace.yaml` 推断，再读取根 manifest 的 `workspaces` 字段。
每个匹配目录按 JSON、JSON5、`package.yaml`、`package.yml` 的顺序选择第一个 manifest。

Hook 是受信任的 shell 命令，并会原样出现在 dry-run 输出中。Secret 应通过环境传入，不要写进
`verso.toml`。

完整说明见[配置参考](https://jikkai.github.io/verso/zh-CN/configuration/)和
[CLI 参考](https://jikkai.github.io/verso/zh-CN/cli-reference/)。

## 边界

Verso 适合所有可发布 package 共用同一个版本和 tag 的仓库。它不支持 package 独立版本、非原子推送、
本地 registry 发布，也不支持 `github_release.enabled = true`。

维护者开发和发布流程见 [CONTRIBUTING.md](CONTRIBUTING.md)，安全问题请按 [SECURITY.md](SECURITY.md)
报告。

## License

MIT
