# Issue tracker: GitHub

任务与规格保存在 `tallmye315/Graham` 的 GitHub Issues 中。
使用 gh CLI，所有仓库级操作显式指定仓库。

## 常用操作

- 创建：gh issue create --repo tallmye315/Graham --title "标题" --body-file <正文文件>
- 读取：gh issue view <编号> --repo tallmye315/Graham --json number,title,body,labels,comments
- 列表：gh issue list --repo tallmye315/Graham --state open --json number,title,body,labels
- 评论：gh issue comment <编号> --repo tallmye315/Graham --body-file <评论文件>
- 添加标签：gh issue edit <编号> --repo tallmye315/Graham --add-label "<标签>"
- 移除标签：gh issue edit <编号> --repo tallmye315/Graham --remove-label "<标签>"
- 关闭：先发表评论说明结果，再运行 gh issue close <编号> --repo tallmye315/Graham

多行正文写入 UTF-8 临时文件后，通过 --body-file 传入。
分诊标签映射见 `triage-labels.md`。

## Pull requests as a triage surface

**PRs as a request surface: no.**

## 技能指令的含义

“发布到任务跟踪器”：创建 GitHub Issue。
“获取相关任务”：读取指定 Issue 及其评论。

## Wayfinding operations

- Map：使用带有 `wayfinder:map` 标签的 Issue，记录 Notes、
  Decisions-so-far 和 Fog。
- 子任务：优先使用 GitHub 子 Issue 关联 Map；不可用时，
  在子任务正文顶部写 `Part of #<Map编号>`，并在 Map 中维护任务清单。
- 类型：使用 `wayfinder:research`、`wayfinder:prototype`、
  `wayfinder:grilling` 或 `wayfinder:task` 标签。
- 阻塞关系：优先使用 GitHub 原生 Issue dependencies；
  不可用时在正文顶部写 `Blocked by: #<编号>, #<编号>`。
  所有阻塞任务关闭后，当前任务才可推进。
- Frontier：按 Map 中的顺序，选择首个未关闭、无阻塞且无人领取的子任务。
- 领取：开始处理前，将任务指派给当前用户。
- 完成：评论记录答案，关闭子任务，再将结论摘要和链接补充到
  Map 的 Decisions-so-far。
