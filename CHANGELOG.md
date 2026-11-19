# Changelog
All notable changes to this project will be documented in this file. See [conventional commits](https://www.conventionalcommits.org/) for commit guidelines.

- - -
## [v0.2.0](https://github.com/msound/vugent/compare/60cf2f638688c35a7e308ad152e72ed106b864e3..v0.2.0) - 2026-11-19
#### Features
- open the review's detail page from its header - ([806460d](https://github.com/msound/vugent/commit/806460da1170066e68cf9bb535b3d66b1158bb92)) - [@msound](https://github.com/msound), Claude Opus 5.5
- show a header for the selected review - ([f2a968e](https://github.com/msound/vugent/commit/f2a968ebe8d02f8b221814241b919d48a0a71f86)) - [@msound](https://github.com/msound), Claude Opus 5.5
- read commit details and the branch base - ([07d2dbf](https://github.com/msound/vugent/commit/07d2dbf7d50c06cc01db4a71cfce89b96708e853)) - [@msound](https://github.com/msound), Claude Opus 5.5
- split uncommitted changes into staged and unstaged - ([e39fd31](https://github.com/msound/vugent/commit/e39fd3118198ac233f296cc022cf6e90b6fdd75f)) - [@msound](https://github.com/msound), Claude Opus 5.5
- pick what to review from a commit history sidebar - ([2aa7cf2](https://github.com/msound/vugent/commit/2aa7cf2f059228ce70033462c06e96d1f971e3b7)) - [@msound](https://github.com/msound), Claude Opus 5.5
- serve commits, staged changes and branch reviews - ([c5400d6](https://github.com/msound/vugent/commit/c5400d6d9d26671a47ac1e17e02984865cd2e355)) - [@msound](https://github.com/msound), Claude Opus 5.5
- review the index, a commit or the branch in the git crate - ([4a6b293](https://github.com/msound/vugent/commit/4a6b293a98e1fc2c8ef85cc9f830015b7e3ba3fb)) - [@msound](https://github.com/msound), Claude Opus 5.5
#### Bug Fixes
- read trailers from every footer paragraph - ([135b0cc](https://github.com/msound/vugent/commit/135b0cc8b894dc03bd97f8648dfe008ce6e9e562)) - [@msound](https://github.com/msound), Claude Opus 5.5
#### Documentation
- describe the cocogitto release process - ([925bed7](https://github.com/msound/vugent/commit/925bed784f552ad352df8eaaa5fadfe017a1f271)) - [@msound](https://github.com/msound), Claude Opus 5.5
- add ADR for the review header and detail page - ([d46850a](https://github.com/msound/vugent/commit/d46850a558712172f6b4eee9faf60cbb3a5ff9e3)) - [@msound](https://github.com/msound), Claude Opus 5.5
- add ADR for branch, staged and single-commit review - ([7fd0021](https://github.com/msound/vugent/commit/7fd0021375c2964f42df0947c48cd5619ed8478a)) - [@msound](https://github.com/msound), Claude Opus 5.5
#### Refactoring
- share the rubber band between full-screen pages - ([c489420](https://github.com/msound/vugent/commit/c4894209dfd1738fc48173f9045980fd61a0401d)) - [@msound](https://github.com/msound), Claude Opus 5.5
#### Miscellaneous Chores
- end CHANGELOG.md with a newline - ([60cf2f6](https://github.com/msound/vugent/commit/60cf2f638688c35a7e308ad152e72ed106b864e3)) - [@msound](https://github.com/msound), Claude Opus 5.5

- - -

## [v0.1.0](https://github.com/msound/vugent/compare/8be436b23e59503c79e6a9e211b89f4a7580f08a..v0.1.0) - 2026-11-19
#### Features
- scroll past the end of a diff to close it - ([5117729](https://github.com/msound/vugent/commit/5117729efe50e9f9693ec55f8f5f3024395b0554)) - [@msound](https://github.com/msound), Claude Opus 5.5
- color boxes by the share of lines changed - ([cdb7d16](https://github.com/msound/vugent/commit/cdb7d161e3fa321c754d365bbc14fb7fd04d2c17)) - [@msound](https://github.com/msound), Claude Opus 5.5
- compute change colors on the server - ([5f0ac29](https://github.com/msound/vugent/commit/5f0ac2953e2e2ab600698067a0152ff5e6549182)) - [@msound](https://github.com/msound), Claude Opus 5.5
- show a file's diff in a full-screen panel - ([90bd669](https://github.com/msound/vugent/commit/90bd669d2aba12cc16a5e86bfe9a43d69053cb7b)) - [@msound](https://github.com/msound), Claude Opus 5.5
- draw the folder diagram with Svelte Flow - ([d2537df](https://github.com/msound/vugent/commit/d2537df76fee77a614e2fe06bdbbb934abd16366)) - [@msound](https://github.com/msound), Claude Opus 5.5
- add the changes API - ([b12b330](https://github.com/msound/vugent/commit/b12b330cee5d71b3e039ec560709e312b6c5cff8)) - [@msound](https://github.com/msound), Claude Opus 5.5
- embed the frontend from inside the server crate - ([74503a3](https://github.com/msound/vugent/commit/74503a39cb4678089674d9a972f2f09370934053)) - [@msound](https://github.com/msound), Claude Opus 5.5
- serve the embedded frontend - ([49e0bbc](https://github.com/msound/vugent/commit/49e0bbcdaa4f8534117d29aadff3cde861d040ca)) - [@msound](https://github.com/msound), Claude Opus 5.5
- scaffold the Vite + Svelte frontend - ([8afabc8](https://github.com/msound/vugent/commit/8afabc8b35fc15cffa10678256b4a61277edaf82)) - [@msound](https://github.com/msound), Claude Opus 5.5
- scaffold the Rust workspace - ([dfe0473](https://github.com/msound/vugent/commit/dfe04736551659dbb9af5b26f334578c9e2d3a8c)) - [@msound](https://github.com/msound), Claude Opus 5.5
#### Bug Fixes
- let Svelte Flow fit the initial view - ([9091bc5](https://github.com/msound/vugent/commit/9091bc5a6cf992f5d62db970fdc715410a0c6ba7)) - [@msound](https://github.com/msound), Claude Opus 5.5
#### Documentation
- add ADR for change classification and colors - ([4dfa41d](https://github.com/msound/vugent/commit/4dfa41d1544c0374517a8106a6a180bc538b5af8)) - [@msound](https://github.com/msound), Claude Opus 5.5
- drop the Homebrew tap from the tech stack ADR - ([a3d1cd9](https://github.com/msound/vugent/commit/a3d1cd9da1846066d567a118b05b86252370c1b9)) - [@msound](https://github.com/msound), Claude Opus 5.5
- add ADR for semantic zoom canvas - ([17b657d](https://github.com/msound/vugent/commit/17b657dd5be5be9cd6b0de337ec29f0a9e3f0aac)) - [@msound](https://github.com/msound), Claude Opus 5.5
- add ADR for reviewing uncommitted changes - ([d07cc97](https://github.com/msound/vugent/commit/d07cc97920371d135c364972fbee4f740c61d864)) - [@msound](https://github.com/msound), Claude Opus 5.5
- add ADR for tech stack - ([be7a565](https://github.com/msound/vugent/commit/be7a565cb80813e4822a4d954441e11c161ed5c9)) - [@msound](https://github.com/msound), Claude Opus 5.5
- add AGENTS.md - ([bf3b186](https://github.com/msound/vugent/commit/bf3b186a8231cf215bb5344d3e07cc1e4790c5cf)) - [@msound](https://github.com/msound)
- add Markdown ADR template - ([ac3393f](https://github.com/msound/vugent/commit/ac3393ff57fefd94d985ecf29bf7d574e777d49f)) - [@msound](https://github.com/msound)
#### Continuous Integration
- add the cargo-dist release workflow - ([50c1be5](https://github.com/msound/vugent/commit/50c1be5322e14e3df81bdbbfe46e8d9f2f9073df)) - [@msound](https://github.com/msound), Claude Opus 5.5
- test the Rust server against the built frontend - ([5f90325](https://github.com/msound/vugent/commit/5f903259b3565eeba0e4bce38be81f9ff1eab426)) - [@msound](https://github.com/msound), Claude Opus 5.5
- add workflow for linting and tests - ([b62c1b9](https://github.com/msound/vugent/commit/b62c1b9a55a364fc938bbbacf08e1163801ac2ff)) - [@msound](https://github.com/msound), Claude Opus 5.5
#### Miscellaneous Chores
- configure cocogitto for releases - ([82f0c7f](https://github.com/msound/vugent/commit/82f0c7f531b58a541dd423b74526e2a89b8fe0b0)) - [@msound](https://github.com/msound), Claude Opus 5.5

- - -

Changelog generated by [cocogitto](https://github.com/cocogitto/cocogitto).
