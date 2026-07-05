# 短视频素材工作台桌面版规格

## 1. 目标

把现有 Rust / Tauri 的 `nanocodex` 桌面端，裁剪成一套可直接部署到 Windows 的
“短视频素材提炼与复盘工作台”。

这套产品只做到两段：

1. `素材提炼`
2. `成片复盘 / 竞品分析`

明确不做：

- 文生视频
- 图生视频
- 付费视频生成 API 的直接接入

原因很简单：对最终客户来说，真正高价值的是“提炼、改写、复盘”，不是在桌面端里
烧高成本视频 API。

## 2. 现有前端可直接复用的部分

参考代码：

- `rust/gui/src/App.svelte`
- `rust/gui/src/app.css`
- `rust/gui/src-tauri/src/lib.rs`
- `rust/gui/src-tauri/src/bridge.rs`

当前现有桌面端已经有这些可复用能力：

- 窗口壳子、顶部栏、侧边栏、弹层模式
- Tauri `invoke()` + Rust backend command 桥
- 目录选择、文件选择、图片粘贴
- 状态栏 token / cost 展示逻辑
- 设置弹窗的加载 / 保存模式

这些东西不需要重做。

## 3. 现有前端需要删掉或隐藏的部分

当前桌面端是“通用 coding agent 桌面端”，部署给客户前要去掉明显开发者心智的内容。

建议隐藏或删除：

- 会话 Fork / Resume / Archive
- Git branch / diff / checkpoint
- Memory / Marketplace / MCP server 管理
- Slash commands
- 权限模式（plan/default/accept-edits/bypass）
- 文件树里的开发向操作
- Coding agent 对话主入口

保留但下沉到内部实现，不作为主界面暴露：

- `ncx-config` 的通用配置能力
- `bridge.rs` 事件流机制
- 文件读写 / 工作区切换

## 4. 产品定位

新的桌面端不是聊天工具，而是：

`短视频素材工作台`

目标用户是：

- 短视频运营
- 口播内容创作者
- 批量素材处理人员
- 需要把视频拆成可编辑内容资产的人

## 5. 总体界面结构

建议复用现有 Tauri 单窗口结构，但改成“工作台”信息架构。

```text
+---------------------------------------------------------------+
| 顶栏: 产品名 | 当前客户/工作区 | 本批次 Token | 预计成本 | 设置 |
+---------+-----------------------------------------------------+
| 左侧栏  | 1. 工作台首页                                        |
|         | 2. 新建任务                                          |
|         | 3. 批量队列                                          |
|         | 4. 素材包编辑                                        |
|         | 5. 成片复盘                                          |
|         | 6. 竞品分析                                          |
|         | 7. 配置中心                                          |
+---------+-----------------------------------------------------+
| 主内容区                                                     |
| - 导入表单 / 任务进度 / 素材包 / 报告预览 / 成本明细           |
+---------------------------------------------------------------+
| 底栏: 当前任务状态 | 调用次数 | DeepSeek Token | Qwen VL Token |
+---------------------------------------------------------------+
```

## 6. 页面设计

### 6.1 工作台首页

首页不是营销页，而是工作入口。

展示内容：

- 最近 10 个任务
- 当前批量队列
- 今天累计 token
- 今天累计成本
- 最近一次复盘结果
- “导入抖音链接”快捷入口
- “上传本地视频”快捷入口

### 6.2 新建任务页

#### A. 导入区

支持三种入口：

1. 抖音链接粘贴
2. 本地视频上传
3. 批量导入（多链接 / 多文件）

表单字段：

- `任务名称`
- `来源类型`：`douyin_url | local_video`
- `输入内容`
- `语言`：默认 `zh-CN`
- `任务模式`：`素材提炼 | 成片复盘 | 竞品分析`

#### B. 成本预估区

在点击“开始处理”前必须展示：

- 预计帧数
- 预计 ASR 时长
- 预计 VL 调用次数
- 预计 DeepSeek 调用次数
- 预计输入 token
- 预计输出 token
- 预计总成本
- 当前任务预算上限

按钮：

- `开始处理`
- `保存为草稿`

### 6.3 批量队列页

这是部署给客户之后最常用的页面之一。

每个任务行展示：

- 任务名
- 来源
- 当前阶段
- 进度
- 已用 token
- 已用成本
- 状态：`等待中 / 处理中 / 成功 / 失败 / 超预算中止`

操作：

- 重试
- 取消
- 打开素材包
- 导出结果

### 6.4 素材包编辑页

这是产品核心页。

采用标签页布局：

1. `原始素材`
2. `字幕 / OCR`
3. `口播转写`
4. `人物与画面`
5. `口播稿`
6. `标题与封面文案`
7. `视频提示词草稿`
8. `导出`

每个可编辑内容块都必须支持：

- 手动修改
- 恢复机器原稿
- 版本记录

最重要的要求：

- 原始事实和优化建议必须分开展示
- 用户修改后的稿子不能被下一次自动覆盖

### 6.5 成片复盘页

输入：

- 上传已生成成片
- 选择对应素材包

输出：

- 钩子强度评分
- 口播清晰度评分
- 字幕可读性评分
- 画面可信度评分
- 平台适配度评分
- 问题列表
- 下一版建议

### 6.6 竞品分析页

输入：

- 当前成片
- 1..N 个竞品链接或竞品视频

输出维度：

- 开头钩子
- 人设建立速度
- 字幕策略
- 画面节奏
- 专家感 / 权威感线索
- 信息密度

最终输出：

- 对比表
- 差距总结
- 下一轮行动建议

### 6.7 配置中心

这个页面需要从“通用 ncx 设置”改造成“部署给客户的业务设置”。

不应该直接暴露：

- sandbox_mode
- approval_policy
- reasoning_effort
- coding agent 模型切换器

应该改成以下结构：

#### A. 文本模型设置

用途：DeepSeek 文本理解、改写、复盘。

固定预设：

- Flash:
  - `model = deepseek-chat`
  - `base_url = https://api.deepseek.com/beta`
- Pro:
  - `model = deepseek-v4-pro`
  - `base_url = https://api.deepseek.com/beta`

用户可填：

- `api_key`

用户可切换：

- 默认档位：`Flash | Pro`
- 路由类型：`官方 Beta | 自定义兼容端点`

说明：

- 当路由类型是 `官方 Beta` 时，URL 只读
- 当路由类型是 `自定义兼容端点` 时，允许改 URL
- 切换 `Flash / Pro` 时，模型名必须自动联动切换

这就是你要的“预留 Flash / Pro 切换入口”。

#### B. 视觉模型设置

用途：Qwen VL 画面理解、字幕理解、成片视觉分析。

固定预设：

- `vl_model = qwen3-vl-plus`
- `vl_base_url = https://dashscope.aliyuncs.com/compatible-mode/v1`

用户可填：

- `vl_api_key`

默认不开放模型名编辑，避免部署后被随意改乱。

如需高级模式，可以增加一个隐藏入口：

- `允许高级覆盖`

只有开启后才允许改：

- `vl_model`
- `vl_base_url`

#### C. 成本与预算设置

这里是部署时必须有的，不然客户不知道“点一下烧了多少钱”。

字段：

- `单任务预算上限`
- `单批次预算上限`
- `VL 单帧预算系数`
- `文本输入 token 单价`
- `文本输出 token 单价`
- `VL 输入 token 单价`
- `VL 输出 token 单价`
- `超预算动作`
  - `仅警告`
  - `禁止启动`
  - `处理中止`

#### D. 运行限制

字段：

- `最大抽帧数`
- `最大竞品数量`
- `最长可转写时长`
- `是否自动 OCR`
- `是否自动 ASR`

## 7. 成本预控制设计

这是这套产品的关键点，不是可选项。

### 7.1 两层成本控制

#### 第一层：启动前预估

在点击“开始处理”前，先估算：

- 抽多少帧
- 每帧送多少 VL token
- OCR / ASR 预计文本量
- DeepSeek 预计输入 / 输出 token
- 预计总价

如果超预算：

- 直接阻止启动
或
- 要求用户确认

#### 第二层：运行中实计

每一步都记录真实 usage：

- `prompt_tokens`
- `completion_tokens`
- `prompt_cache_hit_tokens`
- `prompt_cache_miss_tokens`

并实时累加成：

- 当前任务 token
- 当前任务成本
- 今日累计成本

### 7.2 预估公式建议

预估不追求绝对精确，但必须“偏保守”，宁可高估一点。

建议：

- 中文文本 token 估算：`字符数 * 0.6`
- 英文文本 token 估算：`字符数 * 0.3`
- VL 成本不直接靠字符，而靠：
  - `抽帧数`
  - `每帧保守 token 预算`
  - `每帧平均输出 token`

也就是说，VL 预估模型应该是：

```text
estimated_vl_tokens =
  frame_count * estimated_prompt_tokens_per_frame
  + frame_count * estimated_output_tokens_per_frame
```

这里的两项系数不要让客户编辑，部署时固化为管理员配置。

### 7.3 DeepSeek 定价策略

本仓库已有 DeepSeek token 定价逻辑：

- `nanocodex/agent/pricing.py`

其中当前已覆盖：

- `deepseek-v4-pro`
- `deepseek-v4-flash`
- `deepseek-chat`
- `deepseek-reasoner`

桌面端应该复用这个逻辑，不要在前端单独再算一套。

### 7.4 Qwen VL 定价策略

当前仓库没有完整的 Qwen VL 定价表，因此部署版建议：

1. 内置一份“价格快照”
2. 在设置页展示“价格版本日期”
3. 允许管理员覆盖单价
4. UI 明确区分：
   - `预估成本`
   - `实计成本`

## 8. 模型配置的真正落地方式

### 8.1 不建议直接把客户暴露给 `~/.nanocodex/config.toml`

原因：

- 这是通用 agent 配置，不是业务产品配置
- 里面有很多部署给客户不该见到的字段
- 容易和开发环境混在一起

部署版建议使用独立配置文件，例如：

```text
%APPDATA%/MicrocodeXShortVideo/settings.json
```

或者：

```text
%APPDATA%/MicrocodeXShortVideo/settings.toml
```

然后在运行任务时，把业务配置映射到内部运行参数。

### 8.2 推荐的桌面配置结构

```json
{
  "text_provider": {
    "default_tier": "flash",
    "route_kind": "official",
    "api_key": "",
    "presets": {
      "flash": {
        "model": "deepseek-chat",
        "base_url": "https://api.deepseek.com/beta"
      },
      "pro": {
        "model": "deepseek-v4-pro",
        "base_url": "https://api.deepseek.com/beta"
      }
    }
  },
  "vision_provider": {
    "api_key": "",
    "model": "qwen3-vl-plus",
    "base_url": "https://dashscope.aliyuncs.com/compatible-mode/v1"
  },
  "budget": {
    "per_job_cny": 15.0,
    "per_batch_cny": 100.0,
    "block_when_over_budget": true
  },
  "limits": {
    "max_frames": 16,
    "max_competitors": 5,
    "max_transcription_minutes": 10
  }
}
```

## 9. 需要补的后端命令

现有 Tauri command 主要围绕会话和通用 agent。

部署版需要新增业务命令：

- `create_extract_job`
- `create_review_job`
- `create_competitor_job`
- `estimate_job_cost`
- `list_jobs`
- `get_job`
- `cancel_job`
- `retry_job`
- `get_material_pack`
- `update_material_pack`
- `export_job_bundle`
- `get_runtime_settings`
- `save_runtime_settings`

事件流建议新增业务事件：

- `job_created`
- `job_stage_changed`
- `job_usage_updated`
- `job_completed`
- `job_failed`

## 10. 对现有代码的具体改造建议

### 10.1 `rust/gui/src/App.svelte`

当前文件太像一个通用聊天壳子，后续应拆成组件：

- `Shell.svelte`
- `NavRail.svelte`
- `DashboardPage.svelte`
- `NewJobPage.svelte`
- `QueuePage.svelte`
- `MaterialPackPage.svelte`
- `ReviewPage.svelte`
- `CompetitorPage.svelte`
- `SettingsPage.svelte`
- `CostBadge.svelte`

### 10.2 `rust/gui/src/app.css`

当前样式偏“温暖编辑感”，适合通用桌面工具，但对运营工作台来说稍微太软。

建议保留结构系统，调整为：

- 更中性的浅灰背景
- 更密集的信息布局
- 更明显的状态色
- 更克制的卡片感

### 10.3 `rust/gui/src-tauri/src/lib.rs`

当前 `Settings` 结构还不够部署版使用。

至少需要补充：

- `fast_model`
- `vl_base_url`
- `vl_api_key_masked`
- `vl_has_api_key`
- `vl_model`
- `budget` 相关字段
- `limits` 相关字段

### 10.4 `rust/crates/ncx-config/src/writer.rs`

这一层已经有 `fast_model` 的 runtime 支持，但原先 GUI 写回链路没有把它完整当成主要字段。

部署版必须把它当成正式配置项对待，因为：

- `model` 可以作为 Pro
- `fast_model` 可以作为 Flash
- 桌面端可以默认走 Flash，需要时切到 Pro

### 10.5 `rust/gui/src-tauri/src/bridge.rs`

目前它围绕会话驱动。

部署版要补一个“任务驱动”分支：

- 队列线程
- 任务状态机
- 每步 usage 上报
- 失败可重试

## 11. Flash / Pro 切换的明确规则

为了避免后续实现时理解偏差，这里单独写死。

### 11.1 默认规则

- 默认文本档位：`Flash`
- 默认模型：`deepseek-chat`
- 默认 URL：`https://api.deepseek.com/beta`

### 11.2 切换到 Pro 时

自动改为：

- `model = deepseek-v4-pro`
- `base_url = https://api.deepseek.com/beta`

### 11.3 切换 URI 类型时

当用户从：

- `官方 Beta`

切换到：

- `自定义兼容端点`

才允许编辑 URL。

当切回：

- `官方 Beta`

URL 自动恢复为内置值。

### 11.4 UI 放置位置

必须同时提供两处：

1. `配置中心`里的默认档位设置
2. `新建任务页`里的本次任务覆盖设置

这样客户可以：

- 平时默认走 Flash 控成本
- 某个重点任务临时切到 Pro

## 12. Windows 部署要求

部署目标是给客户直接装机使用，不是开发者本地玩具。

因此需要：

- Tauri 打包为 `msi` 或 `nsis`
- 内置 `ffmpeg` / `ffprobe`
- Python 依赖如仍保留下载链路，则要么：
  - 打成独立 sidecar
  - 要么打成内置 runtime
- 所有子进程默认隐藏黑窗
- 所有日志 UTF-8 落盘
- 所有路径使用 Windows 绝对路径处理

特别注意：

- 若继续复用 Douyin downloader 的 Python 侧，Windows 下应统一注入
  `PYTHONUTF8=1`

## 13. MVP 落地顺序

第一阶段只做：

1. 新建任务页
2. 批量队列页
3. 素材包编辑页
4. 配置中心
5. 成本预估
6. DeepSeek Flash / Pro 切换
7. Qwen VL 固定配置

第二阶段再加：

1. 成片复盘
2. 竞品分析
3. 导出报告

## 14. 结论

这套桌面端不应该再被理解成“改一改 nanocodex GUI”，而应该被理解成：

`基于现有 Rust/Tauri 骨架，收缩成一个面向客户部署的短视频素材工作台`

核心设计原则只有四条：

1. UI 从“对话式 agent”改成“任务式工作台”
2. 模型固定预设，Key 由客户填写
3. 先做成本预控，再允许开始跑
4. 默认 Flash，重点任务可切 Pro
