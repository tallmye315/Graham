# 使用 Tauri 2 构建 Windows 桌面应用

2026-10-24，用户确认以 Tauri 2 + React/TypeScript 实现 Graham，基础界面采用 shadcn/ui、图表采用 ECharts。选择系统 WebView2 与 Rust 原生能力的组合，沿用已确认的网页界面设计，由原生端提供本地文件与数据库能力；接受额外配置 Rust 和 Windows 构建工具的成本，而非采用随应用携带 Chromium/Node 的 Electron。用户确认当前用户安装 .exe、手动升级，并优先支持干净机器离线安装，随包提供缺失 WebView2 时所需的离线安装程序；Python 数据进程的分发取舍见 ADR-0003，不能从此决策推导出已完成安装包或离线验收。
