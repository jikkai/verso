<p align="center">
  <img src="brand/verso-readme-banner.svg" alt="Verso" width="800" />
</p>

[English](README.md) | [简体中文](README.zh-CN.md)

[![CI](https://github.com/jikkai/verso/actions/workflows/ci.yml/badge.svg)](https://github.com/jikkai/verso/actions/workflows/ci.yml)
[![npm version](https://img.shields.io/npm/v/@amamo/verso.svg)](https://www.npmjs.com/package/@amamo/verso)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Verso 把每个已配置的 JavaScript workspace 发布组作为一个版本发布。它会发现 package manifest，按需
更新配置的 Cargo package 和 Conventional Commit changelog，创建 release commit 与 annotated tag，
再原子推送当前 upstream 分支和这个准确的 tag。

Verso 刻意在这里结束。Registry 发布、GitHub Release、二进制构建和部署应交给由 tag 触发的 CI。

## 运行条件

- Node.js 22.18 或更高版本。
- Git、具名分支和已配置的 upstream。
- 受支持的平台：macOS arm64/x64、Linux GNU arm64/x64 或 Windows x64。
- 包含有效 SemVer 的 `package.json`、`package.json5`、`package.yaml` 或 `package.yml`。

## 快速开始

使用你偏好的 package manager 安装 `@amamo/verso`：

```sh
# npm
npm install --save-dev @amamo/verso
# pnpm
pnpm add --save-dev @amamo/verso
# Yarn
yarn add --dev @amamo/verso
# Bun
bun add --dev @amamo/verso
```

下面的示例表示 Verso 的 CLI 语法。请通过 package manager 或项目已有任务调用本地 executable；
[快速开始](https://jikkai.github.io/verso/zh-CN/getting-started/)列出了每种受支持 package manager 的
准确命令。

单包仓库不需要配置文件。Verso 默认包含根 package。如果 workspace 根 manifest 不应共用发布版本，
请创建含显式 package 模式的 `verso.toml`：

```toml
[workspaces]
patterns = ["packages/*"]
include_root = false
```

先检查仓库和一个准确的发布计划：

```sh
verso doctor
verso --dry-run --version 1.4.0
```

Dry-run 会执行与真实运行相同的转换计算，并输出每个修改文件实际的 before/after diff，但不会写入。

计划无误后，使用交互模式，或为自动化提供准确版本：

```sh
verso
verso --version 1.4.0 --yes
```

`--yes` 只接受确认，不会选择版本。

如果只想为 release PR 准备版本修改，可以使用 `bump`：

```sh
verso bump minor
verso bump --version 1.4.0
```

`bump` 会更新 package/Cargo manifest 以及匹配的 Cargo lock 记录，但不会更新 changelog，也不会创建
commit、tag 或 push。

## 发布模型

```text
配置 + manifest + Git 历史
  -> 检查一个发布组并确定目标 SemVer
  -> 计算准确的 before/after 文件修改
  -> 持久化事务 -> 更新文件 -> commit -> annotated tag
  -> git push --atomic <upstream-branch> <exact-tag> -> 清除事务
```

- `verso doctor` 会检查配置、package 发现、版本、changelog 路径、Cargo package 和分支 upstream，
  但不会开始发布。
- `verso --dry-run` 会输出准确的 before/after 文件 diff、hook、警告和 Git 命令，不写文件，也不修改 Git。
- `verso bump patch|minor|major` 或 `verso bump --version <SEMVER>` 只应用版本文件修改。
- 真正发布默认要求工作区干净。宽松模式仍要求 index 和 release 文件干净。
- `verso status`、`verso resume` 和 `verso abort` 可以检查、继续或安全回滚中断的事务。Push 一旦开始，
  abort 就会被禁用，恢复只能通过 `resume` 继续。

完整状态矩阵见[发布流程](https://jikkai.github.io/verso/zh-CN/release-workflow/)。

## 配置

所有配置项都可选。`verso init` 可以生成初始文件。`--config <PATH>` 接受相对或绝对文件路径，并把其
所在目录作为发布根目录；文件内路径必须保持相对于该根目录。一个配置对应一个发布组，`--group core`
会选择 `verso.core.toml`。独立版本组应使用独立配置。

完整说明见[配置参考](https://jikkai.github.io/verso/zh-CN/configuration/)和
[CLI 参考](https://jikkai.github.io/verso/zh-CN/cli-reference/)。

## 边界

Verso 让每个已配置发布组共用一个统一版本和 tag。独立版本组使用独立配置，并且一次只发布一个组；同一
组内不支持独立版本。使用默认 tag 模板的命名组会自动生成 `core-v1.2.3` 这类 tag，避免组间冲突。
Verso 也不支持非原子推送、本地 registry 发布或在本地创建 GitHub Release。

维护者开发和发布流程见 [CONTRIBUTING.md](CONTRIBUTING.md)，安全问题请按 [SECURITY.md](SECURITY.md)
报告。

## License

MIT
