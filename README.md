# HIT

HIT（Human-in-the-loop Trader，人工管理的交易执行器）是一个面向多用户的实盘信号算法交易执行器聚合平台。它把现有 `No-Trade-No-Life/traders` 的策略模板固定在一个经过验证的版本中，通过单个 Rust 二进制部署。

> 实盘交易有资金风险。HIT 不会自行生成交易信号；只有交易者被启用，并且外部系统或管理者写入目标信号后，专用策略才会向交易所提交订单。

## 核心边界

- **交易凭证（credential）**：交易所 API 密钥或 CTPD API Key。凭证以 UUID 标识，机密字段经 AES-256-GCM 加密后才写入 SQLite；浏览器永远不能读取它们。
- **执行参数（params）**：一个策略的稳定配置，例如合约、容差和单笔上限。
- **目标信号（signal）**：策略应当执行到的目标仓位。它与参数独立保存，可由外部信号源更新。
- **信号 API Key**：每个交易者生成独立的 `sk-…` 密钥。HIT 只保存它的 SHA-256 哈希；完整密钥只会在创建或轮换时返回一次。

HIT 不提供通用交易所抽象。Binance UM Futures、OKX Swap 与 CTPD 分别由 `traders` 中的专用策略逻辑执行。

## 已支持的策略模板

第一版包含来自 `traders` 的所有当前模板，至少覆盖：

- Binance UM Futures：目标仓位、BBO 分方向挂单；
- OKX Swap：目标仓位、BBO、分方向、singleflight、多笔 maker；
- CTPD：中金所 IF/IH/IC/IM 股指期货昨仓优先对冲执行。

策略配置会在创建和外部更新信号时用原始 Rust 结构体反序列化校验。凭证类型必须和策略交易所匹配。

## 认证与权限

- 前端使用 `auth-mini-react-components` 对接 [Auth Mini](https://auth.ntnl.io)；其回调主机名自然成为 JWT audience。
- 后端使用 `auth-mini-axum` 直接验证 Auth Mini JWKS 与 `hit.ntnl.io` audience。
- 第一个登录并确认初始化的用户成为 `root_user_id`，存放于 SQLite `app_meta`。root 可以查看所有用户的非机密资源；普通用户只能管理自己的资源。
- Linkit 通知是每位用户独立配置的 Bot 凭证。交易执行失败时，HIT 通过 [Linkit Bot API](https://linkit.ntnl.io) 向该用户配置的用户名发送私信。

## 外部更新目标信号

```bash
curl --fail-with-body https://hit.ntnl.io/signal/v1/traders/TRADER_UUID \
  -X PATCH \
  -H 'Authorization: Bearer sk-REPLACE_WITH_SIGNAL_KEY' \
  -H 'Content-Type: application/json' \
  --data '{"signal":{"target_qty":"0"}}'
```

不同模板需要不同的 `signal` 字段。创建交易者时，HIT 会预填对应模板的示例 JSON。请求成功只更新目标信号，下一轮已启用交易者执行时会读取新信号。

## 本地开发

需要 Rust 1.93 与 Node.js 24：

```bash
cd web && npm ci && npm run build
cd .. && cargo test --all-targets --all-features
cargo run
```

服务监听 `127.0.0.1:8080`。SQLite 位于 `~/.hit/default.sqlite3`，启动时设置 `journal_mode=WAL`（写前日志模式，允许读取在写入时继续进行）。主机本地的 `~/.hit/credential.key` 以 0600 权限保存加密密钥；丢失它会使已保存的交易凭证不可恢复。

## 发布

`main` 的 GitHub Actions 会构建嵌入前端的 Linux 二进制、创建 GitHub Release，并用 AWS Systems Manager（SSM，AWS 的远程命令服务）部署到 EC2。首次主机引导使用：

```bash
sudo bash deploy/bootstrap-ubuntu.sh
```

部署脚本只接受 GitHub Release 的校验和匹配产物。运行账户为无登录权限的 `hit` 用户；状态位于 `/var/lib/hit/.hit/`。

## 许可证与来源

HIT 采用 [MIT](LICENSE) 许可证。执行策略通过固定 Git 提交引用 `No-Trade-No-Life/traders`，该项目同样采用 MIT 许可证；HIT 新增的用户、凭证、信号与运行管理层并不改变原策略的交易所专用边界。
