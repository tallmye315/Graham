# 记录纠错与品牌更新（Issue #18）

规格：`docs/product/record-correction-discovery.md`，用户于 2026-10-25 回复“全部按建议”。总 PRD AC30—AC33；导入批次撤回补入 #8，当前仍未实现 CSV 导入。

## 实现

- 入金可修改、可恢复删除；有效记录决定原币余额，删除不生成真实出金。
- 每次录入、修改、删除和恢复都保存独立历史。操作后可立即撤回，重启后可通过记录历史恢复旧版本。历史版本必须属于目标记录。
- 可修改账户名称；有任何历史记录（包括已删除记录）时锁定币种，不允许永久删除。空账户可改币种和删除。归档账户停止新增入金，余额继续计入总览，可取消归档。
- SQLite v1→v2 迁移前用 `VACUUM INTO` 在账本同目录生成唯一命名的一致快照，再事务迁移。失败时返回错误并保留原账本，不替换为空文件。快照仍是本机未加密文件，不是加密导出备份。
- 正式品牌源为 `assets/brand/wordmark.svg` 和 `icon.svg`；后者生成白底酒红 ICO，安装为独立 `graham-icon.ico`。安装器通过 POSTINSTALL hook 创建桌面快捷方式，应用另提供重新创建入口。
- 快捷方式实现依据 [Tauri 安装器配置](https://v2.tauri.app/distribute/windows-installer/) 的 `installerHooks`，保留 currentUser 安装范围。

## TDD 与验证

沿用公开 Ledger 完整应用用例和少量 React / Tauri IPC 交互边界，真实临时 SQLite，不以 SQL 旁路查询验证业务结果。

| 行为 | 先红 | 后绿 |
| --- | --- | --- |
| 修改金额、重启后核对旧值 | [40161828929](https://github.com/tallmye315/Graham/actions/runs/40161828929)：纠错接口尚未实现 | [40161904183](https://github.com/tallmye315/Graham/actions/runs/40161904183) |
| 删除排除余额、重启历史恢复 | [40161990928](https://github.com/tallmye315/Graham/actions/runs/40161990928)：删除接口尚未实现 | [40162058048](https://github.com/tallmye315/Graham/actions/runs/40162058048) |
| 账户纠错、归档及历史保护 | [40162085732](https://github.com/tallmye315/Graham/actions/runs/40162085732)：账户修改尚未实现 | [40162221540](https://github.com/tallmye315/Graham/actions/runs/40162221540) |
| 界面编辑/撤回、删除确认/恢复、账户锁币种 | 本地测试分别因缺少入口失败 | 对应功能完成后通过 |
| 切换记录丢弃迟到的历史响应 | 延迟 Promise 回归复现 A 历史覆盖 B | 独立 RecordHistory 组件清理后忽略成功/失败响应，7 项界面测试通过 |

最终代码 `1a69f24`，流水线 [40162445103](https://github.com/tallmye315/Graham/actions/runs/40162445103)。11 项 Rust 用例包含金额/日期/备注校验、恢复溢出原子性、跨记录版本拒绝、旧版合成账本迁移与可重开的迁移前快照；7 项界面用例覆盖上述交互及原有功能。前端生产构建和类型检查通过。

## 双轴审查

固定点 `main`（`2767e7e3173b52f9c77d3fe50c41c6bc01b2b391`），命令 `git diff main...HEAD`。独立 Standards 与 Spec 审查均发现同一历史请求竞态，修复后分别复核：Standards 硬性违反 0、判断项 0；Spec 剩余发现 0。

## 安装产物验收

2026-10-25 已从最终流水线下载并安装 0.1.1，以该机器的现有 0.1.0 账本升级。注册版本为 0.1.1；安装二进制与该流水线原始二进制仅存在 Tauri 正常的 NSIS bundle marker 差异，内存替换 marker 后逐字节相同。安装目录中的 `graham-icon.ico` 与已选定短标生成的 ICO 逐字节相同。

- 安装包：`artifacts/windows-0.1.1/target/release/bundle/nsis/Graham_0.1.1_x64-setup.exe`，220,406,829 字节。
- 安装包 SHA256：`ea5c1961c355a294a116ae3d8a334a981d6d15fe505c08604c59287199414057`。
- 已安装 EXE SHA256：`423da7013ac9796e2a6a5739c7f510d66cc6be005fc5ed669b7b22b9904c92d3`。
- 原生 `tauri.localhost` 界面显示完整组合 Logo，窗口使用白底短标；既有账户及余额与升级前核对一致，迁移前快照存在。
- 打开账户修改表单核对字段后取消，没有改动实际账户。修改/删除/恢复行为通过独立合成账本和界面测试验证，没有向用户账本插入验收资金。
- 软件内“创建桌面快捷方式”返回成功，桌面 `Graham.lnk` 的目标与独立图标文件均存在。关闭软件、通过该快捷方式重新启动后，获得新的原生窗口且原有账户仍在。
- 带个人账户的验收截图只存于被忽略的 `artifacts/validation/`，不提交到 Git 或上传 GitHub。

产物下载中发现旧版安装器仍在构建缓存中，原工作流通配符把旧版一并收入附件。实际安装前显式选择上述 0.1.1 文件并验证其与最终 EXE 对应；随后把工作流的校验及上传路径改为配置版本对应的唯一文件，避免未来误选旧产物。该 CI 守卫调整经本地路径检查及 Standards 复核，但未再次运行完整工作流；本次应用产物证据仍对应完整通过的 `1a69f24`。

本次不是干净 Windows 全部运行时、未来版本升级或真实券商导入验收；这些门槛仍归发布和导入任务。
