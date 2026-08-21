# Changelog

## [1.2.0](https://github.com/jikkai/verso/compare/v1.1.0...v1.2.0) (2026-08-21)

### Features

* **release:** improve transaction recovery controls ([525be2d](https://github.com/jikkai/verso/commit/525be2de1d42d116382006fcb829e2a8ec4961b2))

## [1.1.0](https://github.com/jikkai/verso/compare/v1.0.1...v1.1.0) (2026-08-14)

### Features

* **release:** add grouped transactional release workflow ([5c74305](https://github.com/jikkai/verso/commit/5c743052d197cb6fa7f8a405c48b1b745b7a0009))

### Other Changes (chore)

* **deps:** bump @amamo/doctrine to 0.1.1 in apps/docs ([f76491f](https://github.com/jikkai/verso/commit/f76491f637d9348bc30804427fc7d7b1babc4218))
* **docs:** migrate docs app from fumadocs/waku to @amamo/doctrine ([4dfc2c0](https://github.com/jikkai/verso/commit/4dfc2c0d131428023cfb747f49a8052ee46522e0))

### Other Changes (docs)

* refresh release documentation ([7746e9b](https://github.com/jikkai/verso/commit/7746e9bc127be05774d4f237c93672493601d8f3))

## [1.0.1](https://github.com/jikkai/verso/compare/v1.0.0...v1.0.1) (2026-08-06)

### Bug Fixes

* **config:** disable changelog generation by default ([e3b3d1f](https://github.com/jikkai/verso/commit/e3b3d1fee513130429553d09752e3383be00ea4b))

### Other Changes (chore)

* update dependencies ([3e4d5ea](https://github.com/jikkai/verso/commit/3e4d5eaaafa9ed8751954be6decde9b8ec8d24b6))

## [1.0.0](https://github.com/jikkai/verso/compare/v1.0.0-rc.4...v1.0.0) (2026-08-01)

### Other Changes (chore)

* **tooling:** centralize lint and formatting configuration ([#11](https://github.com/jikkai/verso/issues/11)) ([b2d6e45](https://github.com/jikkai/verso/commit/b2d6e4598e57fc88113cd38020c3a38bb6a16e51))

## [1.0.0-rc.4](https://github.com/jikkai/verso/compare/v1.0.0-rc.3...v1.0.0-rc.4) (2026-07-31)

### Bug Fixes

* **workspace:** respect ignored workspace roots ([cb111cd](https://github.com/jikkai/verso/commit/cb111cd3d3fdab8f29d12f4b3a09fff7ed6c9bc5))

### Features

* **cli:** improve diagnostic output ([199c503](https://github.com/jikkai/verso/commit/199c50342b1fdc9d629c4f23a4ef0b1a6100811e))

### Other Changes (ci)

* **release:** use GH_TOKEN for checkout ([6f9a2dc](https://github.com/jikkai/verso/commit/6f9a2dccb1eb8d0683b5a5d790b2b20f070031ad))

## [1.0.0-rc.3](https://github.com/jikkai/verso/compare/v1.0.0-rc.2...v1.0.0-rc.3) (2026-07-30)

### Bug Fixes

* **release:** scope Cargo.lock updates to nearest manifest ([6d670d6](https://github.com/jikkai/verso/commit/6d670d68c2efb347a59bd51acbd7141892e239e0))

### Other Changes (build)

* **deps:** bump actions/setup-node from 6 to 7 in the github-actions group across 1 directory ([fb441b1](https://github.com/jikkai/verso/commit/fb441b1e5a1b96ed289f2b2c5f699dec9da8be95))

### Other Changes (chore)

* transfer repository to jikkai/verso and migrate npm scope to @amamo ([418dfe3](https://github.com/jikkai/verso/commit/418dfe3a49552ed29e892ffe8e4316522d33cc40))

### Other Changes (ci)

* **release:** use github.token for checkout ([15d0bb9](https://github.com/jikkai/verso/commit/15d0bb943c692b13bb14413ef8bf737f7a4cc0dd))
* add GH_REPO environment variable for docs workflow ([0d0a6e0](https://github.com/jikkai/verso/commit/0d0a6e0565d9dc5b25044131da7923640a45ca7b))

### Other Changes (docs)

* serve English documentation from root path ([e807b63](https://github.com/jikkai/verso/commit/e807b63578aa6448d0c6950fad1c9a2d6e09123d))

## [1.0.0-rc.2](https://github.com/jikkai/verso/compare/v1.0.0-rc.1...v1.0.0-rc.2) (2026-07-21)

### Features

* **release:** harden release flow and refresh tooling ([cf7e2e7](https://github.com/jikkai/verso/commit/cf7e2e75ed0b324ce6789fdfeb6c50d03a0398d9))

### Other Changes (docs)

* add docs site ([0e4c515](https://github.com/jikkai/verso/commit/0e4c5153131d2187b25349651807191ba2354e7a))

## [1.0.0-rc.1](https://github.com/jikkai/verso/compare/v1.0.0-rc.0...v1.0.0-rc.1) (2026-06-30)

### Other Changes (build)

* keep published dist output flat ([37c8bd3](https://github.com/jikkai/verso/commit/37c8bd3b6a4962cfea6b2935936cacd7d7fd3f88))

## [1.0.0-rc.0](https://github.com/jikkai/verso/compare/v1.0.0-beta.3...v1.0.0-rc.0) (2026-06-29)

### Bug Fixes

* normalize dry-run paths across platforms ([01e25f0](https://github.com/jikkai/verso/commit/01e25f03e0d79c772378d304011574e342f26f44))

### Features

* **release:** add styled dry-run output ([fc7c18c](https://github.com/jikkai/verso/commit/fc7c18cdee4f0a67be3958186c14fade84929ca9))

### Other Changes (chore)

* simplify CI configuration and update format scripts ([62b4915](https://github.com/jikkai/verso/commit/62b49158d574474a0eb02c291ffd28c73faf14ec))
* add pre-commit formatting and lint checks ([08cb668](https://github.com/jikkai/verso/commit/08cb668ef586568ce2b882a92d26d9c992ee6a46))

### Other Changes (doc)

* update README ([fc91e92](https://github.com/jikkai/verso/commit/fc91e92c8428d9784ed9eefdabceb51a70eff71d))

## [1.0.0-beta.3](https://github.com/jikkai/verso/compare/v1.0.0-beta.2...v1.0.0-beta.3) (2026-06-29)

### Features

* support package manager manifests and interactive release prompts ([58bde75](https://github.com/jikkai/verso/commit/58bde751b17f05711b34b2d7d324c1dfbdf475b9))
* support alternate package manifests and workspace metadata ([f9ecb79](https://github.com/jikkai/verso/commit/f9ecb79846b487cdff91b3e4cc93a34c350368cc))

### Other Changes (doc)

* update readme ([b15bf2d](https://github.com/jikkai/verso/commit/b15bf2d23b9e539be55edd18e65e0158c0e33692))

## [1.0.0-beta.2](https://github.com/jikkai/verso/compare/v1.0.0-beta.1...v1.0.0-beta.2) (2026-06-27)

### Features

* support configless single-package releases ([2803edc](https://github.com/jikkai/verso/commit/2803edc2b02fe22bce2fdd2b897154e7107494ee))

## [1.0.0-beta.1](https://github.com/jikkai/verso/compare/v1.0.0-beta.0...v1.0.0-beta.1) (2026-06-27)

### Features

* **verso:** add release hooks and expand workspace globs ([93d5ed0](https://github.com/jikkai/verso/commit/93d5ed0ae95bd3e090d2620479d94dd222b62be2))

## [1.0.0-beta.0](https://github.com/jikkai/verso/compare/v1.0.0-alpha.2...v1.0.0-beta.0) (2026-06-25)

### Features

* update release confirmation prompts to default to yes ([5cef40c](https://github.com/jikkai/verso/commit/5cef40cd2e519e7d389620234f03ed176956270a))

## [1.0.0-alpha.2](https://github.com/jikkai/verso/compare/v1.0.0-alpha.1...v1.0.0-alpha.2) (2026-06-25)

### Features

* add step confirmations to release flow ([790124c](https://github.com/jikkai/verso/commit/790124c60a1b3632314276116caf2745d6817fb8))

## [1.0.0-alpha.1](https://github.com/jikkai/verso/compare/v1.0.0-alpha.0...v1.0.0-alpha.1) (2026-06-25)

### Other Changes (chore)

* **release:** add cross-platform npm publishing workflow ([3bacca6](https://github.com/jikkai/verso/commit/3bacca60f09a2f2fc4ed02c169bf82ac4739e654))
* add executable permission handling and normalize CLI arguments ([f026816](https://github.com/jikkai/verso/commit/f026816a4ccb8a8e213e026c3dbeb21153de599a))

## 1.0.0-alpha.0 (2026-06-24)

### Features

* init project ([69cf0e8](https://github.com/jikkai/verso/commit/69cf0e8efd3e7390eb124db1a8b03be915b73b26))

### Other Changes (chore)

* add path mapping for resolver in tsconfig.test.json ([63c1ca4](https://github.com/jikkai/verso/commit/63c1ca4cc6b0ed50d5be0bd733b339ed793fb774))

### Other Changes (ci)

* add prepare release workflow ([009d310](https://github.com/jikkai/verso/commit/009d310fc9aefae006bee65974467a02b0507245))
