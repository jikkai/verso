# Verso

[English](README.md) | [简体中文](README.zh-CN.md)

[![CI](https://github.com/jikkai/verso/actions/workflows/ci.yml/badge.svg)](https://github.com/jikkai/verso/actions/workflows/ci.yml)
[![npm version](https://img.shields.io/npm/v/@amamo/verso.svg)](https://www.npmjs.com/package/@amamo/verso)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Verso 是一个面向 JavaScript workspace 的轻量发布工具，适合多个包共用同一个版本号的仓库。它会更新 package manifest，生成 Angular 风格的 conventional changelog，创建 release commit 和 tag，并以原子操作推送当前上游分支和 release tag。

## 安装

```sh
pnpm add -D @amamo/verso
```

在项目的 `package.json` 里增加 release script：

```json
{
  "scripts": {
    "release": "verso"
  }
}
```

## 配置

所有配置项都可选。`verso init` 会生成一份示例配置；没有时，只要根目录存在 `package.json`，Verso 会使用内置默认值。

大多数 workspace 项目只需要配 `workspaces.patterns`。省略它时，Verso 会先读 `pnpm-workspace.yaml` 或根 package manifest 的 `workspaces` 字段；都没有时回退到单包模式。

```toml
[version]
root_package = "package.json"
require_consistent_versions = true
cargo_manifest_paths = []

[workspaces]
patterns = []
include_root = true
ignore = []
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
# before_version = "pnpm test"
# after_version = "pnpm build"
# before_commit = "pnpm lint"
# after_push = "node scripts/notify-release.mts"

[github_release]
enabled = false
```

显式传入的 `--config <PATH>` 必须指向真实文件。package 发现支持 `package.json`、`package.json5`、`package.yaml` 和 `package.yml`；同一目录存在多个 manifest 时按这个顺序选择。旧配置中的 `git.push = "follow-tags"` 会作为兼容别名使用新的原子推送模式。

### 所有配置项（按常用到非常用排序）

| 配置项                                | 默认值                                | 说明                                                                                                                                                                           |
| ------------------------------------- | ------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `workspaces.patterns`                 | `[]`                                  | 相对于配置文件目录的 workspace glob。正斜杠，支持 `*`、`**`、`?`、字符类、brace，以及 `!` 排除。省略时读取 `pnpm-workspace.yaml` 或根 manifest 的 `workspaces`，否则单包模式。 |
| `workspaces.include_root`             | `true`                                | 是否包含 `version.root_package` 指向的根 package。                                                                                                                             |
| `workspaces.ignore`                   | `[]`                                  | workspace 发现时额外忽略的模式。`fixtures` 这类普通路径片段按目录名匹配。                                                                                                      |
| `workspaces.use_gitignore`            | `true`                                | workspace 发现时是否读取根目录和子目录里的 `.gitignore`。如果项目要发布被 `.gitignore` 忽略的目录，设为 `false`。                                                              |
| `version.root_package`                | `package.json`                        | 用于读取当前版本并参与更新的根 package manifest。路径必须在配置文件目录内。省略且 `package.json` 不存在时，Verso 依次尝试 `package.json5`、`package.yaml`、`package.yml`。     |
| `version.require_consistent_versions` | `true`                                | 发现 package 或配置的 Cargo manifest 版本不一致时是否失败。                                                                                                                    |
| `version.cargo_manifest_paths`        | `[]`                                  | 需要同步更新 `[package].version` 的 Cargo manifest 路径。存在最近的 `Cargo.lock` 时会一起更新。                                                                                |
| `changelog.enabled`                   | `true`                                | 设为 `false` 时发布过程不修改 changelog。                                                                                                                                      |
| `changelog.infile`                    | `CHANGELOG.md`                        | 发布时写入的 changelog 文件。路径必须在配置文件目录内。                                                                                                                        |
| `changelog.preset`                    | `angular`                             | 目前只支持 `angular`。                                                                                                                                                         |
| `git.require_clean_worktree`          | `true`                                | 修改文件前要求工作区干净。设为 `false` 时允许无关的未暂存改动，但 Git index、package/Cargo manifest、Cargo lockfile 和 changelog 仍必须干净。                                  |
| `git.commit_message`                  | `chore(release): release v${version}` | release commit message，`${version}` 会替换为目标版本。                                                                                                                        |
| `git.tag_name`                        | `v${version}`                         | release tag 模板。必须包含 `${version}` 并渲染为合法 Git tag。                                                                                                                 |
| `git.push`                            | `atomic`                              | 原子推送当前上游分支和准确的 release tag。目前只支持 `atomic`。                                                                                                                |
| `hooks.before_version`                | 无                                    | 更新 release 文件前执行的 shell 命令。                                                                                                                                         |
| `hooks.after_version`                 | 无                                    | 更新 release 文件后执行的 shell 命令。                                                                                                                                         |
| `hooks.before_commit`                 | 无                                    | 暂存并提交前执行的 shell 命令。                                                                                                                                                |
| `hooks.after_commit`                  | 无                                    | release commit 创建后执行的 shell 命令。                                                                                                                                       |
| `hooks.before_tag`                    | 无                                    | 创建 release tag 前执行的 shell 命令。                                                                                                                                         |
| `hooks.after_tag`                     | 无                                    | release tag 创建后执行的 shell 命令。                                                                                                                                          |
| `hooks.before_push`                   | 无                                    | 原子推送分支和 tag 前执行的 shell 命令。                                                                                                                                       |
| `hooks.after_push`                    | 无                                    | push 成功后执行的 shell 命令。                                                                                                                                                 |
| `github_release.enabled`              | `false`                               | 当前版本不支持设为 `true`。                                                                                                                                                    |

## CLI

```sh
pnpm release
pnpm release -- --dry-run
pnpm release -- --version 0.26.0
pnpm release -- --version 0.26.0 --yes
pnpm release -- --dry-run --json
pnpm release -- --config path/to/verso.toml
pnpm release -- doctor
pnpm release -- init
pnpm release -- -V
pnpm release -- --help
```

| 参数                 | 默认值       | 说明                                                     |
| -------------------- | ------------ | -------------------------------------------------------- |
| `--dry-run`          | `false`      | 预览发布过程，不写文件，也不执行会修改状态的 git 命令。  |
| `--json`             | `false`      | 以 JSON 打印 dry-run 输出。必须和 `--dry-run` 一起使用。 |
| `--version <SEMVER>` | 无           | 使用指定目标版本，跳过版本选择。                         |
| `--config <PATH>`    | `verso.toml` | 读取其他配置文件。                                       |
| `--yes`              | `false`      | 跳过发布确认。它不会替你选择版本。                       |
| `-V, --tool-version` | 无           | 打印 Verso CLI 版本。                                    |
| `--help`             | 无           | 打印帮助信息。                                           |

子命令：

| 命令           | 说明                                                                                                                       |
| -------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `verso init`   | 创建初始 `verso.toml`。会自动探测 `packages/*`，也可以用 `--single`、`--workspace`、`--force` 控制行为。                   |
| `verso doctor` | 校验配置解析、package 发现、版本一致性、changelog 路径、Cargo manifest 版本和 Git 上游状态。可用 `--json` 输出结构化结果。 |

不传 `--version` 时，Verso 会打开交互式菜单选择 patch、minor、major、alpha、beta、rc 或自定义 semver；当前版本是 prerelease 时，还会提供对应的 stable 版本。选择 prerelease channel 后，会继续选择 base version，也支持输入自定义 base version。`--version` 可以传精确版本，包括 `0.26.0-alpha.0`、`0.26.0-beta.1`、`0.26.0-rc.2` 这类 prerelease。

`--yes` 会跳过发布确认，包括目标版本不大于当前版本时默认拒绝的确认。它不会替你选择目标版本；没有 `--version` 时仍然会进入交互式版本选择。`-V` 和 `--tool-version` 会在读取发布配置前直接输出 CLI 版本，适合排查安装问题。

当 stdin 或 stdout 不是终端时，Verso 会保留纯文本 fallback，脚本测试和管道输入仍然可以用名称选择，比如先输入 `beta`，再输入 `minor`。

## 发布时会发生什么

```text
            +----------------------------+
            |  读取 verso.toml +         |
            |  发现 package manifest     |
            +-------------+--------------+
                          |
                          v
            +----------------------------+
            |  解析目标版本              |
            |  (交互菜单 / --version)    |
            +-------------+--------------+
                          |
                          v
            +----------------------------+
            |  写入版本文件              |
            |  o package manifest        |
            |  o Cargo.toml + Cargo.lock |
            |  o CHANGELOG.md 顶部插入   |
            +-------------+--------------+
                          |
                          v
            +----------------------------+
            |  Commit                    |
            +-------------+--------------+
                          |
                          v
            +----------------------------+
            |  Tag                       |
            +-------------+--------------+
                          |
                          v
            +----------------------------+
            |  原子推送上游分支 + tag    |
            +----------------------------+

  -- [hooks] 在每一步之间执行
  -- --dry-run 跳过所有修改操作（不写文件、不跑 git）
  -- 本地失败：回滚文件 + 取消暂存；push 失败：保留 commit/tag
```

维护者开发和发布流程见英文文档：[CONTRIBUTING.md](CONTRIBUTING.md)。
