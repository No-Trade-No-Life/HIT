import type { Locale } from "./types"

export const copy = {
  zh: { overview: "总览", traders: "交易者", credentials: "交易凭证", linkit: "Linkit 通知", admin: "管理后台", createTrader: "新建交易者", createCredential: "新建凭证", noTraders: "还没有交易者", noCredentials: "还没有交易凭证", setup: "设为 Root 管理员", refresh: "刷新", enabled: "运行中", stopped: "已停止", failed: "失败", token: "信号 API Key", credential: "凭证", template: "策略模板", signal: "目标信号", params: "执行参数", runs: "最近运行", save: "保存", cancel: "取消", delete: "删除", language: "语言", root: "Root", owner: "用户", status: "状态", execution: "执行", enableTrader: "启动交易者", disableTrader: "停止交易者", traderEnabled: "交易者已启动", traderStopped: "交易者已停止", updated: "更新于", detail: "运行详情", linkitDescription: "保存 Linkit Bot 的 sk- Token 后，交易执行失败会向你的 Linkit 用户名发送私信。", configured: "已配置", notConfigured: "未配置", signin: "正在验证登录状态…", usage: "外部信号使用以下密钥调用 PATCH /signal/v1/traders/{trader_id}，请求体为 { signal: {...} }。完整密钥只显示一次。" },
  en: { overview: "Overview", traders: "Traders", credentials: "Trading credentials", linkit: "Linkit notifications", admin: "Admin", createTrader: "Create trader", createCredential: "Create credential", noTraders: "No traders yet", noCredentials: "No trading credentials yet", setup: "Become root administrator", refresh: "Refresh", enabled: "Running", stopped: "Stopped", failed: "Failed", token: "Signal API key", credential: "Credential", template: "Strategy template", signal: "Target signal", params: "Execution parameters", runs: "Recent runs", save: "Save", cancel: "Cancel", delete: "Delete", language: "Language", root: "Root", owner: "User", status: "Status", execution: "Execution", enableTrader: "Start trader", disableTrader: "Stop trader", traderEnabled: "Trader started", traderStopped: "Trader stopped", updated: "Updated", detail: "Runtime details", linkitDescription: "Save a Linkit Bot sk- token to receive direct messages when one of your trader executions fails.", configured: "Configured", notConfigured: "Not configured", signin: "Checking your sign-in state…", usage: "External signals call PATCH /signal/v1/traders/{trader_id} with { signal: {...} }. The full key is shown once." },
} as const

export type Copy = Record<keyof typeof copy.zh, string>

export function localeText(t: Copy, english: string, chinese: string) {
  return t.overview === "总览" ? chinese : english
}

export function initialLocale(): Locale {
  return navigator.language.startsWith("zh") ? "zh" : "en"
}
