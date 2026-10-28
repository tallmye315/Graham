# Graham 品牌资源

2026-10-25 用户以截图正式选定 D 融合方向。`wordmark.svg` 是蛇环徽记与蛇形 G 的完整 Graham 组合，用于软件内品牌展示；`icon.svg` 为白底酒红的蛇环 `Gra / ham` 短标，用于 Windows 软件图标、安装器与桌面快捷方式。两份 SVG 均从用户确认的原型矢量源独立保存，全部字母为路径，不依赖原型服务器或运行时字体。

使用 `npm run desktop -- icon assets/brand/icon.svg --output src-tauri/icons` 生成 ICO 与 PNG。安装器同时安装独立的 `graham-icon.ico`，快捷方式显式引用该文件，避免沿用旧版可执行文件图标缓存。软件窗口图标使用相同资源。小尺寸和安装产物显示仍需逐版验收。
