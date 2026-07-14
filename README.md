# MicrocodeX Short Video Workbench

一个面向 Windows 的短视频素材提炼工作台。

它的目标不是直接替代文生视频 App，而是把短视频生产前半段里最费时间、最容易重复劳动的部分整理成一个可部署、可交付、可复用的桌面工具，包括：

- 抖音链接导入
- 本地视频导入
- 素材提炼与结构化整理
- 提示词生成与改写
- 按目标时长和幕数拆分长视频提示词，并锁定章节首尾帧连续性
- 成本与模型配置管理
- 竞品对比与复盘分析

这个项目与原始 `nanocodex` 仓库完全分离，作为一个独立的 Windows 桌面端项目维护。

## 适用场景

- 给客户做短视频素材提炼工作台
- 给运营同学做“导入素材 -> 提炼内容 -> 生成提示词”的半自动流程
- 做 Windows 本地部署交付
- 为后续接入 Agent、MCP、Skill、任务队列提供桌面壳和工程骨架

## 当前状态

当前仓库已经包含：

- 独立的 Tauri 桌面端工程
- 任务队列与工作台界面骨架
- DeepSeek Flash / Pro 切换入口
- Qwen VL / ASR 配置入口
- 成本预估与设置持久化
- Windows 安装版打包链路
- Windows 绿色便携版打包链路

当前仍处于持续接入阶段的能力包括：

- 抖音下载链路进一步实装
- OCR 实际能力接入
- ASR 实际能力接入
- VL 批量抽帧分析执行器
- 更完整的任务编排与物料落盘

### 长视频章节规划

左侧导航提供独立的“长视频章节”模块：

- 可直接带入素材提炼后的完整提示词，也可手工粘贴故事底稿
- 可输入自然语言创作要求，让模型理解后重新设计冲突、节奏、风格、人物表现和结尾
- 支持设置 30 秒到 30 分钟总时长、2 到 12 幕剧情
- 为每幕分配时长、口播字数和镜头数量预算
- 输出人物、服装、场景、视觉风格和镜头轴线连续性锚点
- 强制将上一幕尾帧作为下一幕首帧，修改尾帧时同步更新下一幕
- 有文本模型 Key 时使用 DeepSeek 结构化重写，没有 Key 时生成本地规划草稿
- 可保存为带时间戳的 JSON 和 Markdown 新版本，不覆盖旧稿

## 技术栈

- Rust `1.96.x`
- Tauri `2.8.x`
- Svelte `5`
- Vite `6`
- TypeScript `5`

## 下载方式

已发布的构建产物可在 GitHub Releases 获取：

- 安装版 EXE：适合常规交付与安装使用
- 绿色便携版 ZIP：适合免安装、整包拷走、U 盘分发

发布页：

- [Releases](https://github.com/dgy-github/microcodex-short-video-workbench/releases)

## 本地开发

```bash
npm install
npm run tauri dev
```

## Windows 打包

### 1. 安装版 EXE

如果你要构建带完整运行时的安装版，先准备离线运行时资源：

```powershell
.\scripts\prepare_windows_bundle.ps1
npm run tauri:installer
```

这条链路会把以下内容打进本地打包资源目录：

- Python
- FFmpeg
- Playwright
- Chromium
- 干净的 `douyin-downloader`

安装包输出目录通常在：

```text
src-tauri/target/x86_64-pc-windows-gnu/release/bundle/nsis/
```

### 2. 绿色便携版

如果你要构建免安装的绿色便携版：

```powershell
npm run tauri:portable
```

便携版会生成一个可直接拷走的目录和一个 zip 包，包含：

- 应用可执行文件
- 固定版 WebView2 Runtime
- Python / FFmpeg / Playwright / Chromium
- 内置 `douyin-downloader`
- 本地 `data/` 数据目录

## 面向最终使用者的说明

正常使用安装版或便携版时，最终使用者通常不需要额外安装：

- Node.js
- npm
- Rust
- Cargo

但仍需要准备以下业务运行条件：

- Windows 10 / 11 x64
- DeepSeek 文本 Key
- Qwen VL / ASR Key
- 至少一次抖音 Cookie 登录

如果使用轻量模式而不是完整打包模式，还需要自行准备：

- `ffmpeg`
- `douyin-downloader`
- 对应 Python / Playwright / Chromium 环境

## 开源仓库范围

这个 GitHub 仓库保持为“源码仓”，不会提交本地构建产物和大体积运行时载荷。

默认忽略的内容包括：

- `node_modules/`
- `dist/`
- `dist-portable/`
- `src-tauri/target/`
- `bundle-assets/windows-runtime/`

也就是说：

- 仓库里保留源码、脚本、文档、配置
- 安装包、便携包、离线运行时在本机构建生成

## 项目结构

```text
docs/
  project-space/
src/
src-tauri/
tests/
resources/
scripts/
bundle-assets/
```

## 相关文档

- [Windows 部署说明](docs/WINDOWS_DEPLOYMENT.md)
- [项目空间定义](docs/project-space/SPACE.md)
- [边界版本说明](docs/project-space/BOUNDARY_VERSIONS.md)
- [代码规范](docs/project-space/CODE_STYLE.md)
- [测试规范](docs/project-space/TESTING.md)
- [UI 规范](docs/project-space/UI_GUIDELINES.md)
- [交接文档](docs/project-space/HANDOFF.md)
- [短视频 Agent 设计文档](docs/short-video-agent-design.md)
- [桌面端规格说明](docs/short-video-desktop-spec.md)

## License

本项目使用 [MIT License](LICENSE)。
