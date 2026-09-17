# AGENTS.md

## Git 工作流约定

- **推送规则：每次 `git push` 必须同时推送到两个远端平台**：
  - `origin` — GitHub（git@github.com:vcqr/ashell.git）
  - `gitee` — Gitee（git@gitee.com:vcqr/ashell.git）
- 即：`git push origin main && git push gitee main`（或 `git push --all` 覆盖两远端时逐个确认都成功）。两个远端任一失败都要报告，不允许只推其一。
- 提交说明使用中文，遵循 `feat: / fix: / chore:` 前缀的既有风格。
- 提交说明要简介明了，长度控制在 10-30 个字（不含前缀），一句话说清改动即可，不在正文里展开细节。
