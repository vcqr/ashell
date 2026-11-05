# sudo 密码自动填充 · 误报构造用例集

> 状态：**已收敛**（2026-11-05）。三轮加固后探针 66/66 全部符合预期。
> 设计原则：**宁可不报，不要误报**——误报的代价是把 SSH 登录密码明文注入命令行（回显可见、进 shell history）。
>
> 关联代码：
> - 后端检测 `src-tauri/src/handlers/terminal.rs:1135` `detect_sudo_password_prompt`
> - 后端注入门禁 `src-tauri/src/handlers/terminal.rs:1768` `ClientMsg::SudoFill`（`sudo_prompt_pending_at` + `SUDO_FILL_MAX_AGE`，`terminal.rs:1687` / `:479`）
> - 前端状态机 `src/composables/useSudoFill.ts`
> - 前端确认入口 `src/components/TerminalView.vue:111` `onSudoHintClick`（点击提示条 / Ctrl+Enter）

---

## 一、怎么跑

```bash
./scripts/sudo-fp-probe.sh              # 跑全部 66 例，打印期望/实际对照表
./scripts/sudo-fp-probe.sh --strict     # 额外打印多方案判定对比（历史评估用）
./scripts/sudo-fp-probe.sh --only=B01   # 只跑 id 含 B01 的用例
```

退出码 `0` = 全部符合预期，`1` = 有用例与预期不符（即检测行为发生了变化）。

探针的实现方式：脚本按函数名从 `handlers/terminal.rs` **逐字抽取** `sanitize_prompt_text` / `tail_chars` / `last_input_line` / `last_input_requests_elevation` / `input_line_echoes_colon` / `detect_sudo_password_prompt`，与 `scripts/sudo-fp-cases.rs` 拼成临时 crate 用 `rustc` 编译。
**不抄写函数**——抄写会与源码漂移，下次改了检测逻辑就测不准。改源码后重跑脚本，用例结论自动跟着变。

用例本体在 `scripts/sudo-fp-cases.rs`，每个用例带 `expect_hit`（期望是否命中）和 `why`（说明）。新增用例直接往 `CASES` 数组里加一行。

改动检测逻辑后的验收标准：`./scripts/sudo-fp-probe.sh` 退出码 0，且 `cargo test --lib` 全绿（`handlers::terminal::tests` 内有同源的 52 项单测）。

---

## 二、判定链（现状）

检测函数从后到前：

```
输出末行（末尾 320 字符内，去 ANSI、小写归一）
  │
  ├─ 不以 : 或 ： 结尾 ──────────────────────────→ 不命中
  │
  ├─ [sudo] 开头 + 密码词 + ≤64 字符 ────→ 命中 ★强信号：不看输入侧
  │                                        （重试识别靠它：命中后输入缓冲已清空）
  ├─ doas 开头 + 密码词 + 最近输入是提权命令 ──→ 命中（裸前缀弱于 [sudo]，需佐证）
  │
  ├─ 弱信号，逐项过护栏：
  │    ├─ 提权命令是最后一条输入命令的【首词】（grep sudo / ssh host sudo 不算）
  │    ├─ 输入末行以 : / ： 结尾 → 不命中（正在键入的命令回显）
  │    ├─ 提示行必须紧跟含 sudo/doas 的命令回显行
  │    │    （sudo 本体要密码紧随回显；sudo docker login / sudo ftp
  │    │      先打印自己的输出再要密码，隔着内容的是别家在要密码）
  │    ├─ 含密码词 → 整行恰为 "password:"（macOS/doas 裸提示）
  │    └─ 无密码词 → ≤24 字符且含字母数字（sudo -p 自定义短提示）
  │
  └─ → 不命中
```

密码词表：`password` / `密码` / `contraseña` / `mot de passe` / `passwort` / `senha` / `пароль`。
提权命令：`sudo` / `sudoedit` / `doas`，且必须是输入命令行**首词**（token 精确匹配）。

## 三、注入侧门禁（第二道防线）

检测命中 ≠ 会注入。`sudo_prompt_pending_at: Option<Instant>`：

- 命中后**任何新输出**（下一按键回显、命令后续输出）→ 立即作废 + 下发 `sudo_prompt_gone` 撤掉前端提示条
- 命中后**任何键盘输入**（`ClientMsg::Cmd`）→ 同上
- 注入有效期 20 秒（`SUDO_FILL_MAX_AGE`，前端待填充窗口 15 秒 + 在途余量），注入一次性（`take()`）

依据：真实密码提示等待期间终端静默且用户不该敲键；误报命中后必然跟着新输出/输入。
`read -p` / python `input()` / `tail -f` 这类静默等待程序会骗过"无新输出"检查，由 20 秒超时兜底。

## 四、确认交互（第三道防线）

**回车不再确认填充**。回车是最高频按键，误命中时回车确认会把密码明文注入命令行。
确认只走两条显式路径：

- **点击提示条**（`onSudoHintClick`）
- **Ctrl+Enter**（`onTermCustomKey`，吞键不发 PTY）

待填充期间敲任意键（含回车）一律取消自动填充并照常透传。真实提示下按回车会提交空密码，sudo 重问并重新武装，再点提示条即可。

## 五、用例总览

66 例全部符合预期（2026-11-05，提交 `50b068c` + 探针修订后）。

| 分组 | 用例数 | 状态 | 说明 |
|---|---|---|---|
| A 真提示 | 14 | 全过 | 含 6 语种本地化、ANSI 颜色、doas、sudo -p 自定义提示 |
| B 误报·强信号 | 12 | 全拦 | journalctl / man / cat 文件 / 伪造提示 / visudo 子串 / JSON / 大写… |
| C 误报·弱信号 | 10 | 全拦 | grep sudo / ssh host sudo / docker login / ftp / redis / vim 内提示… |
| D 误报·回显瞬态 | 7 | 全拦 | 键入 sudo chown root: / sudo grep password x: / vim 冒号… |
| E 护栏·他人口令 | 11 | 保持 | mysql / 嵌套 ssh / passwd / psql / apt / NOPASSWD / read -p… |
| F 漏报 | 3 | 已接受的漏报 | 见下节，宁可不报 |
| G 边界 | 9 | 全过 | 空输出 / 320 窗口 / 512 输入上限 / 全角冒号 / 词位置… |

## 六、已接受的漏报（宁可不报，有意为之）

| 场景 | 原因 | 兜底 |
|---|---|---|
| `sudo -p 'pwd: '` 重试（输入缓冲已清空） | 弱信号依赖输入缓冲（F01） | 手动输入 |
| 裸 `Password:` 重试（输入缓冲已清空，F02） | 同上 | macOS 重试由下一节强信号兜住；doas 重试手动输 |
| sudo lecture 多行警告夹在提示行前 | 紧邻回显护栏无法越过夹层 | 首帧手动输，之后有凭证缓存 |
| 长命令折行后提示行前一行是折行尾部 | 同上 | 手动输入 |
| 本地化裸提示（不带 `[sudo]`/`doas` 词，如纯 `密码：`） | 裸提示只认整行 `password:`（G05） | 手动输入 |

## 七、历史：修复前的三类误报（已全部修复）

修复前（提交 `f94e252` 及之前）的判定链里，强信号分支是 `末行含 sudo/doas + 密码词 → 命中`，
完全不看输入上下文；弱信号提权判据只认"最近输入含 sudo token"。实测 24 例误报：

- **强信号绕过输入校验**（B 组 11 例）：journalctl 日志行、man 文档、cat 文件内容、伪造提示、
  `visudo` 子串、大写提示、键入 `sudo grep password x:` 的回显瞬态（D02/D04）——全部命中。
- **弱信号 token 不认语义**（C 组 10 例）：`grep sudo /etc/x`、`ssh host sudo`、`echo sudo`
  都算提权，随后裸 `Password:` 全部命中，登录密码被填进 docker registry / ftp / redis 等别家口令框。
- **危害链**：误报命中 → 前端提示条 → 静默等待程序（`read -p`/`input()`/`tail -f`）不产生新输出
  → pending 持续有效 → 按回车注入 → PTY echo 开着时密码明文上屏、进 shell history。

对应的修复即上文第二～四节，回归用例进 `handlers::terminal::tests`（伪造提示专项）+ 本探针。
探针 `--strict` 保留"简单收紧 vs 行首锚定"的多方案对比实现，供后续调整时评估收益
（结论：简单收紧漏报 8 例伤真提示，负收益；行首锚定是正确方向）。
