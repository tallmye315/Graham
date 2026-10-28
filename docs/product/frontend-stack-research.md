# Graham 前端技术调研：React、shadcn/ui 与 ECharts

核对日期：2026-10-24。范围：官方 GitHub 仓库、许可证、源码及官方文档。本文不代表已经安装依赖或完成技术迁移。

## 查证结果

| 项目 | 已查证的职责与许可证 | 对 Graham 的工程建议 |
| --- | --- | --- |
| [React](https://github.com/react/react) | 用于构建界面的 JavaScript 库，采用声明式、组件化方式组织界面与状态；MIT。[README](https://github.com/react/react/blob/main/README.md)、[许可证](https://github.com/react/react/blob/main/LICENSE) | 适合组织六个页面、账户与币种筛选、导入步骤及共享状态。 |
| [shadcn/ui](https://github.com/shadcn-ui/ui) | 提供可以复制、修改和组合的组件源码；MIT。[README](https://github.com/shadcn-ui/ui/blob/main/README.md)、[许可证](https://github.com/shadcn-ui/ui/blob/main/LICENSE.md) | 适合按钮、表格、侧栏、日期选择、弹窗和导入表单；将酒红与白色统一落实到主题变量。 |
| [Apache ECharts](https://github.com/apache/echarts) | JavaScript 图表库，采用 Apache-2.0；官方提供交互式、可定制图表。[README](https://github.com/apache/echarts/blob/master/README.md)、[许可证](https://github.com/apache/echarts/blob/master/LICENSE) | 适合收益折线、持仓比例、分红柱图和收益贡献图，统一图表视觉和交互。 |

React 的旧地址 `facebook/react` 当前重定向到 `react/react`；本次已跟随重定向核对官方 README、许可证和包元数据，没有把旧地址读取失败当作项目不存在。[官方仓库](https://github.com/react/react)

## 如何落实酒红与白色

shadcn/ui 官方推荐使用 CSS 变量和语义主题令牌，包括背景、文字、主色、卡片、侧栏、边框、焦点环及图表色。这样可以让按钮、选中态和弹层共享同一主题。工程上建议以酒红作为品牌主色、白色作为内容表面，分别配置盈亏、警告和错误的语义颜色，避免品牌色和金融含义混淆。[官方主题文档](https://ui.shadcn.com/docs/theming)

ECharts 支持主题、全局或系列调色板，以及 `lineStyle`、`areaStyle`、`itemStyle` 等样式。可以让曲线、面积填充和分布图使用同一套酒红主题值；原生 JavaScript 图表配置与界面 CSS 变量需要在封装层做明确映射。[官方样式说明](https://echarts.apache.org/handbook/en/concepts/style/)

收益曲线、资产比例和分红比较分别可以使用官方提供的折线、饼图和柱图能力；这属于图表呈现，收益数据仍应由经过验证的账务与计算模块提供。[折线](https://echarts.apache.org/handbook/en/how-to/chart-types/line/basic-line/)、[饼图](https://echarts.apache.org/handbook/en/how-to/chart-types/pie/basic-pie/)、[柱图](https://echarts.apache.org/handbook/en/how-to/chart-types/bar/basic-bar/)

## 两个需要明确的集成点

1. **shadcn 的 Chart 基于 Recharts，并非 ECharts。** 官方文档和组件源码都可以直接核对到 Recharts。若选 ECharts，可采用 shadcn 的普通 Card、Tabs、Select 等界面组件，在其中放置独立的 ECharts 封装；工程上建议只保留一套主图表引擎，避免同时维护两套主题与交互。[官方 Chart 文档](https://ui.shadcn.com/docs/components/chart)、[Chart 源码](https://github.com/shadcn-ui/ui/blob/main/apps/v4/registry/new-york-v4/ui/chart.tsx)
2. **图表生命周期应与 React 和容器变化协调。** React 的 Effect 可同步外部系统并在依赖变化或卸载时执行清理；ECharts 初始化要求容器已有尺寸，尺寸变化时调用 `resize`，移除容器时应 `dispose`。工程上建议将初始化、数据更新、尺寸监听与清理集中在一个小组件中，并清理监听器，避免重复实例。[React Effect](https://react.dev/reference/react/useEffect)、[ECharts 容器与销毁](https://echarts.apache.org/handbook/en/concepts/chart-size/)

## 建议的采用顺序

- **本轮原型**：直接在 A 方案现有页面中验证酒红与白色，保留已经可以操作的导航和模拟数据。单次配色确认无需立即重写整套原型。
- **正式前端**：建议采用 React 组织界面、shadcn/ui 组织基础组件、ECharts 统一图表；TypeScript 可作为实施时的类型约束选择。安装时再核对具体稳定版本和兼容性，不把仓库开发分支版本当作已经选择的发布版本。
- **Windows 交付**：以上三个项目负责界面与图表；Windows 桌面运行、安装包、本地存储、导入适配、行情接入和金融计算仍是独立的工程范围。正式工程阶段另行确定桌面壳和本地存储方案。

以上采用顺序是结合 Graham 当前“已选 A、仍在调整视觉”的阶段作出的工程建议，不是三个项目官方为 Graham 背书，也不是已经完成迁移的声明。
