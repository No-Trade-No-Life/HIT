# HIT

HIT（Human-in-the-loop Trader，人工管理的交易执行器）是一个面向多用户的实盘信号算法交易执行器聚合平台。它把现有 `No-Trade-No-Life/traders` 的策略模板从经过验证的提交直接收进单一 Rust crate，通过单个二进制部署。

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
- OKX Swap：目标仓位、BBO、分方向、singleflight、多笔 maker，以及目标杠杆 BBO post-only；
- CTPD：中金所 IF/IH/IC/IM 股指期货昨仓优先对冲执行。

策略配置会在创建和外部更新信号时用原始 Rust 结构体反序列化校验。凭证类型必须和策略交易所匹配。

CTPD 模板的 `params` 仅需要 `instrument_id`，目标信号仅需要带符号的 `net_volume`：正数为目标多头，负数为目标空头，`0` 为平仓。每笔实际委托前，HIT 都会从 CTPD 的 Tick（实时行情）流读取最新盘口；买入使用 `AskPrice1`，卖出使用 `BidPrice1`，即按最优对手价（BBO，最佳买卖报价）提交限价单。目标合约必须已在 CTPD 中启用 Tick 订阅；没有新 Tick 时 HIT 会等待，不会复用旧报价报单。

OKX Swap 目标杠杆模板要求 `net_mode`。其 `signal.target_leverage` 是带方向的杠杆：仓位名义价值 / OKX `totalEq`（账户净值），例如 `1` 为 1 倍做多、`-1` 为 1 倍做空、`0` 为平仓。它使用 BBO 的 post-only（只挂单）限价单。开仓、显式 signal 改变、归零或反向后，HIT 会读取当时净值、合约面值和 BBO 来计算目标张数；持仓期间仅因 PnL 导致的净值或实际杠杆变化不会触发调仓。反向会先 BBO post-only 平仓，确认归零后才按最新净值开反向仓。

## 策略模板元信息

`GET /api/templates` 返回所有可用策略模板。每个模板包含 `id`、`name`、`credential_type`、`description`、`params_schema` 和 `signal_schema`；后两项使用 JSON Schema（JSON 结构校验规范）描述可填写字段、必填项、标题和说明。创建交易者时，HIT 会根据所选凭证自动填入账户标识，因此它不属于 `params_schema` 的用户输入字段。

交易者详情页会依据这两份 Schema 同时展示执行参数和目标信号的字段标题、说明与当前值，并提供原始 JSON tab 供人工编辑。参数保存只调用专用的 `PATCH /api/v1/traders/{id}/params`，不会回写目标信号、交易凭证或运行开关。

## 认证与权限

- 前端使用 `auth-mini-react-components` 对接 [Auth Mini](https://auth.ntnl.io)。登录令牌同时包含 `hit.ntnl.io`（HIT 回调主机名）和 `linkit.ntnl.io` 两个 JWT audience（受众，即允许接收该令牌的服务），以便用户在 Linkit 集成中复用同一登录会话。
- 后端使用 `auth-mini-axum` 直接验证 Auth Mini JWKS 与 `hit.ntnl.io` audience。
- 第一个登录并确认初始化的用户成为 `root_user_id`，存放于 SQLite `app_meta`。root 可以查看所有用户的非机密资源；普通用户只能管理自己的资源。
- Linkit 通知是每位用户独立配置的 Bot 凭证。交易执行失败时，HIT 通过 [Linkit Bot API](https://linkit.ntnl.io) 向该用户配置的用户名发送私信。

## 系统资源

仅 root 可以访问【系统资源】页面及 `GET /api/v1/system/resources`。页面每 5 秒采样部署主机的 CPU、内存、SQLite 所在磁盘和 SQLite 文件大小；SQLite 大小合计主库、WAL（写前日志）与 SHM（共享内存）三个文件，反映 WAL 模式下实际占用的磁盘空间。

## 外部更新目标信号

```bash
curl --fail-with-body https://hit.ntnl.io/signal/v1/traders/TRADER_UUID \
  -X PATCH \
  -H 'Authorization: Bearer sk-REPLACE_WITH_SIGNAL_KEY' \
  -H 'Content-Type: application/json' \
  --data '{"signal":{"target_qty":"0"}}'
```

不同模板需要不同的 `signal` 字段。创建交易者时，HIT 会预填对应模板的示例 JSON。请求成功只更新目标信号，下一轮已启用交易者执行时会读取新信号。

## 运行与信号记录

- 每个交易者保存成功执行循环计数器（counter，即只累计成功次数的数值），不再为每一轮成功或失败执行写入一条日志。当前失败原因仍保留在交易者状态中。
- 每次目标信号 PATCH 请求都会保存 payload（请求体中的信号 JSON）历史。连续相同 payload 合并为一条记录，只更新 `updated_at` 并递增出现次数；不同 payload 则创建新的历史记录，包含 `created_at`、`updated_at` 和出现次数。
- 升级到此版本时，旧 `trader_runs` 表及其全部历史数据会被永久删除。

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

HIT 采用 [MIT](LICENSE) 许可证。执行策略来自 `No-Trade-No-Life/traders` 的固定提交 `d368e39836a5779525b4844233728057904513ae`，该项目同样采用 MIT 许可证；HIT 新增的用户、凭证、信号与运行管理层并不改变原策略的交易所专用边界。
