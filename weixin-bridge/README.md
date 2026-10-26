# weixin-bridge

微信通知桥：把本地 HTTP 请求转发到**个人微信**。用于让 AShell 的通知模块（`custom` 平台）把 AI 审批提问、任务完成等事件推送到微信。

协议层抽取自腾讯官方的 [`@tencent-weixin/openclaw-weixin`](https://github.com/Tencent/openclaw-weixin)（OpenClaw 微信渠道插件，MIT），请求格式与其保持一致；这里去掉了 OpenClaw 依赖，只保留「登录 + 保活 + 发文本」三件事。

## 工作原理

- 走微信 iLink 机器人后端（`ilinkai.weixin.qq.com`），**扫码登录**拿 `bot_token`，凭证存 `~/.weixin-bridge/credentials.json`（0600）。
- 启动后维持 **getUpdates 长轮询**：一是保活，二是接收你发给机器人的消息，从中采集 `context_token`（回复时透传可提高可达性）。长轮询的断点 `get_updates_buf` 持久化到本地，重启不丢消息。
- 本地起一个 **HTTP 服务**（默认 `127.0.0.1:7420`），`POST /notify` 即推送一条微信文本消息。
- token 被服务端判定失效（errcode -14）时按官方插件同样的策略冷却 1 小时并提示重新扫码。

> ⚠️ 这是微信面向个人 AI 助手的内部 bot 接口，不是公开商业 API。协议可能随版本变动，也存在账号风控的不确定性，请自行评估使用场景。

## 使用

要求 Node ≥ 22.13。

```bash
cd weixin-bridge
npm install

npm run login     # 终端出二维码，手机微信扫码；如提示配对数字则输入
npm start         # 启动服务（HTTP + 长轮询保活）
```

登录成功后，**在微信里找到机器人会话，随手发一条消息**（例如「绑定」）。这一步很关键：让桥采集到你的 `context_token`，之后推送更可靠。可用 `npm run dev -- peers` 查看已配对用户。

手动测试推送：

```bash
curl -X POST http://127.0.0.1:7420/notify \
  -H 'Content-Type: application/json' \
  -d '{"title":"测试通知","content":"**hello** 微信桥已就绪"}'
```

## HTTP 接口

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| POST | `/notify`（`/send` 等价） | 推送消息，见下方 body |
| GET | `/status` | 登录/轮询/配对状态 |
| GET | `/peers` | 已配对用户及 context token 采集时间 |

`/notify` 请求体（JSON）：

```jsonc
{
  "title": "标题（可选）",
  "content": "正文，支持 markdown 语法，发送前会自动转纯文本",
  "to": "可选，指定接收者 ilink 用户 ID，缺省推给扫码者/最近配对用户",
  "format": "plain"   // 可选，跳过 markdown 清洗
}
```

非 JSON 请求体按纯文本处理。超过 1800 字符自动拆分为多条。设置了 `WEIXIN_BRIDGE_TOKEN` 时需带 `Authorization: Bearer <token>`。

## 环境变量

| 变量 | 默认 | 说明 |
| --- | --- | --- |
| `WEIXIN_BRIDGE_PORT` | `7420` | HTTP 端口 |
| `WEIXIN_BRIDGE_HOST` | `127.0.0.1` | 监听地址 |
| `WEIXIN_BRIDGE_TOKEN` | 无 | 设置后所有接口需 Bearer Token |
| `WEIXIN_BRIDGE_STATE_DIR` | `~/.weixin-bridge` | 凭证与状态目录 |
| `WEIXIN_BRIDGE_DEBUG` | 关 | `=1` 输出调试日志 |

## AShell 对接（custom 平台）

设置 → 通知 → 平台选「自定义 webhook」：

- **Webhook URL**：`http://127.0.0.1:7420/notify`
- **消息体模板**：

```json
{"title":"{title}","content":"{markdown}"}
```

若桥设置了 `WEIXIN_BRIDGE_TOKEN`，模板需自带鉴权头字段时，请改用带 token 的反向代理或在模板里加入桥支持的字段（当前桥只解析 JSON body，鉴权头由 Rust 侧发送时是否可带请以通知模块实际支持为准）。

## 常见问题

- **`/notify` 返回「没有可用的推送目标」**：还没配对。启动桥之后用微信给机器人发一条消息，或请求里带 `to` 字段。
- **推送报「会话冷却中」**：token 已失效（-14）。重新 `npm run login` 扫码即可。
- **消息收不到但 `/status` 一切正常**：检查是否给扫码账号推送（`/peers` 里的 userId），以及 content 清洗后是否为空（`format: "plain"` 可跳过清洗）。
- **想换号**：`npm run dev -- logout` 清除凭证后重新登录。
