<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { onDestroy, onMount, tick } from "svelte";

  type PageKey =
    | "dashboard"
    | "new-job"
    | "queue"
    | "material"
    | "review"
    | "competitor"
    | "settings";

  type TextTier = "flash" | "pro";
  type RouteKind = "official" | "custom";
  type JobMode = "extract" | "review" | "competitor";
  type SourceKind = "douyin_url" | "local_video";
  type JobStatus = "waiting" | "running" | "done" | "blocked";
  type NoticeTone = "good" | "warn";

  type TextPreset = {
    model: string;
    baseUrl: string;
  };

  type RuntimeSettingsView = {
    schemaVersion: number;
    updatedAtMs: number;
    settingsPath: string;
    textProvider: {
      defaultTier: TextTier;
      routeKind: RouteKind;
      hasApiKey: boolean;
      apiKeyMasked: string;
      customBaseUrl: string;
      presets: {
        flash: TextPreset;
        pro: TextPreset;
      };
    };
    visionProvider: {
      hasApiKey: boolean;
      apiKeyMasked: string;
      model: string;
      baseUrl: string;
      allowAdvancedOverride: boolean;
    };
    budget: {
      perJobCny: number;
      perBatchCny: number;
      blockWhenOverBudget: boolean;
      flashInputPerMTokensCny: number;
      flashOutputPerMTokensCny: number;
      proInputPerMTokensCny: number;
      proOutputPerMTokensCny: number;
      vlInputPerFrameCny: number;
      vlOutputPerFrameCny: number;
    };
    limits: {
      maxFrames: number;
      maxCompetitors: number;
      maxTranscriptionMinutes: number;
      autoOcr: boolean;
      autoAsr: boolean;
    };
  };

  type RuntimeSettingsUpdate = {
    textProvider: {
      defaultTier: TextTier;
      routeKind: RouteKind;
      textApiKey: string;
      customBaseUrl: string;
      presets: {
        flash: TextPreset;
        pro: TextPreset;
      };
    };
    visionProvider: {
      visionApiKey: string;
      model: string;
      baseUrl: string;
      allowAdvancedOverride: boolean;
    };
    budget: RuntimeSettingsView["budget"];
    limits: RuntimeSettingsView["limits"];
  };

  type DashboardSnapshot = {
    pendingJobs: number;
    runningJobs: number;
    finishedJobsToday: number;
    estimatedSpendTodayCny: number;
  };

  type EstimateJobRequest = {
    mode: JobMode;
    sourceKind: SourceKind;
    durationMinutes: number;
    frameCount: number;
    competitorCount: number;
    textTier: TextTier;
  };

  type EstimateJobResult = {
    estimatedPromptTokens: number;
    estimatedCompletionTokens: number;
    estimatedVlFrames: number;
    estimatedVlCalls: number;
    estimatedTextCalls: number;
    estimatedCostCny: number;
    exceedsJobBudget: boolean;
    effectiveTextModel: string;
    effectiveTextBaseUrl: string;
    effectiveVisionModel: string;
    notes: string[];
  };

  type CreateJobRequest = {
    name: string;
    mode: JobMode;
    sourceKind: SourceKind;
    sourceValue: string;
    durationMinutes: number;
    frameCount: number;
    competitorCount: number;
    textTier: TextTier;
  };

  type JobView = {
    id: string;
    name: string;
    mode: JobMode;
    sourceKind: SourceKind;
    sourceValue: string;
    status: JobStatus;
    stageKey: string;
    progress: number;
    textTier: TextTier;
    estimatedPromptTokens: number;
    estimatedCompletionTokens: number;
    estimatedTotalTokens: number;
    actualPromptTokens: number;
    actualCompletionTokens: number;
    actualTotalTokens: number;
    estimatedCostCny: number;
    actualCostCny: number;
    effectiveTextModel: string;
    effectiveTextBaseUrl: string;
    effectiveVisionModel: string;
    createdAtMs: number;
    updatedAtMs: number;
    startedAtMs: number | null;
    finishedAtMs: number | null;
    artifactDir: string;
    materialPackPath: string | null;
    competitorReportPath: string | null;
    stageLogPath: string;
    notes: string[];
    error: string | null;
  };

  type MaterialTabKey = "raw" | "ocr" | "vision" | "script" | "copy" | "prompt";
  type PromptPlatformKey = "generic" | "jimeng" | "keling";
  type PromptVersionKey = "full" | "balanced" | "compact";
  type PromptFocusKey = "balanced" | "lip_sync" | "visual";
  type PromptTweakKey =
    | "vivid_background"
    | "stronger_authority"
    | "clearer_hook"
    | "safer_subtitles"
    | "softer_lighting";

  type LocalizedPromptMaterial = {
    topic: string;
    audience: string;
    persona: string;
    tone: string;
    coreMessage: string[];
    hook: string;
    scriptBody: string[];
    ending: string;
    titleCandidates: string[];
    coverCandidates: string[];
    promoCopy: string[];
    visualBrief: string;
    spokenBrief: string;
    reusablePrompt: string;
  };

  type MaterialPackView = {
    job_id: string;
    topic: string;
    audience: string;
    speaker_profile: {
      persona: string;
      tone: string;
    };
    core_message: string[];
    editable_script: {
      hook: string;
      body: string[];
      ending: string;
    };
    title_candidates: string[];
    cover_copy_candidates: string[];
    promo_copy: string[];
    video_prompt_draft: {
      visual_brief: string;
      spoken_brief: string;
      reusable_prompt: string;
    };
    evidence_refs: {
      vision_summary: string;
      transcript_structured: string;
    };
  };

  type ManualMaterialDraftFile = {
    sourceJobId: string;
    updatedAtMs: number;
    platformLabel: string;
    versionLabel: string;
    focusLabel: string;
    tweakLabels: string[];
    topic: string;
    audience: string;
    persona: string;
    tone: string;
    hook: string;
    body: string[];
    ending: string;
    visualBrief: string;
    spokenBrief: string;
    reusablePrompt: string;
    fullPrompt: string;
    titleCandidates: string[];
    coverCopyCandidates: string[];
    promoCopy: string[];
  };

  type ManualMaterialDraftView = {
    topic: string;
    audience: string;
    persona: string;
    tone: string;
    hook: string;
    bodyText: string;
    ending: string;
    visualBrief: string;
    spokenBrief: string;
    reusablePrompt: string;
    fullPrompt: string;
    titleCandidatesText: string;
    coverCopyCandidatesText: string;
    promoCopyText: string;
  };

  type ManualMaterialDraftSaveResult = {
    draftPath: string;
    promptPath: string;
    markdownPath: string;
    updatedAtMs: number;
  };

  type CompetitorSourceSpec = {
    kind: SourceKind;
    value: string;
    label?: string;
  };

  type CompetitorSourceBundle = {
    primary: CompetitorSourceSpec;
    competitors: CompetitorSourceSpec[];
  };

  type CompetitorReportMetricView = {
    key: CompetitorMetricKey;
    label: string;
    summary: string;
    current_score: number;
    competitor_score: number;
    competitor_best_score: number;
    current_note: string;
    benchmark_note: string;
    action: string;
    rewrite_hint: string;
    prompt_tweaks: PromptTweakKey[];
    prompt_focus: PromptFocusKey;
    evidence: string[];
  };

  type CompetitorReportView = {
    job_id: string;
    current_label: string;
    current_topic: string;
    competitor_count: number;
    competitor_labels: string[];
    top_findings: string[];
    recommended_focus: PromptFocusKey;
    recommended_tweaks: PromptTweakKey[];
    metrics: CompetitorReportMetricView[];
    generated_by_model?: string;
    llm_usage?: {
      prompt_tokens: number;
      completion_tokens: number;
      total_tokens: number;
    };
    generated_at_ms: number;
  };

  type MaterialPromptRewriteResult = {
    prompt: string;
    generatedByModel: string;
    llmUsage: {
      prompt_tokens: number;
      completion_tokens: number;
      total_tokens: number;
    };
    costCny: number;
  };

  type EnvironmentCheckItem = {
    key: string;
    label: string;
    status: "ok" | "warn" | "missing";
    detail: string;
    actionHint: string;
  };

  type EnvironmentHealthReport = {
    checkedAtMs: number;
    overallStatus: "ok" | "warn" | "missing";
    okCount: number;
    warningCount: number;
    missingCount: number;
    helperScriptPath: string;
    items: EnvironmentCheckItem[];
  };

  type CompetitorMetricKey =
    | "hook"
    | "authority"
    | "subtitles"
    | "rhythm"
    | "background"
    | "lighting"
    | "framing"
    | "persona";

  type CompetitorMetricPreset = {
    key: CompetitorMetricKey;
    label: string;
    summary: string;
    benchmarkScore: number;
    benchmarkNote: string;
    action: string;
    rewriteHint: (pack: MaterialPackView | null) => string;
    promptTweaks: PromptTweakKey[];
    promptFocus: PromptFocusKey;
    currentScore: (pack: MaterialPackView | null) => number;
    currentNote: (pack: MaterialPackView | null) => string;
  };

  type CompetitorInsight = {
    key: CompetitorMetricKey;
    label: string;
    summary: string;
    enabled: boolean;
    weight: number;
    currentScore: number;
    benchmarkScore: number;
    gap: number;
    weightedGap: number;
    currentNote: string;
    benchmarkNote: string;
    action: string;
    rewriteHint: string;
    promptTweaks: PromptTweakKey[];
    promptFocus: PromptFocusKey;
  };

  const pages: { key: PageKey; title: string; kicker: string; icon: string }[] = [
    { key: "dashboard", title: "工作台首页", kicker: "Dashboard", icon: "D" },
    { key: "new-job", title: "新建任务", kicker: "Create", icon: "+" },
    { key: "queue", title: "批量队列", kicker: "Queue", icon: "Q" },
    { key: "material", title: "素材包编辑", kicker: "Assets", icon: "M" },
    { key: "review", title: "成片复盘", kicker: "Review", icon: "R" },
    { key: "competitor", title: "竞品分析", kicker: "Compare", icon: "C" },
    { key: "settings", title: "配置中心", kicker: "Settings", icon: "S" },
  ];

  const materialTabs = [
    { key: "raw", label: "原始素材" },
    { key: "ocr", label: "字幕 / OCR" },
    { key: "vision", label: "画面理解" },
    { key: "script", label: "口播稿" },
    { key: "copy", label: "标题与封面" },
    { key: "prompt", label: "提示词草稿" },
  ];

  const promptPlatformOptions: {
    key: PromptPlatformKey;
    label: string;
    summary: string;
    instruction: string;
  }[] = [
    {
      key: "generic",
      label: "通用版",
      summary: "适合大多数视频生成器",
      instruction:
        "输出为通用中文视频生成提示词，结构清晰，方便复制到多数文生视频工具中继续调整。",
    },
    {
      key: "jimeng",
      label: "即梦版",
      summary: "更强调主体、场景、风格和画面描述",
      instruction:
        "适配即梦类生成器，优先写清人物、背景、镜头、光线、字幕占位和整体氛围，避免只给脚本不给画面。",
    },
    {
      key: "keling",
      label: "可灵版",
      summary: "更强调镜头连续性和动态真实感",
      instruction:
        "适配可灵类生成器，优先写清动作连贯性、镜头稳定性、人物口型同步和环境细节，让画面更像真实拍摄。",
    },
  ];

  const promptVersionOptions: {
    key: PromptVersionKey;
    label: string;
    summary: string;
  }[] = [
    { key: "full", label: "完整", summary: "信息最全，适合首轮生成" },
    { key: "balanced", label: "平衡", summary: "保留关键约束，长度更克制" },
    { key: "compact", label: "精简", summary: "适合手动二次改写或快速试图" },
  ];

  const promptFocusOptions: {
    key: PromptFocusKey;
    label: string;
    summary: string;
    instruction: string;
  }[] = [
    {
      key: "balanced",
      label: "均衡",
      summary: "画面、口播、字幕都兼顾",
      instruction: "整体以均衡出片为目标，不要让某一项过度抢戏。",
    },
    {
      key: "lip_sync",
      label: "口型同步优先",
      summary: "更适合真人口播短视频",
      instruction: "强调正面口播、语速稳定、嘴型清晰、动作克制，避免大幅度头部转动。",
    },
    {
      key: "visual",
      label: "画面质感优先",
      summary: "更强调空间层次和镜头氛围",
      instruction: "强调背景层次、灯光质感、景深和环境细节，让画面更高级但不要丢掉主体清晰度。",
    },
  ];

  const promptTweakOptions: {
    key: PromptTweakKey;
    label: string;
    summary: string;
    promptLine: string;
    guide: string;
  }[] = [
    {
      key: "vivid_background",
      label: "背景更生动",
      summary: "增加空间层次和环境细节",
      promptLine: "背景更有层次感，窗帘、书架、桌面或环境道具细节更丰富，但始终不要抢过主体人物。",
      guide: "想让背景更生动，就补“层次、道具、景深、环境细节”，但要同时加一句“不要压过人物主体”。",
    },
    {
      key: "stronger_authority",
      label: "权威感更强",
      summary: "加强专业、可信的专家形象",
      promptLine: "人物呈现更专业可信，坐姿稳定，镜头平视，服装规整，神态沉稳，有明确专家表达气质。",
      guide: "想更像专家，就补“平视镜头、坐姿稳定、服装规整、神态沉稳、可信表达”这类约束。",
    },
    {
      key: "clearer_hook",
      label: "前3秒更抓人",
      summary: "让开场冲突和钩子更明确",
      promptLine: "前 3 秒必须明确抛出反常识问题或冲突句，首屏字幕更直接，信息更快进入主题。",
      guide: "想提升开场抓力，就直接要求“前 3 秒抛问题”“首屏字幕更直接”“开头先冲突再解释”。",
    },
    {
      key: "safer_subtitles",
      label: "字幕更稳妥",
      summary: "更适合口播和后续手动改字",
      promptLine: "字幕与信息条更克制，每屏字数更少，不遮挡人物脸部，重点词适度高亮，不要满屏堆字。",
      guide: "想让字幕更稳，就限制“每屏字数、遮脸风险、重点词数量”，而不是只写“加字幕”。",
    },
    {
      key: "softer_lighting",
      label: "光线更高级",
      summary: "让画面更柔和、通透、耐看",
      promptLine: "布光更柔和通透，主光干净，辅光轻微，肤色自然，整体暖中性色调，不要过曝或发灰。",
      guide: "想让质感更高级，优先补“主光/辅光、肤色、暖中性色调、不过曝不过灰”这些摄影级描述。",
    },
  ];

  const competitorMetricOptions: CompetitorMetricPreset[] = [
    {
      key: "hook",
      label: "开头钩子",
      summary: "前 3 秒的问题感、反常识感和主题进入速度",
      benchmarkScore: 9.1,
      benchmarkNote: "首句先抛冲突或反常识问题，2 秒内把主题冲突说透。",
      action: "把解释型开头改成先冲突、后解释。",
      rewriteHint: (pack) => {
        const candidate =
          cleanText(pack?.editable_script.hook) ||
          cleanText(pack?.title_candidates?.[0]) ||
          "为什么少吃还是会胖？";
        return `首句直接抛出“${candidate}”这类反常识问题，再用下一句补解释，不要先铺背景。`;
      },
      promptTweaks: ["clearer_hook"],
      promptFocus: "balanced",
      currentScore: (pack) => {
        const hook = compactText(pack?.editable_script.hook);
        if (!hook) return 5.6;
        let score = 6.1;
        if (containsAnyText(hook, ["为什么", "怎么", "反而", "却", "竟然", "别再", "少吃"])) score += 1.1;
        if (hook.includes("?") || hook.includes("？")) score += 0.6;
        if (hook.length <= 18) score += 0.5;
        if (containsAnyText(hook, ["很多人", "今天", "我们"])) score -= 0.3;
        return clampScore(score);
      },
      currentNote: (pack) => `当前开场：${previewText(pack?.editable_script.hook, "还没有明确钩子", 24)}`,
    },
    {
      key: "authority",
      label: "权威感线索",
      summary: "专家身份、可信线索和首屏说服力",
      benchmarkScore: 8.9,
      benchmarkNote: "首屏就给出专家身份、专业场景或可信赖的专业线索。",
      action: "把身份标签、专业背景和可信场景前置到首屏。",
      rewriteHint: (pack) => {
        const persona = previewText(pack?.speaker_profile.persona, "专业健康讲师", 22);
        return `在首屏信息条提前补出“${persona} / 专业研究背景 / 服务对象”这类可信线索，让用户 1 秒内知道你是谁。`;
      },
      promptTweaks: ["stronger_authority"],
      promptFocus: "balanced",
      currentScore: (pack) => {
        const context = packSearchText(pack);
        let score = 6.0;
        if (containsAnyText(context, ["专家", "教授", "博士", "医生", "学者", "剑桥", "复旦", "科普", "讲师"])) score += 1.5;
        if (containsAnyText(context, ["眼镜", "领带", "书房", "书架", "专业", "身份"])) score += 0.7;
        if (cleanText(pack?.speaker_profile.persona).length > 8) score += 0.4;
        return clampScore(score);
      },
      currentNote: (pack) =>
        `当前人设：${previewText(
          `${cleanText(pack?.speaker_profile.persona)} ${cleanText(pack?.speaker_profile.tone)}`,
          "还没有明确的人设标签",
          26,
        )}`,
    },
    {
      key: "subtitles",
      label: "字幕策略",
      summary: "单屏字数、关键词高亮和信息条安全区",
      benchmarkScore: 8.7,
      benchmarkNote: "字幕更短，关键词更亮，且始终不压住人物脸部。",
      action: "把字幕压成短句，保留 1 个关键词做高亮。",
      rewriteHint: (pack) => {
        const keyword =
          previewText(pack?.cover_copy_candidates?.[0], "", 14) ||
          previewText(pack?.title_candidates?.[0], "", 14) ||
          "压力不降，少吃也胖";
        return `把字幕压成每屏 10-14 个字的短句，首屏优先亮出“${keyword}”这类关键词，别满屏堆字。`;
      },
      promptTweaks: ["safer_subtitles"],
      promptFocus: "balanced",
      currentScore: (pack) => {
        const candidates = cleanLines([
          ...(pack?.cover_copy_candidates ?? []),
          ...(pack?.title_candidates ?? []),
        ]);
        if (!candidates.length) return 5.9;
        const shortest = candidates.reduce((min, value) => Math.min(min, compactText(value).length), 999);
        let score = 6.0;
        if (shortest <= 12) score += 1.0;
        else if (shortest <= 16) score += 0.6;
        else if (shortest >= 22) score -= 0.4;
        if (containsAnyText(candidates.join(" "), ["胖", "压力", "代谢", "胰岛素", "长寿"])) score += 0.5;
        score += 0.4;
        return clampScore(score);
      },
      currentNote: (pack) =>
        `当前短句：${shortListPreview(
          [...(pack?.cover_copy_candidates ?? []), ...(pack?.title_candidates ?? [])],
          "还没有提炼出短字幕",
          2,
        )}`,
    },
    {
      key: "rhythm",
      label: "口播节奏",
      summary: "一句一锤点的讲解节奏和停顿层次",
      benchmarkScore: 8.5,
      benchmarkNote: "一段只讲一个点，节奏稳定，15-25 秒内完成一轮解释。",
      action: "按“结论 -> 原因 -> 机制 -> 落点”四拍重写。",
      rewriteHint: (pack) => {
        const hook = previewText(pack?.editable_script.hook, "少吃不一定会瘦", 18);
        return `围绕“${hook}”按“结论 -> 原因 -> 机制 -> 落点”重排结构，每句只保留一个信息点，给听众稳定停顿。`;
      },
      promptTweaks: ["clearer_hook"],
      promptFocus: "lip_sync",
      currentScore: (pack) => {
        const bodyCount = cleanLines(pack?.editable_script.body ?? []).length;
        const coreCount = cleanLines(pack?.core_message ?? []).length;
        let score = 6.1;
        if (bodyCount >= 2 && bodyCount <= 4) score += 1.1;
        else if (bodyCount === 1) score += 0.3;
        else if (bodyCount >= 5) score -= 0.5;
        if (coreCount >= 2 && coreCount <= 4) score += 0.5;
        if (containsAnyText(packSearchText(pack), ["节奏", "停顿", "口播", "自然", "连贯"])) score += 0.3;
        return clampScore(score);
      },
      currentNote: (pack) => {
        const bodyCount = cleanLines(pack?.editable_script.body ?? []).length;
        const coreCount = cleanLines(pack?.core_message ?? []).length;
        return bodyCount
          ? `当前结构：正文 ${bodyCount} 段，核心 ${coreCount || 0} 点`
          : "当前结构：还没有稳定的段落节奏";
      },
    },
    {
      key: "background",
      label: "背景层次",
      summary: "空间层次、道具细节和环境可信度",
      benchmarkScore: 8.4,
      benchmarkNote: "背景有层次和景深，但不会抢人物主体。",
      action: "补足书房感、道具层次和景深信息。",
      rewriteHint: (pack) => {
        const visual = previewText(pack?.video_prompt_draft.visual_brief, "书房背景", 18);
        return `保留“${visual}”这类空间线索，再补进书架、窗帘、桌面道具和景深层次，让背景更生动但不要压过人物。`;
      },
      promptTweaks: ["vivid_background"],
      promptFocus: "visual",
      currentScore: (pack) => {
        const visual = compactText(
          [pack?.video_prompt_draft.visual_brief, pack?.video_prompt_draft.reusable_prompt].join(" "),
        );
        if (!visual) return 5.8;
        let score = 5.9;
        if (containsAnyText(visual, ["书架", "窗帘", "木质", "书房", "景深", "背景", "道具"])) score += 1.3;
        if (containsAnyText(visual, ["层次", "虚化", "环境", "高级", "安静"])) score += 0.8;
        return clampScore(score);
      },
      currentNote: (pack) => `当前背景：${previewText(pack?.video_prompt_draft.visual_brief, "背景信息还偏少", 28)}`,
    },
    {
      key: "lighting",
      label: "布光质感",
      summary: "主光、补光、肤色和整体通透感",
      benchmarkScore: 8.8,
      benchmarkNote: "主光干净，补光克制，肤色自然，整体更通透耐看。",
      action: "明确主光、补光、肤色和整体色调。",
      rewriteHint: (pack) => {
        const tone = previewText(pack?.speaker_profile.tone, "专业可信", 16);
        return `保持“${tone}”的人物状态，同时明确“柔和主光 + 轻微侧补光 + 自然肤色 + 暖中性色调”，让画面更高级。`;
      },
      promptTweaks: ["softer_lighting"],
      promptFocus: "visual",
      currentScore: (pack) => {
        const visual = compactText(
          [pack?.video_prompt_draft.visual_brief, pack?.video_prompt_draft.reusable_prompt].join(" "),
        );
        if (!visual) return 6.0;
        let score = 6.2;
        if (containsAnyText(visual, ["柔和", "主光", "补光", "肤色", "暖中性", "通透"])) score += 1.2;
        if (containsAnyText(visual, ["布光", "高级", "自然", "不过曝", "不过灰"])) score += 0.7;
        return clampScore(score);
      },
      currentNote: (pack) => `当前布光：${previewText(pack?.video_prompt_draft.visual_brief, "布光描述还不够明确", 28)}`,
    },
    {
      key: "framing",
      label: "镜头构图",
      summary: "竖屏构图、机位稳定和字幕安全区",
      benchmarkScore: 8.6,
      benchmarkNote: "9:16 中近景稳定构图，人物清晰，字幕和信息条都有安全区。",
      action: "锁定竖屏中近景，给字幕和信息条预留安全区。",
      rewriteHint: (pack) => {
        const frame = previewText(pack?.video_prompt_draft.reusable_prompt, "竖屏中近景", 18);
        return `把镜头约束成“${frame}”这一类稳定构图，减少漂移和夸张运动，同时给底部字幕预留安全区。`;
      },
      promptTweaks: ["safer_subtitles"],
      promptFocus: "visual",
      currentScore: (pack) => {
        const visual = compactText(
          [pack?.video_prompt_draft.visual_brief, pack?.video_prompt_draft.reusable_prompt].join(" "),
        );
        if (!visual) return 6.1;
        let score = 6.3;
        if (containsAnyText(visual, ["9:16", "竖屏", "中近景", "胸像", "平视", "稳定", "机位"])) score += 1.4;
        if (containsAnyText(visual, ["构图", "安全区", "字幕", "口型"])) score += 0.5;
        return clampScore(score);
      },
      currentNote: (pack) => `当前构图：${previewText(pack?.video_prompt_draft.reusable_prompt, "机位约束还不够明确", 28)}`,
    },
    {
      key: "persona",
      label: "人物表达",
      summary: "口播状态、亲和力和可信感",
      benchmarkScore: 8.7,
      benchmarkNote: "像稳定输出的知识博主，专业、亲和、沉稳，不像念稿。",
      action: "统一人物状态，减少念稿感，增强陪伴感。",
      rewriteHint: (pack) => {
        const persona =
          previewText(pack?.speaker_profile.persona, "", 18) ||
          previewText(pack?.speaker_profile.tone, "", 18) ||
          "专业健康讲师";
        return `把人物表达统一成“${persona}”这一路数，语速更稳，停顿更自然，手势克制，减少照本宣科的感觉。`;
      },
      promptTweaks: ["stronger_authority"],
      promptFocus: "lip_sync",
      currentScore: (pack) => {
        const context = compactText(
          [pack?.speaker_profile.persona, pack?.speaker_profile.tone, pack?.video_prompt_draft.spoken_brief].join(
            " ",
          ),
        );
        if (!context) return 6.2;
        let score = 6.3;
        if (containsAnyText(context, ["专业", "亲和", "沉稳", "可信", "自然", "克制", "讲解"])) score += 1.2;
        if (containsAnyText(context, ["口播", "表达", "稳定", "陪伴"])) score += 0.6;
        if (cleanText(pack?.speaker_profile.persona).length > 0) score += 0.4;
        return clampScore(score);
      },
      currentNote: (pack) =>
        `当前状态：${previewText(
          `${cleanText(pack?.speaker_profile.persona)} ${cleanText(pack?.speaker_profile.tone)}`,
          "人物表达还偏泛",
          26,
        )}`,
    },
  ];

  const stageLabels: Record<string, string> = {
    queued: "排队中",
    cancelled: "已取消",
    completed: "已完成",
    extract_ingest: "导入视频",
    extract_preprocess: "预处理",
    extract_ocr: "OCR 提取",
    extract_asr: "音频转写",
    extract_vision: "VL 识别",
    extract_text: "文本整理",
    extract_material_pack: "生成素材包",
    review_preprocess: "准备复盘素材",
    review_transcript: "转写成片",
    review_vision: "分析画面",
    review_text: "总结问题",
    review_report: "生成复盘结论",
    competitor_ingest: "导入竞品",
    competitor_preprocess: "整理样本",
    competitor_vision: "比对画面",
    competitor_text: "比对话术",
    competitor_report: "生成对比报告",
  };

  let currentPage: PageKey = "dashboard";
  let dashboard: DashboardSnapshot = {
    pendingJobs: 0,
    runningJobs: 0,
    finishedJobsToday: 0,
    estimatedSpendTodayCny: 0,
  };

  let settings: RuntimeSettingsView | null = null;
  let settingsDraft: RuntimeSettingsUpdate | null = null;
  let environmentReport: EnvironmentHealthReport | null = null;
  let jobs: JobView[] = [];

  let draftJob = {
    name: "健康科普口播拆解",
    mode: "extract" as JobMode,
    sourceKind: "douyin_url" as SourceKind,
    sourceValue: "",
    durationMinutes: 3,
    frameCount: 12,
    competitorCount: 3,
    tierOverride: "flash" as TextTier,
  };
  let draftCompetitorSourcesText = "";

  let estimate: EstimateJobResult | null = null;
  let lastEstimatedSignature = "";
  let estimateStale = false;
  let materialTab: MaterialTabKey = "script";

  let pendingTextApiKey = "";
  let pendingVisionApiKey = "";

  let loadingBootstrap = true;
  let loadingEnvironmentReport = false;
  let estimating = false;
  let creatingJob = false;
  let savingSettings = false;
  let refreshingQueue = false;
  let activeJobActionId = "";

  let newJobMessage = "";
  let newJobTone: NoticeTone = "good";
  let queueMessage = "";
  let queueTone: NoticeTone = "good";
  let settingsMessage = "";
  let settingsTone: NoticeTone = "good";
  let selectedLogJob: JobView | null = null;
  let selectedStageLog = "";
  let loadingStageLog = false;
  let refreshingStageLog = false;
  let selectedStageLogSignature = "";
  let stageLogViewport: HTMLDivElement | undefined;
  let stageLogStickToBottom = true;
  let materialPack: MaterialPackView | null = null;
  let materialPromptText = "";
  let materialMessage = "";
  let materialTone: NoticeTone = "good";
  let materialPromptRegenerating = false;
  let materialPromptGeneratedByModel = "";
  let materialPromptUsage:
    | {
        promptTokens: number;
        completionTokens: number;
        totalTokens: number;
        costCny: number;
      }
    | null = null;
  let loadingMaterialPack = false;
  let loadedMaterialJobId = "";
  let selectedMaterialJobId = "";
  let lastMaterialPromptSignature = "";
  let materialPromptRewriteSeq = 0;
  let materialPromptRewriteTimer: ReturnType<typeof setTimeout> | null = null;
  let promptPlatform: PromptPlatformKey = "generic";
  let promptVersion: PromptVersionKey = "full";
  let promptFocus: PromptFocusKey = "balanced";
  let activePromptTweaks: PromptTweakKey[] = [];
  let manualMaterialDraft: ManualMaterialDraftView | null = null;
  let manualMaterialSaving = false;
  let manualMaterialAutoSyncPrompt = true;
  let manualMaterialLastSaveResult: ManualMaterialDraftSaveResult | null = null;

  let recentJobs: JobView[] = [];
  let latestFinishedJob: JobView | undefined;
  let latestActiveJob: JobView | undefined;
  let materialJobs: JobView[] = [];
  let selectedMaterialJob: JobView | null = null;
  let competitorJobs: JobView[] = [];
  let competitorSelectableJobs: JobView[] = [];
  let competitorPack: MaterialPackView | null = null;
  let competitorReport: CompetitorReportView | null = null;
  let competitorMessage = "";
  let competitorTone: NoticeTone = "good";
  let loadingCompetitorPack = false;
  let loadedCompetitorJobId = "";
  let selectedCompetitorJobId = "";
  let selectedCompetitorJob: JobView | null = null;
  let activeCompetitorMetrics: CompetitorMetricKey[] = [
    "hook",
    "authority",
    "subtitles",
    "rhythm",
    "background",
    "lighting",
  ];
  let competitorMetricWeights: Record<CompetitorMetricKey, number> = {
    hook: 5,
    authority: 4,
    subtitles: 4,
    rhythm: 3,
    background: 2,
    lighting: 3,
    framing: 3,
    persona: 3,
  };
  let competitorInsights: CompetitorInsight[] = [];
  let selectedCompetitorInsightsList: CompetitorInsight[] = [];
  let competitorCurrentComposite = 0;
  let competitorBenchmarkComposite = 0;
  let competitorRecommendedFocusKey: PromptFocusKey = "balanced";
  let competitorRecommendedTweakKeys: PromptTweakKey[] = [];
  let competitorRecommendedTweakOptionsList: (typeof promptTweakOptions)[number][] = [];

  function cloneSettingsView(view: RuntimeSettingsView): RuntimeSettingsUpdate {
    return {
      textProvider: {
        defaultTier: view.textProvider.defaultTier,
        routeKind: view.textProvider.routeKind,
        textApiKey: "",
        customBaseUrl: view.textProvider.customBaseUrl,
        presets: {
          flash: { ...view.textProvider.presets.flash },
          pro: { ...view.textProvider.presets.pro },
        },
      },
      visionProvider: {
        visionApiKey: "",
        model: view.visionProvider.model,
        baseUrl: view.visionProvider.baseUrl,
        allowAdvancedOverride: view.visionProvider.allowAdvancedOverride,
      },
      budget: { ...view.budget },
      limits: { ...view.limits },
    };
  }

  function stringifyError(error: unknown): string {
    if (typeof error === "string") return error;
    if (error instanceof Error) return error.message;
    try {
      return JSON.stringify(error);
    } catch {
      return "未知错误";
    }
  }

  function formatCurrency(value: number): string {
    const numeric = Number(value);
    if (!Number.isFinite(numeric) || numeric === 0) return "¥0.00";
    const abs = Math.abs(numeric);
    if (abs >= 1) return `¥${numeric.toFixed(2)}`;
    if (abs >= 0.1) return `¥${numeric.toFixed(3)}`;
    return `¥${numeric.toFixed(4)}`;
  }

  function formatTokenCount(value: number): string {
    if (value >= 10000) return `${(value / 1000).toFixed(0)}k`;
    if (value >= 1000) return `${(value / 1000).toFixed(1)}k`;
    return value.toLocaleString("zh-CN");
  }

  function formatDateTime(timestamp: number | null | undefined): string {
    if (!timestamp) return "未开始";
    return new Date(timestamp).toLocaleString("zh-CN", {
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    });
  }

  function clearMaterialPromptRewriteTimer() {
    if (materialPromptRewriteTimer) {
      clearTimeout(materialPromptRewriteTimer);
      materialPromptRewriteTimer = null;
    }
  }

  onDestroy(() => {
    clearMaterialPromptRewriteTimer();
  });

  function jobModeLabel(mode: JobMode): string {
    if (mode === "review") return "成片复盘";
    if (mode === "competitor") return "竞品分析";
    return "素材提炼";
  }

  function sourceKindLabel(kind: SourceKind): string {
    return kind === "local_video" ? "本地视频" : "抖音链接";
  }

  function statusLabel(status: JobStatus): string {
    if (status === "running") return "处理中";
    if (status === "done") return "成功";
    if (status === "blocked") return "已中止";
    return "等待中";
  }

  function stageLabel(stageKey: string): string {
    return stageLabels[stageKey] ?? stageKey.replaceAll("_", " / ");
  }

  function cleanText(value: string | null | undefined): string {
    return (value ?? "").trim();
  }

  function cleanLines(values: Array<string | null | undefined>): string[] {
    return values.map(cleanText).filter((value) => value.length > 0);
  }

  function bulletedLines(values: string[]): string {
    return values.map((value) => `- ${value}`).join("\n");
  }

  function numberedLines(values: string[]): string {
    return values.map((value, index) => `${index + 1}. ${value}`).join("\n");
  }

  function round1(value: number): number {
    return Math.round(value * 10) / 10;
  }

  function clampScore(value: number): number {
    return Math.max(0, Math.min(10, round1(value)));
  }

  function containsAnyText(source: string, keywords: string[]): boolean {
    return keywords.some((keyword) => source.includes(keyword));
  }

  function compactText(value: string | null | undefined): string {
    return cleanText(value).replace(/\s+/g, " ");
  }

  function previewText(value: string | null | undefined, fallback: string, maxLength = 28): string {
    const cleaned = compactText(value);
    if (!cleaned) return fallback;
    return cleaned.length > maxLength ? `${cleaned.slice(0, maxLength)}...` : cleaned;
  }

  function shortListPreview(
    values: Array<string | null | undefined>,
    fallback: string,
    maxItems = 2,
  ): string {
    const cleaned = cleanLines(values).slice(0, maxItems).map((value) => previewText(value, value, 18));
    return cleaned.length ? cleaned.join(" / ") : fallback;
  }

  function chineseCharacterCount(value: string | null | undefined): number {
    return ((value ?? "").match(/[\u3400-\u9fff]/g) ?? []).length;
  }

  function latinLetterCount(value: string | null | undefined): number {
    return ((value ?? "").match(/[A-Za-z]/g) ?? []).length;
  }

  function isEnglishHeavy(value: string | null | undefined): boolean {
    const cleaned = compactText(value);
    if (!cleaned) return false;
    return latinLetterCount(cleaned) > Math.max(6, chineseCharacterCount(cleaned));
  }

  function uniqueCleanLines(values: Array<string | null | undefined>): string[] {
    const seen = new Set<string>();
    const next: string[] = [];
    for (const raw of values) {
      const value = cleanText(raw);
      if (!value || seen.has(value)) continue;
      seen.add(value);
      next.push(value);
    }
    return next;
  }

  function preferredChineseLines(
    values: Array<string | null | undefined>,
    fallback: Array<string | null | undefined> = [],
  ): string[] {
    const primary = uniqueCleanLines(values).filter((value) => !isEnglishHeavy(value));
    if (primary.length) return primary;
    return uniqueCleanLines([...fallback, ...values]);
  }

  function roleHintForPack(pack: MaterialPackView): string {
    const source = packSearchText(pack);
    if (containsAnyText(source, ["健康", "长寿", "代谢", "胰岛素", "营养", "脂肪"])) {
      return "健康科普讲师";
    }
    if (containsAnyText(source, ["情感", "恋爱", "分手", "婚姻", "关系"])) {
      return "情感讲述者";
    }
    return "知识型口播讲述者";
  }

  function localizedPersona(pack: MaterialPackView): string {
    const raw = cleanText(pack.speaker_profile.persona);
    if (raw && !isEnglishHeavy(raw)) return raw;

    const source = compactText(
      [
        pack.speaker_profile.persona,
        pack.video_prompt_draft.reusable_prompt,
        pack.video_prompt_draft.visual_brief,
      ].join(" "),
    ).toLowerCase();

    const age =
      /middle-aged|late 30s|mid 30s|40s/.test(source) ? "35-45 岁" : /young/.test(source) ? "年轻" : "";
    const ethnicity = /east asian/.test(source) ? "东亚" : "";
    const gender = /female|woman/.test(source) ? "女性" : /male|man/.test(source) ? "男性" : "";
    const lead = `${age}${ethnicity}${gender}${roleHintForPack(pack)}`;
    const descriptors = uniqueCleanLines([
      lead,
      /short black hair/.test(source) ? "短黑发" : "",
      /thin-?rimmed glasses/.test(source) ? "戴细框眼镜" : /glasses/.test(source) ? "戴眼镜" : "",
      /lavender/.test(source) && /shirt/.test(source) ? "穿浅紫色衬衫" : /shirt/.test(source) ? "穿衬衫" : "",
      /navy vest|dark navy vest/.test(source) ? "深蓝色马甲" : /vest/.test(source) ? "穿马甲" : "",
      /patterned tie/.test(source) ? "花纹领带" : /tie/.test(source) ? "佩戴领带" : "",
      /armchair|wooden chair|ornate wooden chair|classic wooden/.test(source) ? "坐在木质扶手椅上" : "",
      /directly to the camera|to the camera/.test(source) ? "面对镜头口播" : "面对镜头表达",
      /gesture/.test(source) ? "手势自然克制" : "",
    ]);
    return descriptors.join("，") || `${roleHintForPack(pack)}，面对镜头口播，形象专业可信`;
  }

  function localizedTone(pack: MaterialPackView): string {
    const raw = cleanText(pack.speaker_profile.tone);
    if (raw && !isEnglishHeavy(raw)) return raw;

    const source = compactText(
      [pack.speaker_profile.tone, pack.video_prompt_draft.reusable_prompt, pack.video_prompt_draft.visual_brief].join(
        " ",
      ),
    ).toLowerCase();
    const phrases = uniqueCleanLines([
      /calm|measured|deliberate|thoughtful/.test(source) ? "沉稳、克制" : "",
      /reassuring|empathetic|approachable/.test(source) ? "亲和、可信" : "",
      /informative|scientific|rigorous|authoritative|professional/.test(source) ? "专业、严谨" : "",
      /curiosity|reflection/.test(source) ? "引导观众理解机制而不是制造焦虑" : "",
    ]);
    return phrases.join("，") || "专业、亲和、可信，表达稳，节奏清晰";
  }

  function localizedVisualBrief(pack: MaterialPackView): string {
    const raw = cleanText(pack.video_prompt_draft.visual_brief);
    if (raw && !isEnglishHeavy(raw)) return raw;

    const source = compactText(
      [
        pack.video_prompt_draft.visual_brief,
        pack.video_prompt_draft.reusable_prompt,
        pack.speaker_profile.persona,
        pack.speaker_profile.tone,
      ].join(" "),
    ).toLowerCase();
    const clauses = uniqueCleanLines([
      /deep green|olive-green|green velvet curtains/.test(source) ? "背景以深绿色丝绒窗帘为主" : "",
      /bookshelf|books/.test(source) ? "右侧可带虚化书架和少量书本" : "",
      /medium close-up|chest-up/.test(source) ? "竖屏 9:16，中近景胸像构图" : "竖屏 9:16，中近景构图",
      /static|steady|no cuts|no movement|no camera movement/.test(source)
        ? "机位平视，画面基本静止，仅保留轻微人物动作和口型变化"
        : "",
      /soft|balanced|studio lighting|three-point/.test(source) ? "布光柔和通透，主光干净，辅光轻微" : "",
      /warm|amber|warm-neutral/.test(source) ? "整体暖中性色调，肤色自然" : "",
      /depth of field|background softly blurred|shallow/.test(source)
        ? "人物主体清晰，背景轻微虚化，保留适度景深"
        : "",
      /text overlays|subtitles|typography|infographic/.test(source) ? "画面加入简洁中文字幕和信息条，不遮挡人物脸部" : "",
      containsAnyText(packSearchText(pack), ["健康", "长寿", "代谢", "胰岛素"])
        ? "整体是高质量中文健康科普短视频质感"
        : "整体是高质量中文知识型口播短视频质感",
    ]);
    return clauses.join("；");
  }

  function localizedReusablePrompt(pack: MaterialPackView): string {
    const raw = cleanText(pack.video_prompt_draft.reusable_prompt);
    if (raw && !isEnglishHeavy(raw)) return raw;

    const topic = cleanText(pack.topic) || "中文知识口播";
    const persona = localizedPersona(pack);
    const tone = localizedTone(pack);
    const visualBrief = localizedVisualBrief(pack);
    const spokenBrief =
      cleanText(pack.video_prompt_draft.spoken_brief) || cleanText(pack.editable_script.hook) || "围绕主题做清晰解释";

    return [
      `一条竖屏 9:16 的中文真人口播短视频，主题围绕“${topic}”。`,
      `人物设定为${persona}。`,
      `表达气质为${tone}。`,
      `${visualBrief}。`,
      `口播核心表达：“${spokenBrief}”。`,
      "字幕简洁，不遮脸，口型自然同步，适合短视频平台传播。",
    ].join("");
  }

  function localizedPromptMaterial(pack: MaterialPackView): LocalizedPromptMaterial {
    const promoCopy = preferredChineseLines(pack.promo_copy);
    const coreMessage = preferredChineseLines(pack.core_message, promoCopy);
    const titleCandidates = preferredChineseLines(pack.title_candidates, [pack.topic, ...promoCopy, ...coreMessage]);
    const coverCandidates = preferredChineseLines(pack.cover_copy_candidates, [...promoCopy, ...titleCandidates]);
    const hook = cleanText(pack.editable_script.hook) || cleanText(pack.video_prompt_draft.spoken_brief) || "未提供";
    const scriptBody = preferredChineseLines(pack.editable_script.body, [...coreMessage, ...promoCopy]);
    const ending = cleanText(pack.editable_script.ending) || promoCopy[0] || hook;

    return {
      topic: cleanText(pack.topic) || "中文知识口播短视频",
      audience: cleanText(pack.audience) || "短视频中文用户",
      persona: localizedPersona(pack),
      tone: localizedTone(pack),
      coreMessage,
      hook,
      scriptBody: scriptBody.length ? scriptBody : [hook],
      ending,
      titleCandidates,
      coverCandidates,
      promoCopy,
      visualBrief: localizedVisualBrief(pack),
      spokenBrief: cleanText(pack.video_prompt_draft.spoken_brief) || hook,
      reusablePrompt: localizedReusablePrompt(pack),
    };
  }

  function packSearchText(pack: MaterialPackView | null): string {
    if (!pack) return "";
    return compactText(
      [
        pack.topic,
        pack.audience,
        pack.speaker_profile.persona,
        pack.speaker_profile.tone,
        ...pack.core_message,
        pack.editable_script.hook,
        ...pack.editable_script.body,
        pack.editable_script.ending,
        ...pack.title_candidates,
        ...pack.cover_copy_candidates,
        ...pack.promo_copy,
        pack.video_prompt_draft.visual_brief,
        pack.video_prompt_draft.spoken_brief,
        pack.video_prompt_draft.reusable_prompt,
      ].join(" "),
    );
  }

  function guessSourceKindFromValue(value: string): SourceKind {
    const trimmed = value.trim();
    if (
      trimmed.startsWith("http://") ||
      trimmed.startsWith("https://") ||
      trimmed.includes("douyin.com") ||
      trimmed.includes("iesdouyin.com")
    ) {
      return "douyin_url";
    }
    return "local_video";
  }

  function parsedCompetitorSourceValues(): string[] {
    return draftCompetitorSourcesText
      .split(/\r?\n/)
      .map((value) => value.trim())
      .filter((value) => value.length > 0);
  }

  function effectiveCompetitorCount(): number {
    return draftJob.mode === "competitor"
      ? parsedCompetitorSourceValues().length
      : sanitizeNonNegativeInt(draftJob.competitorCount);
  }

  function buildCompetitorSourceBundle(): CompetitorSourceBundle {
    return {
      primary: {
        kind: draftJob.sourceKind,
        value: draftJob.sourceValue.trim(),
        label: "当前视频",
      },
      competitors: parsedCompetitorSourceValues().map((value, index) => ({
        kind: guessSourceKindFromValue(value),
        value,
        label: `竞品 ${index + 1}`,
      })),
    };
  }

  function parseCompetitorSourceBundle(raw: string): CompetitorSourceBundle | null {
    try {
      const parsed = JSON.parse(raw) as CompetitorSourceBundle;
      if (!parsed?.primary?.value) return null;
      return parsed;
    } catch {
      return null;
    }
  }

  function jobSourceSummary(job: JobView): string {
    if (job.mode !== "competitor") return job.sourceValue;
    const parsed = parseCompetitorSourceBundle(job.sourceValue);
    if (!parsed) return job.sourceValue;
    return `当前 1 条 + 竞品 ${parsed.competitors.length} 条`;
  }

  function jobSourceDetail(job: JobView): string {
    if (job.mode !== "competitor") return job.sourceValue;
    const parsed = parseCompetitorSourceBundle(job.sourceValue);
    if (!parsed) return job.sourceValue;
    const primary = parsed.primary.value;
    return `${primary}；竞品 ${parsed.competitors.length} 条`;
  }

  function currentPromptPlatformOption() {
    return promptPlatformOptions.find((option) => option.key === promptPlatform) ?? promptPlatformOptions[0];
  }

  function currentPromptVersionOption() {
    return promptVersionOptions.find((option) => option.key === promptVersion) ?? promptVersionOptions[0];
  }

  function currentPromptFocusOption() {
    return promptFocusOptions.find((option) => option.key === promptFocus) ?? promptFocusOptions[0];
  }

  function activePromptTweakOptions() {
    return promptTweakOptions.filter((option) => activePromptTweaks.includes(option.key));
  }

  function materialPromptSignature(pack: MaterialPackView | null): string {
    if (!pack) return "";
    return JSON.stringify({
      jobId: pack.job_id,
      platform: promptPlatform,
      version: promptVersion,
      focus: promptFocus,
      tweaks: [...activePromptTweaks].sort(),
      topic: pack.topic,
      visualBrief: pack.video_prompt_draft.visual_brief,
      spokenBrief: pack.video_prompt_draft.spoken_brief,
    });
  }

  function togglePromptTweak(key: PromptTweakKey) {
    activePromptTweaks = activePromptTweaks.includes(key)
      ? activePromptTweaks.filter((value) => value !== key)
      : [...activePromptTweaks, key];
  }

  function buildMaterialTabContent(tab: MaterialTabKey, pack: MaterialPackView): string {
    const localized = localizedPromptMaterial(pack);
    const persona = localized.persona;
    const tone = localized.tone;
    const coreMessage = localized.coreMessage;
    const scriptBody = localized.scriptBody;
    const titles = localized.titleCandidates;
    const covers = localized.coverCandidates;
    const promoCopy = localized.promoCopy;

    if (tab === "raw") {
      return [
        `topic: ${cleanText(pack.topic) || "未提供"}`,
        `audience: ${cleanText(pack.audience) || "未提供"}`,
        `persona: ${persona || "未提供"}`,
        `tone: ${tone || "未提供"}`,
        "",
        "core_message:",
        coreMessage.length ? bulletedLines(coreMessage) : "- 暂无结构化核心信息",
      ].join("\n");
    }

    if (tab === "ocr") {
      return [
        "当前 MVP 先展示素材包里沉淀出的字幕/文案线索；更原始的 OCR 结果仍保存在任务 analysis 目录。",
        "",
        "key_lines:",
        cleanLines([...coreMessage, ...promoCopy]).length
          ? bulletedLines(cleanLines([...coreMessage, ...promoCopy]))
          : "- 暂无可展示的 OCR 线索",
      ].join("\n");
    }

    if (tab === "vision") {
      return [
        `人物设定: ${persona || "未提供"}`,
        `表达气质: ${tone || "未提供"}`,
        "",
        `visual_brief: ${localized.visualBrief || "未提供"}`,
        "",
        `reusable_prompt_base: ${localized.reusablePrompt || "未提供"}`,
      ].join("\n");
    }

    if (tab === "script") {
      return [
        `钩子: ${localized.hook || "未提供"}`,
        "",
        "正文:",
        scriptBody.length ? bulletedLines(scriptBody) : "- 暂无正文段落",
        "",
        `收尾: ${localized.ending || "未提供"}`,
      ].join("\n");
    }

    if (tab === "copy") {
      return [
        "标题候选:",
        titles.length ? numberedLines(titles) : "1. 暂无标题候选",
        "",
        "封面文案:",
        covers.length ? bulletedLines(covers) : "- 暂无封面文案",
        "",
        "宣传短句:",
        promoCopy.length ? bulletedLines(promoCopy) : "- 暂无宣传短句",
      ].join("\n");
    }

    return [
      "完整提示词会结合人物、话术、标题和画面简报自动生成。",
      "",
      `visual_brief: ${localized.visualBrief || "未提供"}`,
      `spoken_brief: ${localized.spokenBrief || "未提供"}`,
      `reusable_prompt_base: ${localized.reusablePrompt || "未提供"}`,
    ].join("\n");
  }

  function buildFullMaterialPrompt(pack: MaterialPackView): string {
    const localized = localizedPromptMaterial(pack);
    const topic = localized.topic;
    const audience = localized.audience;
    const persona = localized.persona || "专业中文讲述者";
    const tone = localized.tone || "专业、克制、可信";
    const coreMessage = localized.coreMessage;
    const scriptBody = localized.scriptBody;
    const titleCandidates = localized.titleCandidates.slice(0, 3);
    const coverCandidates = localized.coverCandidates.slice(0, 3);
    const promoCopy = localized.promoCopy.slice(0, 2);
    const platform = currentPromptPlatformOption();
    const version = currentPromptVersionOption();
    const focus = currentPromptFocusOption();
    const tweakLines = activePromptTweakOptions().map((option) => option.promptLine);
    const visualBrief = localized.visualBrief;
    const spokenBrief = localized.spokenBrief;
    const reusablePrompt = localized.reusablePrompt;
    const scriptSection = [
      `- 开场钩子：${localized.hook || "未提供"}`,
      ...(scriptBody.length
        ? scriptBody.map((value, index) => `- 正文要点 ${index + 1}：${value}`)
        : ["- 正文要点：未提供"]),
      `- 收尾：${localized.ending || "未提供"}`,
    ].join("\n");
    const titlesLine = titleCandidates.length ? titleCandidates.join(" / ") : "未提供";
    const coversLine = coverCandidates.length ? coverCandidates.join(" / ") : "未提供";
    const promoLine = promoCopy.length ? promoCopy.join(" / ") : "未提供";

    if (version.key === "compact") {
      return [
        `为 ${platform.label} 生成一条中文竖屏 9:16 真人口播短视频提示词。`,
        `主题是“${topic}”，受众为${audience}，人物设定为${persona}，表达气质${tone}。`,
        coreMessage.length ? `核心表达包括：${coreMessage.join("、")}。` : "",
        `开场先说“${localized.hook || "未提供"}”，再围绕${scriptBody.join("；") || "核心信息"}展开，最后用“${localized.ending || "未提供"}”收束。`,
        visualBrief ? `画面上要${visualBrief}。` : "",
        spokenBrief ? `口播上要${spokenBrief}。` : "",
        `重点要求：${focus.instruction}`,
        tweakLines.length ? `额外优化：${tweakLines.join("；")}` : "",
        `字幕不要遮脸，中文输出，标题可参考“${titlesLine}”，封面文案可参考“${coversLine}”。`,
      ]
        .filter((value) => value.length > 0)
        .join("");
    }

    const promptSections = [
      `请为 ${platform.label} 输出一条中文竖屏 9:16 真人口播短视频生成提示词。`,
      `平台适配要求：${platform.instruction}`,
      `版本策略：${version.summary}`,
      `优化目标：${focus.instruction}`,
      `主题：${topic}`,
      `目标受众：${audience}`,
      `人物设定：${persona}`,
      `表达气质：${tone}`,
      coreMessage.length ? `核心信息：\n${bulletedLines(coreMessage)}` : "",
      `口播结构：\n${scriptSection}`,
      visualBrief ? `画面要求：${visualBrief}` : "",
      spokenBrief ? `口播提醒：${spokenBrief}` : "",
      tweakLines.length ? `额外调优：\n${bulletedLines(tweakLines)}` : "",
      "通用质量要求：全程中文表达，口型自然同步，人物面对镜头，中近景构图，动作克制，字幕与信息条简洁克制，不遮挡人物脸部。",
      `标题参考：${titlesLine}`,
      `封面文案参考：${coversLine}`,
      `宣传短句参考：${promoLine}`,
      reusablePrompt ? `可复用画面基底：${reusablePrompt}` : "",
    ].filter((value) => value.length > 0);

    if (version.key === "balanced") {
      return [
        promptSections[0],
        promptSections[1],
        promptSections[3],
        promptSections[4],
        promptSections[6],
        promptSections[7],
        coreMessage.length ? `核心信息：${coreMessage.join(" / ")}` : "",
        `口播结构：${localized.hook || "未提供"} -> ${scriptBody.join(" / ") || "核心要点"} -> ${localized.ending || "未提供"}`,
        visualBrief ? `画面要求：${visualBrief}` : "",
        tweakLines.length ? `额外调优：${tweakLines.join("；")}` : "",
        "硬性要求：中文口播、口型自然、字幕不遮脸、人物主体清晰。",
        `标题参考：${titlesLine}`,
        reusablePrompt ? `画面基底：${reusablePrompt}` : "",
      ]
        .filter((value) => value.length > 0)
        .join("\n\n");
    }

    return promptSections.join("\n\n");
  }

  function buildPromptDraftBlock(pack: MaterialPackView): string {
    const localized = localizedPromptMaterial(pack);
    return [
      `visual_brief: ${localized.visualBrief || "未提供"}`,
      "",
      `spoken_brief: ${localized.spokenBrief || "未提供"}`,
      "",
      `reusable_prompt: ${localized.reusablePrompt || "未提供"}`,
    ].join("\n");
  }

  function textareaLines(value: string): string[] {
    return uniqueCleanLines(value.split(/\r?\n/));
  }

  function joinTextareaLines(values: Array<string | null | undefined>): string {
    return uniqueCleanLines(values).join("\n");
  }

  function buildManualMaterialDraftFromPack(
    pack: MaterialPackView,
    promptText: string,
  ): ManualMaterialDraftView {
    const localized = localizedPromptMaterial(pack);
    return {
      topic: localized.topic,
      audience: localized.audience,
      persona: localized.persona,
      tone: localized.tone,
      hook: localized.hook,
      bodyText: joinTextareaLines(localized.scriptBody),
      ending: localized.ending,
      visualBrief: localized.visualBrief,
      spokenBrief: localized.spokenBrief,
      reusablePrompt: localized.reusablePrompt,
      fullPrompt: cleanText(promptText),
      titleCandidatesText: joinTextareaLines(localized.titleCandidates),
      coverCopyCandidatesText: joinTextareaLines(localized.coverCandidates),
      promoCopyText: joinTextareaLines(localized.promoCopy),
    };
  }

  function hydrateManualMaterialDraftView(file: ManualMaterialDraftFile): ManualMaterialDraftView {
    return {
      topic: cleanText(file.topic),
      audience: cleanText(file.audience),
      persona: cleanText(file.persona),
      tone: cleanText(file.tone),
      hook: cleanText(file.hook),
      bodyText: joinTextareaLines(file.body),
      ending: cleanText(file.ending),
      visualBrief: cleanText(file.visualBrief),
      spokenBrief: cleanText(file.spokenBrief),
      reusablePrompt: cleanText(file.reusablePrompt),
      fullPrompt: cleanText(file.fullPrompt),
      titleCandidatesText: joinTextareaLines(file.titleCandidates),
      coverCopyCandidatesText: joinTextareaLines(file.coverCopyCandidates),
      promoCopyText: joinTextareaLines(file.promoCopy),
    };
  }

  function serializeManualMaterialDraft(view: ManualMaterialDraftView): ManualMaterialDraftFile {
    return {
      sourceJobId: materialPack?.job_id ?? selectedMaterialJobId,
      updatedAtMs: Date.now(),
      platformLabel: currentPromptPlatformOption().label,
      versionLabel: currentPromptVersionOption().label,
      focusLabel: currentPromptFocusOption().label,
      tweakLabels: activePromptTweakOptions().map((option) => option.label),
      topic: cleanText(view.topic),
      audience: cleanText(view.audience),
      persona: cleanText(view.persona),
      tone: cleanText(view.tone),
      hook: cleanText(view.hook),
      body: textareaLines(view.bodyText),
      ending: cleanText(view.ending),
      visualBrief: cleanText(view.visualBrief),
      spokenBrief: cleanText(view.spokenBrief),
      reusablePrompt: cleanText(view.reusablePrompt),
      fullPrompt: cleanText(view.fullPrompt),
      titleCandidates: textareaLines(view.titleCandidatesText),
      coverCopyCandidates: textareaLines(view.coverCopyCandidatesText),
      promoCopy: textareaLines(view.promoCopyText),
    };
  }

  function buildManualMaterialPreview(view: ManualMaterialDraftView): string {
    const bodyLines = textareaLines(view.bodyText);
    const titleLines = textareaLines(view.titleCandidatesText);
    const coverLines = textareaLines(view.coverCopyCandidatesText);
    const promoLines = textareaLines(view.promoCopyText);
    const tweakLabels = activePromptTweakOptions().map((option) => option.label).join(" / ") || "未启用";

    return [
      `主题: ${cleanText(view.topic) || "未填写"}`,
      `受众: ${cleanText(view.audience) || "未填写"}`,
      `目标平台: ${currentPromptPlatformOption().label}`,
      `提示词版本: ${currentPromptVersionOption().label}`,
      `优化目标: ${currentPromptFocusOption().label}`,
      `调优项: ${tweakLabels}`,
      "",
      "口播结构:",
      `- 开场钩子: ${cleanText(view.hook) || "未填写"}`,
      ...(bodyLines.length ? bodyLines.map((line, index) => `- 正文要点 ${index + 1}: ${line}`) : ["- 正文要点: 未填写"]),
      `- 收尾: ${cleanText(view.ending) || "未填写"}`,
      "",
      `visual_brief: ${cleanText(view.visualBrief) || "未填写"}`,
      "",
      `spoken_brief: ${cleanText(view.spokenBrief) || "未填写"}`,
      "",
      `reusable_prompt: ${cleanText(view.reusablePrompt) || "未填写"}`,
      "",
      "标题候选:",
      titleLines.length ? numberedLines(titleLines) : "1. 暂无标题候选",
      "",
      "封面文案:",
      coverLines.length ? bulletedLines(coverLines) : "- 暂无封面文案",
      "",
      "宣传短句:",
      promoLines.length ? bulletedLines(promoLines) : "- 暂无宣传短句",
      "",
      "完整提示词:",
      cleanText(view.fullPrompt) || "未填写",
    ].join("\n");
  }

  function cloneManualMaterialDraftFromCurrentPack(showNotice = true) {
    if (!materialPack) return;
    manualMaterialDraft = buildManualMaterialDraftFromPack(materialPack, materialPromptText);
    manualMaterialAutoSyncPrompt = true;
    manualMaterialLastSaveResult = null;
    if (showNotice) {
      materialTone = "good";
      materialMessage = "已从当前素材包复刻一份可手工改写的新视频素材草稿。";
    }
  }

  function syncManualMaterialPromptFromCurrent(showNotice = true) {
    if (!manualMaterialDraft) return;
    manualMaterialDraft = {
      ...manualMaterialDraft,
      fullPrompt: cleanText(materialPromptText),
    };
    manualMaterialAutoSyncPrompt = true;
    if (showNotice) {
      materialTone = "good";
      materialMessage = "已把当前完整提示词带入人工改稿区。";
    }
  }

  async function saveManualMaterialDraft() {
    if (!manualMaterialDraft || !selectedMaterialJobId) return;
    manualMaterialSaving = true;
    try {
      const result = await invoke<ManualMaterialDraftSaveResult>("save_job_manual_material_draft", {
        jobId: selectedMaterialJobId,
        draft: serializeManualMaterialDraft(manualMaterialDraft),
      });
      manualMaterialLastSaveResult = result;
      materialTone = "good";
      materialMessage = `已保存新素材草稿，并同步写入 ${result.draftPath}。`;
    } catch (error) {
      materialTone = "warn";
      materialMessage = `保存新素材草稿失败：${stringifyError(error)}`;
    } finally {
      manualMaterialSaving = false;
    }
  }

  function toggleCompetitorMetric(key: CompetitorMetricKey) {
    if (activeCompetitorMetrics.includes(key)) {
      activeCompetitorMetrics = activeCompetitorMetrics.filter((value) => value !== key);
    } else {
      activeCompetitorMetrics = [...activeCompetitorMetrics, key];
    }
  }

  function setCompetitorMetricWeight(key: CompetitorMetricKey, nextValue: number) {
    const normalized = Math.min(5, Math.max(1, sanitizePositiveInt(nextValue, 1)));
    competitorMetricWeights = { ...competitorMetricWeights, [key]: normalized };
  }

  function buildCompetitorInsights(pack: MaterialPackView | null): CompetitorInsight[] {
    return competitorMetricOptions.map((option) => {
      const weight = competitorMetricWeights[option.key] ?? 1;
      const currentScore = option.currentScore(pack);
      const benchmarkScore = option.benchmarkScore;
      const gap = round1(Math.max(0, benchmarkScore - currentScore));

      return {
        key: option.key,
        label: option.label,
        summary: option.summary,
        enabled: activeCompetitorMetrics.includes(option.key),
        weight,
        currentScore,
        benchmarkScore,
        gap,
        weightedGap: round1(gap * weight),
        currentNote: option.currentNote(pack),
        benchmarkNote: option.benchmarkNote,
        action: option.action,
        rewriteHint: option.rewriteHint(pack),
        promptTweaks: option.promptTweaks,
        promptFocus: option.promptFocus,
      };
    });
  }

  function buildCompetitorInsightsFromReport(report: CompetitorReportView): CompetitorInsight[] {
    return report.metrics.map((metric) => ({
      key: metric.key,
      label: metric.label,
      summary: metric.summary,
      enabled: activeCompetitorMetrics.includes(metric.key),
      weight: competitorMetricWeights[metric.key] ?? 1,
      currentScore: metric.current_score,
      benchmarkScore: metric.competitor_score,
      gap: round1(Math.max(0, metric.competitor_score - metric.current_score)),
      weightedGap: round1(Math.max(0, metric.competitor_score - metric.current_score) * (competitorMetricWeights[metric.key] ?? 1)),
      currentNote: metric.current_note,
      benchmarkNote: metric.benchmark_note,
      action: metric.action,
      rewriteHint: metric.rewrite_hint,
      promptTweaks: metric.prompt_tweaks,
      promptFocus: metric.prompt_focus,
    }));
  }

  function aggregateCompetitorScore(
    rows: CompetitorInsight[],
    key: "currentScore" | "benchmarkScore",
  ): number {
    if (!rows.length) return 0;
    const totalWeight = rows.reduce((sum, row) => sum + row.weight, 0);
    if (!totalWeight) return 0;
    const weighted = rows.reduce((sum, row) => sum + row[key] * row.weight, 0);
    return round1(weighted / totalWeight);
  }

  function deriveCompetitorFocus(rows: CompetitorInsight[]): PromptFocusKey {
    const buckets: Record<PromptFocusKey, number> = {
      balanced: 0,
      lip_sync: 0,
      visual: 0,
    };

    for (const row of rows) {
      buckets[row.promptFocus] += row.weightedGap || row.gap;
    }

    return (Object.entries(buckets).sort((a, b) => b[1] - a[1])[0]?.[0] as PromptFocusKey) ?? "balanced";
  }

  function deriveCompetitorTweaks(rows: CompetitorInsight[]): PromptTweakKey[] {
    const selected: PromptTweakKey[] = [];

    for (const row of rows) {
      for (const tweak of row.promptTweaks) {
        if (!selected.includes(tweak)) selected.push(tweak);
      }
      if (selected.length >= 4) break;
    }

    return selected;
  }

  function promptFocusLabel(key: PromptFocusKey): string {
    return promptFocusOptions.find((option) => option.key === key)?.label ?? key;
  }

  function applyCompetitorRecommendations() {
    if (!competitorPack || !selectedCompetitorInsightsList.length || !selectedCompetitorJobId) return;

    selectedMaterialJobId = selectedCompetitorJobId;
    materialPack = competitorPack;
    loadedMaterialJobId = selectedCompetitorJobId;
    promptFocus = competitorRecommendedFocusKey;
    activePromptTweaks = [...competitorRecommendedTweakKeys];
    materialTab = "prompt";
    materialPromptText = buildFullMaterialPrompt(competitorPack);
    lastMaterialPromptSignature = materialPromptSignature(competitorPack);
    cloneManualMaterialDraftFromCurrentPack(false);
    materialTone = "good";
    materialMessage = `已同步竞品分析建议：${competitorRecommendedTweakOptionsList
      .map((option) => option.label)
      .join(" / ") || "保持当前策略"}，并切到完整提示词。`;
    currentPage = "material";
  }

  function effectiveTextPreset(): TextPreset | null {
    if (!settingsDraft) return null;
    return draftJob.tierOverride === "pro"
      ? settingsDraft.textProvider.presets.pro
      : settingsDraft.textProvider.presets.flash;
  }

  function routeSummary(): string {
    const preset = effectiveTextPreset();
    if (!preset || !settingsDraft) return "未加载";
    const route =
      settingsDraft.textProvider.routeKind === "custom"
        ? settingsDraft.textProvider.customBaseUrl || preset.baseUrl
        : preset.baseUrl;
    return `${preset.model} · ${route}`;
  }

  function buildDraftSignature(): string {
    return JSON.stringify({
      ...draftJob,
      competitorSourcesText: draftCompetitorSourcesText,
      effectiveCompetitorCount: effectiveCompetitorCount(),
      limits: settings?.limits ?? null,
      budget: settings?.budget ?? null,
      routeKind: settingsDraft?.textProvider.routeKind ?? null,
      customBaseUrl: settingsDraft?.textProvider.customBaseUrl ?? null,
      flashModel: settingsDraft?.textProvider.presets.flash.model ?? null,
      proModel: settingsDraft?.textProvider.presets.pro.model ?? null,
      visionModel: settingsDraft?.visionProvider.model ?? null,
      visionBaseUrl: settingsDraft?.visionProvider.baseUrl ?? null,
    });
  }

  function sanitizePositiveInt(value: number, fallback: number): number {
    const numeric = Number(value);
    if (!Number.isFinite(numeric)) return fallback;
    return Math.max(1, Math.round(numeric));
  }

  function sanitizeNonNegativeInt(value: number): number {
    const numeric = Number(value);
    if (!Number.isFinite(numeric)) return 0;
    return Math.max(0, Math.round(numeric));
  }

  function buildEstimateRequest(): EstimateJobRequest {
    return {
      mode: draftJob.mode,
      sourceKind: draftJob.sourceKind,
      durationMinutes: sanitizePositiveInt(draftJob.durationMinutes, 1),
      frameCount: sanitizePositiveInt(draftJob.frameCount, 1),
      competitorCount: effectiveCompetitorCount(),
      textTier: draftJob.tierOverride,
    };
  }

  function buildCreateJobRequest(): CreateJobRequest {
    return {
      name: draftJob.name.trim(),
      mode: draftJob.mode,
      sourceKind: draftJob.sourceKind,
      sourceValue:
        draftJob.mode === "competitor"
          ? JSON.stringify(buildCompetitorSourceBundle())
          : draftJob.sourceValue.trim(),
      durationMinutes: sanitizePositiveInt(draftJob.durationMinutes, 1),
      frameCount: sanitizePositiveInt(draftJob.frameCount, 1),
      competitorCount: effectiveCompetitorCount(),
      textTier: draftJob.tierOverride,
    };
  }

  function activeBatchCostCny(): number {
    return jobs
      .filter((job) => job.status === "waiting" || job.status === "running")
      .reduce((sum, job) => sum + Math.max(job.estimatedCostCny, job.actualCostCny), 0);
  }

  function projectedBatchCostCny(): number | null {
    if (!estimate) return null;
    return activeBatchCostCny() + estimate.estimatedCostCny;
  }

  function startWouldBeBlocked(): boolean {
    if (!estimate || !settings) return false;
    if (!settings.budget.blockWhenOverBudget) return false;
    if (estimate.exceedsJobBudget) return true;
    const projected = projectedBatchCostCny();
    return projected !== null && settings.budget.perBatchCny > 0 && projected > settings.budget.perBatchCny;
  }

  function estimateNotice(): { tone: NoticeTone; text: string } | null {
    if (!estimate || !settings) return null;
    const projected = projectedBatchCostCny();
    if (estimate.exceedsJobBudget && settings.budget.blockWhenOverBudget) {
      return { tone: "warn", text: "该任务超过单任务预算，按当前策略会被拦截。" };
    }
    if (
      projected !== null &&
      settings.budget.perBatchCny > 0 &&
      projected > settings.budget.perBatchCny &&
      settings.budget.blockWhenOverBudget
    ) {
      return { tone: "warn", text: "加上当前队列后会超过批次预算，按当前策略会被拦截。" };
    }
    if (
      (estimate.exceedsJobBudget ||
        (projected !== null &&
          settings.budget.perBatchCny > 0 &&
          projected > settings.budget.perBatchCny)) &&
      !settings.budget.blockWhenOverBudget
    ) {
      return { tone: "warn", text: "预算已超线，但当前是“仅警告”策略，仍可入队。" };
    }
    return { tone: "good", text: "当前预估处于可执行范围内。" };
  }

  async function loadSettings() {
    settings = await invoke<RuntimeSettingsView>("get_runtime_settings");
    settingsDraft = cloneSettingsView(settings);
    pendingTextApiKey = "";
    pendingVisionApiKey = "";
  }

  async function loadEnvironmentReport() {
    loadingEnvironmentReport = true;
    try {
      environmentReport = await invoke<EnvironmentHealthReport>("check_runtime_environment");
    } catch (error) {
      environmentReport = null;
      settingsTone = "warn";
      settingsMessage = `环境检查失败：${stringifyError(error)}`;
    } finally {
      loadingEnvironmentReport = false;
    }
  }

  function environmentHeadline(): string {
    if (!environmentReport) return "尚未执行环境检查。";
    if (environmentReport.missingCount > 0) {
      return `还有 ${environmentReport.missingCount} 项缺失，建议先补齐再交付给最终使用者。`;
    }
    if (environmentReport.warningCount > 0) {
      return `还有 ${environmentReport.warningCount} 项需要留意，当前可以继续试跑。`;
    }
    return "当前运行环境已满足交付版基本要求。";
  }

  async function openEnvironmentSetupScript() {
    try {
      const path = await invoke<string>("open_environment_setup_script");
      settingsTone = "good";
      settingsMessage = `已打开环境引导脚本：${path}`;
    } catch (error) {
      settingsTone = "warn";
      settingsMessage = `打开环境引导脚本失败：${stringifyError(error)}`;
    }
  }

  async function runDouyinCookieLogin() {
    try {
      const message = await invoke<string>("run_douyin_cookie_login");
      settingsTone = "good";
      settingsMessage = `${message} 完成后点一次“重新检查”，确认 Cookie 已写回。`;
    } catch (error) {
      settingsTone = "warn";
      settingsMessage = `启动抖音登录失败：${stringifyError(error)}`;
    }
  }

  function openEnvironmentPanel() {
    currentPage = "settings";
  }

  async function loadDashboard() {
    dashboard = await invoke<DashboardSnapshot>("get_dashboard_snapshot");
  }

  async function loadJobs() {
    jobs = await invoke<JobView[]>("list_jobs");
  }

  async function refreshQueueState(showSpinner = false) {
    if (showSpinner) refreshingQueue = true;
    try {
      await Promise.all([loadDashboard(), loadJobs()]);
      await tick();
      await refreshSelectedStageLog();
    } catch (error) {
      queueTone = "warn";
      queueMessage = `刷新队列失败：${stringifyError(error)}`;
    } finally {
      if (showSpinner) refreshingQueue = false;
    }
  }

  async function bootstrap() {
    try {
      await loadSettings();
      await Promise.all([refreshQueueState(true), loadEnvironmentReport()]);
    } catch (error) {
      settingsTone = "warn";
      settingsMessage = `初始化失败：${stringifyError(error)}`;
    } finally {
      loadingBootstrap = false;
    }
  }

  onMount(() => {
    void bootstrap();
    const timer = window.setInterval(() => {
      void refreshQueueState();
    }, 1200);
    return () => window.clearInterval(timer);
  });

  async function chooseLocalVideo() {
    const selected = await open({
      multiple: false,
      filters: [{ name: "Video", extensions: ["mp4", "mov", "mkv", "avi"] }],
    });
    if (!selected || Array.isArray(selected)) return;
    draftJob.sourceKind = "local_video";
    draftJob.sourceValue = selected;
  }

  function prepareCompetitorDraft() {
    draftJob.mode = "competitor";
    if (!draftJob.name.trim() || draftJob.name === "健康科普口播拆解") {
      draftJob.name = "竞品分析任务";
    }
  }

  async function estimateJob() {
    estimating = true;
    newJobMessage = "";
    try {
      estimate = await invoke<EstimateJobResult>("estimate_job_cost", {
        request: buildEstimateRequest(),
      });
      lastEstimatedSignature = buildDraftSignature();
      estimateStale = false;
      newJobTone = "good";
      newJobMessage = "成本预估已更新。";
    } catch (error) {
      newJobTone = "warn";
      newJobMessage = `预估失败：${stringifyError(error)}`;
    } finally {
      estimating = false;
    }
  }

  async function createJob() {
    creatingJob = true;
    newJobMessage = "";
    try {
      const created = await invoke<JobView>("create_job", {
        request: buildCreateJobRequest(),
      });
      await refreshQueueState(true);
      queueTone = "good";
      queueMessage = `任务“${created.name}”已加入队列。`;
      currentPage = "queue";
    } catch (error) {
      newJobTone = "warn";
      newJobMessage = `启动失败：${stringifyError(error)}`;
    } finally {
      creatingJob = false;
    }
  }

  async function estimateCompetitorJob() {
    prepareCompetitorDraft();
    await estimateJob();
  }

  async function createCompetitorJob() {
    prepareCompetitorDraft();
    await createJob();
  }

  async function cancelExistingJob(jobId: string) {
    activeJobActionId = jobId;
    try {
      const updated = await invoke<JobView>("cancel_job", { jobId });
      await refreshQueueState();
      queueTone = "warn";
      queueMessage = `任务“${updated.name}”已取消。`;
    } catch (error) {
      queueTone = "warn";
      queueMessage = `取消失败：${stringifyError(error)}`;
    } finally {
      activeJobActionId = "";
    }
  }

  async function retryExistingJob(jobId: string) {
    activeJobActionId = jobId;
    try {
      const updated = await invoke<JobView>("retry_job", { jobId });
      await refreshQueueState();
      queueTone = "good";
      queueMessage = `任务“${updated.name}”已重新入队。`;
    } catch (error) {
      queueTone = "warn";
      queueMessage = `重试失败：${stringifyError(error)}`;
    } finally {
      activeJobActionId = "";
    }
  }

  async function openJobArtifactDir(job: JobView) {
    try {
      await invoke("open_job_artifact_dir", { jobId: job.id });
      queueTone = "good";
      queueMessage = `已打开任务目录：${job.name}`;
    } catch (error) {
      queueTone = "warn";
      queueMessage = `打开任务目录失败：${stringifyError(error)}`;
    }
  }

  async function openJobMaterialPack(job: JobView) {
    try {
      await invoke("open_job_material_pack", { jobId: job.id });
      queueTone = "good";
      queueMessage = `已定位素材包：${job.name}`;
    } catch (error) {
      queueTone = "warn";
      queueMessage = `打开素材包失败：${stringifyError(error)}`;
    }
  }

  async function loadMaterialPack(jobId: string) {
    loadingMaterialPack = true;
    clearMaterialPromptRewriteTimer();
    try {
      const [nextPack, savedDraft] = await Promise.all([
        invoke<MaterialPackView>("read_job_material_pack", { jobId }),
        invoke<ManualMaterialDraftFile>("read_job_manual_material_draft", { jobId }).catch(() => null),
      ]);
      materialPack = nextPack;
      materialPromptText = buildFullMaterialPrompt(nextPack);
      lastMaterialPromptSignature = materialPromptSignature(nextPack);
      materialPromptGeneratedByModel = "";
      materialPromptUsage = null;
      materialPromptRegenerating = false;
      manualMaterialDraft = savedDraft
        ? hydrateManualMaterialDraftView(savedDraft)
        : buildManualMaterialDraftFromPack(nextPack, materialPromptText);
      manualMaterialAutoSyncPrompt = !savedDraft;
      manualMaterialLastSaveResult = null;
      materialMessage = savedDraft ? "已载入素材包，并恢复上次保存的新视频素材草稿。" : "";
      materialTone = "good";
      loadedMaterialJobId = jobId;
    } catch (error) {
      materialPack = null;
      materialPromptText = "";
      lastMaterialPromptSignature = "";
      materialPromptGeneratedByModel = "";
      materialPromptUsage = null;
      materialPromptRegenerating = false;
      manualMaterialDraft = null;
      manualMaterialAutoSyncPrompt = true;
      manualMaterialLastSaveResult = null;
      materialTone = "warn";
      materialMessage = `读取素材包失败：${stringifyError(error)}`;
      loadedMaterialJobId = "";
    } finally {
      loadingMaterialPack = false;
    }
  }

  async function loadCompetitorPack(jobId: string) {
    loadingCompetitorPack = true;
    try {
      const [nextPack, nextReport] = await Promise.all([
        invoke<MaterialPackView>("read_job_material_pack", { jobId }),
        invoke<CompetitorReportView>("read_job_competitor_report", { jobId }).catch(() => null),
      ]);
      competitorPack = nextPack;
      competitorReport = nextReport;
      loadedCompetitorJobId = jobId;
      competitorTone = "good";
      const loadedName = materialJobs.find((job) => job.id === jobId)?.name;
      competitorMessage = nextReport
        ? loadedName
          ? `已载入 ${loadedName} 的真实竞品报告。`
          : "已载入真实竞品报告。"
        : loadedName
          ? `已载入 ${loadedName} 的分析底稿。`
          : "已载入当前素材包。";
    } catch (error) {
      competitorPack = null;
      competitorReport = null;
      loadedCompetitorJobId = "";
      competitorTone = "warn";
      competitorMessage = `读取竞品分析底稿失败：${stringifyError(error)}`;
    } finally {
      loadingCompetitorPack = false;
    }
  }

  async function regenerateMaterialPrompt(forceAi = true) {
    if (!materialPack) return;
    clearMaterialPromptRewriteTimer();
    const signature = materialPromptSignature(materialPack);
    const templatePrompt = buildFullMaterialPrompt(materialPack);
    materialPromptText = templatePrompt;
    if (manualMaterialDraft && manualMaterialAutoSyncPrompt) {
      manualMaterialDraft = {
        ...manualMaterialDraft,
        fullPrompt: cleanText(templatePrompt),
      };
    }
    lastMaterialPromptSignature = signature;
    materialPromptGeneratedByModel = "";
    materialPromptUsage = null;
    materialTone = "good";
    materialMessage = `已生成 ${currentPromptPlatformOption().label} · ${currentPromptVersionOption().label} · ${currentPromptFocusOption().label} 版本提示词草稿。`;
    materialTab = "prompt";

    if (!forceAi || !settings?.textProvider.hasApiKey) {
      materialPromptRegenerating = false;
      return;
    }

    const requestSeq = ++materialPromptRewriteSeq;
    materialPromptRegenerating = true;
    materialMessage = `正在用 ${selectedMaterialJob?.textTier === "pro" ? "Pro" : "Flash"} 重写中文提示词…`;

    try {
      const result = await invoke<MaterialPromptRewriteResult>("generate_material_prompt", {
        request: {
          basePrompt: templatePrompt,
          textTier: selectedMaterialJob?.textTier ?? settings.textProvider.defaultTier,
          platformLabel: currentPromptPlatformOption().label,
          versionLabel: currentPromptVersionOption().label,
          focusLabel: currentPromptFocusOption().label,
          tweakLabels: activePromptTweakOptions().map((option) => option.label),
        },
      });
      if (requestSeq !== materialPromptRewriteSeq) return;
      materialPromptText = cleanText(result.prompt) || templatePrompt;
      if (manualMaterialDraft && manualMaterialAutoSyncPrompt) {
        manualMaterialDraft = {
          ...manualMaterialDraft,
          fullPrompt: cleanText(materialPromptText),
        };
      }
      materialPromptGeneratedByModel = result.generatedByModel;
      materialPromptUsage = {
        promptTokens: result.llmUsage?.prompt_tokens ?? 0,
        completionTokens: result.llmUsage?.completion_tokens ?? 0,
        totalTokens: result.llmUsage?.total_tokens ?? 0,
        costCny: result.costCny ?? 0,
      };
      materialMessage = `已用 ${result.generatedByModel} 重新生成中文提示词，本次成本 ${formatCurrency(result.costCny ?? 0)}。`;
      materialTone = "good";
      await loadDashboard();
    } catch (error) {
      if (requestSeq !== materialPromptRewriteSeq) return;
      materialTone = "warn";
      materialMessage = `AI 重写失败，先回退到本地模板草稿：${stringifyError(error)}`;
    } finally {
      if (requestSeq === materialPromptRewriteSeq) {
        materialPromptRegenerating = false;
      }
    }
  }

  function queueMaterialPromptRewrite() {
    if (!materialPack) return;
    clearMaterialPromptRewriteTimer();
    materialPromptRewriteTimer = setTimeout(() => {
      void regenerateMaterialPrompt(true);
    }, 280);
  }

  function stageLogSignature(job: JobView): string {
    return [
      job.id,
      job.status,
      job.stageKey,
      job.updatedAtMs,
      job.error ?? "",
      job.materialPackPath ?? "",
      job.stageLogPath,
    ].join("|");
  }

  function isStageLogNearBottom(): boolean {
    if (!stageLogViewport) return true;
    const offsetFromBottom =
      stageLogViewport.scrollHeight -
      stageLogViewport.scrollTop -
      stageLogViewport.clientHeight;
    return offsetFromBottom <= 24;
  }

  function handleStageLogScroll() {
    stageLogStickToBottom = isStageLogNearBottom();
  }

  async function scrollStageLogToBottom(force = false) {
    await tick();
    if (!stageLogViewport) return;
    if (!force && !stageLogStickToBottom) return;
    stageLogViewport.scrollTop = stageLogViewport.scrollHeight;
    stageLogStickToBottom = true;
  }

  async function jumpStageLogToLatest() {
    stageLogStickToBottom = true;
    await scrollStageLogToBottom(true);
  }

  async function refreshSelectedStageLog(options?: {
    force?: boolean;
    showSpinner?: boolean;
  }) {
    if (!selectedLogJob || currentPage !== "queue") return;
    if (loadingStageLog || refreshingStageLog) return;

    const signature = stageLogSignature(selectedLogJob);
    if (!options?.force && signature === selectedStageLogSignature) return;

    const jobId = selectedLogJob.id;
    const shouldStickToBottom = options?.force || isStageLogNearBottom();
    if (options?.showSpinner) {
      loadingStageLog = true;
    } else {
      refreshingStageLog = true;
    }

    try {
      const nextLog = await invoke<string>("read_job_stage_log", { jobId });
      if (selectedLogJob?.id !== jobId) return;
      selectedStageLog = nextLog;
      selectedStageLogSignature = signature;
      stageLogStickToBottom = shouldStickToBottom;
      if (shouldStickToBottom) {
        await scrollStageLogToBottom(true);
      }
    } catch (error) {
      if (selectedLogJob?.id !== jobId) return;
      if (options?.showSpinner) {
        selectedStageLog = "";
      }
      selectedStageLogSignature = "";
      queueTone = "warn";
      queueMessage = `Stage log refresh failed: ${stringifyError(error)}`;
    } finally {
      loadingStageLog = false;
      refreshingStageLog = false;
    }
  }

  async function showJobStageLog(job: JobView) {
    if (selectedLogJob?.id === job.id) {
      selectedLogJob = null;
      selectedStageLog = "";
      selectedStageLogSignature = "";
      stageLogStickToBottom = true;
      stageLogViewport = undefined;
      return;
    }

    selectedLogJob = job;
    selectedStageLog = "";
    selectedStageLogSignature = "";
    stageLogStickToBottom = true;
    await refreshSelectedStageLog({ force: true, showSpinner: true });
  }

  async function saveSettings() {
    if (!settingsDraft) return;
    savingSettings = true;
    settingsMessage = "";
    try {
      await invoke("save_runtime_settings", {
        update: {
          ...settingsDraft,
          textProvider: {
            ...settingsDraft.textProvider,
            textApiKey: pendingTextApiKey.trim(),
          },
          visionProvider: {
            ...settingsDraft.visionProvider,
            visionApiKey: pendingVisionApiKey.trim(),
          },
        } satisfies RuntimeSettingsUpdate,
      });
      settingsTone = "good";
      settingsMessage = "设置已持久化到本地配置目录。";
      await loadSettings();
      await loadEnvironmentReport();
      if (estimate) {
        await estimateJob();
      }
    } catch (error) {
      settingsTone = "warn";
      settingsMessage = `保存失败：${stringifyError(error)}`;
    } finally {
      savingSettings = false;
    }
  }

  async function openSettingsDir() {
    try {
      await invoke("open_runtime_settings_dir");
      settingsTone = "good";
      settingsMessage = "已打开运行时配置目录。";
    } catch (error) {
      settingsTone = "warn";
      settingsMessage = `打开目录失败：${stringifyError(error)}`;
    }
  }

  $: {
    recentJobs = jobs.slice(0, 5);
    latestFinishedJob = jobs.find((job) => job.status === "done");
    latestActiveJob = jobs.find((job) => job.status === "running" || job.status === "waiting");
    materialJobs = jobs.filter((job) => job.status === "done" && Boolean(job.materialPackPath));
    competitorJobs = jobs.filter((job) => job.status === "done" && Boolean(job.competitorReportPath));
    competitorSelectableJobs = competitorJobs.length ? competitorJobs : materialJobs;
    selectedMaterialJob = materialJobs.find((job) => job.id === selectedMaterialJobId) ?? null;
    selectedCompetitorJob = competitorSelectableJobs.find((job) => job.id === selectedCompetitorJobId) ?? null;
    estimateStale = Boolean(estimate) && lastEstimatedSignature !== buildDraftSignature();

    if (!materialJobs.length) {
      selectedMaterialJobId = "";
      loadedMaterialJobId = "";
      materialPack = null;
      materialPromptText = "";
      manualMaterialDraft = null;
      manualMaterialAutoSyncPrompt = true;
      manualMaterialLastSaveResult = null;
      selectedCompetitorJobId = "";
      loadedCompetitorJobId = "";
      competitorPack = null;
      competitorReport = null;
    } else if (!selectedMaterialJobId || !materialJobs.some((job) => job.id === selectedMaterialJobId)) {
      selectedMaterialJobId = materialJobs[0].id;
    }

    if (
      !selectedCompetitorJobId ||
      !competitorSelectableJobs.some((job) => job.id === selectedCompetitorJobId)
    ) {
      selectedCompetitorJobId =
        competitorSelectableJobs[0]?.id || selectedMaterialJobId || materialJobs[0]?.id || "";
    }

    if (selectedLogJob) {
      const refreshed = jobs.find((job) => job.id === selectedLogJob?.id);
      if (refreshed && refreshed !== selectedLogJob) {
        selectedLogJob = refreshed;
      } else {
        if (!refreshed) {
          selectedLogJob = null;
          selectedStageLog = "";
          selectedStageLogSignature = "";
          stageLogStickToBottom = true;
          stageLogViewport = undefined;
        }
      }
    }
  }

  $: {
    competitorInsights = competitorReport
      ? buildCompetitorInsightsFromReport(competitorReport)
      : buildCompetitorInsights(competitorPack);
    selectedCompetitorInsightsList = competitorInsights
      .filter((row) => row.enabled)
      .sort((a, b) => b.weightedGap - a.weightedGap || b.gap - a.gap || b.weight - a.weight);
    competitorCurrentComposite = aggregateCompetitorScore(selectedCompetitorInsightsList, "currentScore");
    competitorBenchmarkComposite = aggregateCompetitorScore(selectedCompetitorInsightsList, "benchmarkScore");
    competitorRecommendedFocusKey = competitorReport?.recommended_focus ?? deriveCompetitorFocus(selectedCompetitorInsightsList);
    competitorRecommendedTweakKeys =
      competitorReport?.recommended_tweaks?.length
        ? competitorReport.recommended_tweaks
        : deriveCompetitorTweaks(selectedCompetitorInsightsList);
    competitorRecommendedTweakOptionsList = promptTweakOptions.filter((option) =>
      competitorRecommendedTweakKeys.includes(option.key),
    );
  }

  $: if (
    currentPage === "material" &&
    selectedMaterialJobId &&
    selectedMaterialJobId !== loadedMaterialJobId &&
    !loadingMaterialPack
  ) {
    void loadMaterialPack(selectedMaterialJobId);
  }

  $: if (
    currentPage === "competitor" &&
    selectedCompetitorJobId &&
    selectedCompetitorJobId !== loadedCompetitorJobId &&
    !loadingCompetitorPack
  ) {
    void loadCompetitorPack(selectedCompetitorJobId);
  }

  $: if (materialPack) {
    const signature = materialPromptSignature(materialPack);
    if (signature !== lastMaterialPromptSignature) {
      materialPromptText = buildFullMaterialPrompt(materialPack);
      if (manualMaterialDraft && manualMaterialAutoSyncPrompt) {
        manualMaterialDraft = {
          ...manualMaterialDraft,
          fullPrompt: cleanText(materialPromptText),
        };
      }
      lastMaterialPromptSignature = signature;
      materialPromptGeneratedByModel = "";
      materialPromptUsage = null;
      if (currentPage === "material") {
        queueMaterialPromptRewrite();
      }
    }
  }
</script>

<div class="app-shell">
  <header class="topbar">
    <div class="brand-row">
      <div class="brand-badge">MX</div>
      <div class="brand-copy">
        <h1>MicrocodeX 短视频素材工作台</h1>
        <p>Windows 部署版 · 独立项目</p>
      </div>
    </div>
    <div class="topbar-stats">
      <span class="chip"><strong>{dashboard.runningJobs}</strong> 处理中</span>
      <span class="chip"><strong>{dashboard.pendingJobs}</strong> 等待中</span>
      <span class="chip"><strong>{dashboard.finishedJobsToday}</strong> 今日完成</span>
      <span class="chip"><strong>{formatCurrency(dashboard.estimatedSpendTodayCny)}</strong> 今日预估</span>
      {#if estimate}
        <span class="chip"><strong>{formatCurrency(estimate.estimatedCostCny)}</strong> 当前任务预估</span>
      {/if}
    </div>
  </header>

  <aside class="sidebar">
    <div class="side-section-label">Workflows</div>
    <nav class="nav-list" aria-label="Main navigation">
      {#each pages as page}
        <button
          type="button"
          class:active={currentPage === page.key}
          class="nav-btn"
          aria-label={page.title}
          on:click={() => (currentPage = page.key)}
        >
          <span>{page.icon}</span>
          <span>
            <span class="nav-kicker">{page.kicker}</span>
            <span class="nav-title">{page.title}</span>
          </span>
        </button>
      {/each}
    </nav>
  </aside>

  <main class="content">
    {#if loadingBootstrap}
      <section class="page-grid">
        <div class="panel">
          <div class="panel-body">
            <div class="empty-state">正在加载工作台数据…</div>
          </div>
        </div>
      </section>
    {:else}
      {#if currentPage === "dashboard"}
        <section class="page-grid">
          <div class="page-header">
            <div>
              <h2>工作台首页</h2>
              <p>这里直接看队列、成本和模型路由，不把客户带进通用 Agent 控制台。</p>
            </div>
            <div class="actions">
              <button type="button" class="btn primary" on:click={() => (currentPage = "new-job")}>新建任务</button>
              <button type="button" class="btn" on:click={() => (currentPage = "queue")}>查看队列</button>
            </div>
          </div>

          {#if environmentReport}
            <div class={`notice ${environmentReport.overallStatus === "ok" ? "good" : "warn"}`}>
              {environmentHeadline()}
              <span class="mono"> · OK {environmentReport.okCount} / 警告 {environmentReport.warningCount} / 缺失 {environmentReport.missingCount}</span>
            </div>

            <div class="panel">
              <div class="panel-head">
                <h3>交付环境检查</h3>
                <div class="actions">
                  <button type="button" class="btn" on:click={openEnvironmentPanel}>打开详细检查</button>
                  <button type="button" class="btn" on:click={() => void runDouyinCookieLogin()}>抖音登录 / 获取 Cookie</button>
                  <button type="button" class="btn" on:click={() => void openEnvironmentSetupScript()}>打开引导脚本</button>
                  <button type="button" class="btn primary" on:click={() => void loadEnvironmentReport()} disabled={loadingEnvironmentReport}>
                    {loadingEnvironmentReport ? "检查中…" : "重新检查"}
                  </button>
                </div>
              </div>
              <div class="panel-body stack">
                <div class="field-grid">
                  {#each environmentReport.items.slice(0, 4) as item}
                    <div class="env-mini-card">
                      <div class="env-row">
                        <strong>{item.label}</strong>
                        <span class={`status ${item.status}`}>{item.status === "ok" ? "已就绪" : item.status === "warn" ? "需留意" : "缺失"}</span>
                      </div>
                      <div class="small muted">{item.detail}</div>
                    </div>
                  {/each}
                </div>
                <div class="small muted">引导脚本：<span class="mono">{environmentReport.helperScriptPath}</span></div>
              </div>
            </div>
          {/if}

          <div class="cards-4">
            <div class="panel stat-card">
              <div class="label">今日处理中任务</div>
              <div class="value">{dashboard.runningJobs}</div>
              <div class="sub">当前正在推进的作业</div>
            </div>
            <div class="panel stat-card">
              <div class="label">今日完成任务</div>
              <div class="value">{dashboard.finishedJobsToday}</div>
              <div class="sub">提炼 / 复盘 / 竞品合计</div>
            </div>
            <div class="panel stat-card">
              <div class="label">今日预估成本</div>
              <div class="value">{formatCurrency(dashboard.estimatedSpendTodayCny)}</div>
              <div class="sub">按当前价格快照统计</div>
            </div>
            <div class="panel stat-card">
              <div class="label">默认文本档位</div>
              <div class="value compact">
                {settings?.textProvider.defaultTier === "pro" ? "DeepSeek Pro" : "DeepSeek Flash"}
              </div>
              <div class="sub">任务级别可临时覆盖</div>
            </div>
          </div>

          <div class="grid-2">
            <div class="panel">
              <div class="panel-head">
                <h3>最近队列</h3>
                <button type="button" class="btn ghost" on:click={() => (currentPage = "queue")}>打开完整队列</button>
              </div>
              <div class="panel-body">
                <table class="table">
                  <thead>
                    <tr>
                      <th>任务</th>
                      <th>模式</th>
                      <th>阶段</th>
                      <th>状态</th>
                      <th>成本</th>
                    </tr>
                  </thead>
                  <tbody>
                    {#if recentJobs.length}
                      {#each recentJobs as job}
                        <tr>
                          <td>
                            <div class="queue-name">{job.name}</div>
                            <div class="small muted">{sourceKindLabel(job.sourceKind)}</div>
                          </td>
                          <td>{jobModeLabel(job.mode)}</td>
                          <td>{stageLabel(job.stageKey)}</td>
                          <td><span class={`status ${job.status}`}>{statusLabel(job.status)}</span></td>
                          <td>{formatCurrency(job.actualCostCny > 0 ? job.actualCostCny : job.estimatedCostCny)}</td>
                        </tr>
                      {/each}
                    {:else}
                      <tr>
                        <td colspan="5">
                          <div class="empty-state">还没有任务。先从“新建任务”页导入第一条素材。</div>
                        </td>
                      </tr>
                    {/if}
                  </tbody>
                </table>
              </div>
            </div>

            <div class="stack">
              <div class="panel">
                <div class="panel-head">
                  <h3>当前模型路由</h3>
                </div>
                <div class="panel-body">
                  <div class="kv"><div class="k">文本</div><div class="v">{routeSummary()}</div></div>
                  <div class="kv"><div class="k">视觉</div><div class="v">{settings?.visionProvider.model} · {settings?.visionProvider.baseUrl}</div></div>
                  <div class="kv"><div class="k">单任务预算</div><div class="v">{formatCurrency(settings?.budget.perJobCny ?? 0)}</div></div>
                  <div class="kv"><div class="k">批次预算</div><div class="v">{formatCurrency(settings?.budget.perBatchCny ?? 0)}</div></div>
                </div>
              </div>

              <div class="panel">
                <div class="panel-head">
                  <h3>队列焦点</h3>
                </div>
                <div class="panel-body stack">
                  {#if latestActiveJob}
                    <div class="notice good">
                      当前活跃任务：{latestActiveJob.name} · {stageLabel(latestActiveJob.stageKey)}
                    </div>
                  {:else}
                    <div class="notice">当前没有活跃任务，队列是空的。</div>
                  {/if}
                  {#if latestFinishedJob}
                    <div class="notice">
                      最近完成：{latestFinishedJob.name} · 完成时间 {formatDateTime(latestFinishedJob.finishedAtMs)}
                    </div>
                  {/if}
                  <div class="notice">
                    这套桌面端只做素材提炼、素材编辑、成片复盘和竞品分析。视频生成故意留给外部 App。
                  </div>
                </div>
              </div>
            </div>
          </div>
        </section>
      {/if}

      {#if currentPage === "new-job"}
        <section class="page-grid">
          <div class="page-header">
            <div>
              <h2>新建任务</h2>
              <p>先做成本预估，再决定是否启动。默认走 Flash，需要时再切 Pro。</p>
            </div>
            <div class="actions">
              <button type="button" class="btn" on:click={estimateJob} disabled={estimating}>
                {estimating ? "预估中…" : "刷新预估"}
              </button>
              <button type="button" class="btn primary" on:click={createJob} disabled={creatingJob || startWouldBeBlocked()}>
                {creatingJob ? "正在入队…" : "开始处理"}
              </button>
            </div>
          </div>

          {#if newJobMessage}
            <div class={`notice ${newJobTone}`}>{newJobMessage}</div>
          {/if}

          <div class="grid-2">
            <div class="panel">
              <div class="panel-head">
                <h3>任务输入</h3>
              </div>
              <div class="panel-body stack">
                <div class="field-grid">
                  <div class="field">
                    <label for="job-name">任务名称</label>
                    <input id="job-name" bind:value={draftJob.name} />
                  </div>
                  <div class="field">
                    <label for="job-mode">任务模式</label>
                    <select id="job-mode" bind:value={draftJob.mode}>
                      <option value="extract">素材提炼</option>
                      <option value="review">成片复盘</option>
                      <option value="competitor">竞品分析</option>
                    </select>
                  </div>
                </div>

                <div class="field">
                  <div class="field-title">来源类型</div>
                  <div class="segmented">
                    <button type="button" class:active={draftJob.sourceKind === "douyin_url"} on:click={() => (draftJob.sourceKind = "douyin_url")}>抖音链接</button>
                    <button type="button" class:active={draftJob.sourceKind === "local_video"} on:click={() => (draftJob.sourceKind = "local_video")}>本地视频</button>
                  </div>
                </div>

                {#if draftJob.sourceKind === "douyin_url"}
                  <div class="field">
                    <label for="job-source-url">抖音链接</label>
                    <textarea id="job-source-url" bind:value={draftJob.sourceValue} placeholder="粘贴 v.douyin.com/... 或 www.douyin.com/video/..."></textarea>
                  </div>
                {:else}
                  <div class="field">
                    <label for="job-source-file">本地视频路径</label>
                    <input id="job-source-file" bind:value={draftJob.sourceValue} placeholder="选择本地视频文件" />
                    <div>
                      <button type="button" class="btn" on:click={chooseLocalVideo}>选择视频</button>
                    </div>
                  </div>
                {/if}

                {#if draftJob.mode === "competitor"}
                  <div class="field">
                    <label for="job-competitor-sources">竞品样本</label>
                    <textarea
                      id="job-competitor-sources"
                      bind:value={draftCompetitorSourcesText}
                      placeholder="每行 1 条。可混合粘贴抖音链接或本地视频路径，系统会自动识别来源类型。"
                    ></textarea>
                    <div class="small muted">当前已录入 {effectiveCompetitorCount()} 条竞品样本。</div>
                  </div>
                {/if}

                <div class="field-grid-3">
                  <div class="field">
                    <label for="job-duration">预计时长（分钟）</label>
                    <input id="job-duration" type="number" min="1" bind:value={draftJob.durationMinutes} />
                  </div>
                  <div class="field">
                    <label for="job-frames">抽帧数</label>
                    <input id="job-frames" type="number" min="1" max={settings?.limits.maxFrames ?? 16} bind:value={draftJob.frameCount} />
                  </div>
                  <div class="field">
                    <label for="job-competitors">竞品数量</label>
                    <input
                      id="job-competitors"
                      type="number"
                      min="0"
                      max={settings?.limits.maxCompetitors ?? 5}
                      value={effectiveCompetitorCount()}
                      disabled={draftJob.mode === "competitor"}
                      on:input={(event) => (draftJob.competitorCount = Number((event.currentTarget as HTMLInputElement).value))}
                    />
                  </div>
                </div>

                <div class="field">
                  <div class="field-title">本次文本档位覆盖</div>
                  <div class="segmented">
                    <button type="button" class:active={draftJob.tierOverride === "flash"} on:click={() => (draftJob.tierOverride = "flash")}>Flash</button>
                    <button type="button" class:active={draftJob.tierOverride === "pro"} on:click={() => (draftJob.tierOverride = "pro")}>Pro</button>
                  </div>
                </div>

                <div class="notice">
                  当前任务路由：{routeSummary()}
                </div>
              </div>
            </div>

            <div class="panel">
              <div class="panel-head">
                <h3>成本预控制</h3>
              </div>
              <div class="panel-body stack">
                {#if estimate}
                  {#if estimateNotice()}
                    <div class={`notice ${estimateNotice()?.tone}`}>{estimateNotice()?.text}</div>
                  {/if}

                  {#if estimateStale}
                    <div class="notice warn">任务参数已经变化，当前预估已过期，建议先刷新预估再启动。</div>
                  {/if}

                  <div class="metric-grid">
                    <div class="metric-block">
                      <div class="metric-label">Prompt Tokens</div>
                      <div class="metric-value">{estimate.estimatedPromptTokens.toLocaleString("zh-CN")}</div>
                    </div>
                    <div class="metric-block">
                      <div class="metric-label">Completion Tokens</div>
                      <div class="metric-value">{estimate.estimatedCompletionTokens.toLocaleString("zh-CN")}</div>
                    </div>
                    <div class="metric-block">
                      <div class="metric-label">VL 帧数</div>
                      <div class="metric-value">{estimate.estimatedVlFrames}</div>
                    </div>
                    <div class="metric-block">
                      <div class="metric-label">预估成本</div>
                      <div class="metric-value">{formatCurrency(estimate.estimatedCostCny)}</div>
                    </div>
                  </div>

                  <div class="kv"><div class="k">文本模型</div><div class="v">{estimate.effectiveTextModel}</div></div>
                  <div class="kv"><div class="k">文本 URL</div><div class="v">{estimate.effectiveTextBaseUrl}</div></div>
                  <div class="kv"><div class="k">视觉模型</div><div class="v">{estimate.effectiveVisionModel}</div></div>
                  <div class="kv"><div class="k">VL 调用次数</div><div class="v">{estimate.estimatedVlCalls}</div></div>
                  <div class="kv"><div class="k">文本调用次数</div><div class="v">{estimate.estimatedTextCalls}</div></div>
                  <div class="kv"><div class="k">单任务预算</div><div class="v">{formatCurrency(settings?.budget.perJobCny ?? 0)}</div></div>
                  <div class="kv"><div class="k">当前活跃批次</div><div class="v">{formatCurrency(activeBatchCostCny())}</div></div>
                  <div class="kv"><div class="k">入队后批次预估</div><div class="v">{formatCurrency(projectedBatchCostCny() ?? 0)}</div></div>
                  <div class="kv"><div class="k">批次预算</div><div class="v">{formatCurrency(settings?.budget.perBatchCny ?? 0)}</div></div>

                  {#if estimate.notes.length}
                    <div class="notice">
                      {#each estimate.notes as note}
                        <div>{note}</div>
                      {/each}
                    </div>
                  {/if}
                {:else}
                  <div class="empty-state">还没有生成预估。先填写任务参数，再点击“刷新预估”。</div>
                {/if}
              </div>
            </div>
          </div>
        </section>
      {/if}

      {#if currentPage === "queue"}
        <section class="page-grid">
          <div class="page-header">
            <div>
              <h2>批量队列</h2>
              <p>这里已经接上真实后端队列。任务会写入本地 `jobs.json`，并把阶段日志和素材包落到任务目录里。</p>
            </div>
            <div class="actions">
              <button type="button" class="btn" on:click={() => void refreshQueueState(true)} disabled={refreshingQueue}>
                {refreshingQueue ? "刷新中…" : "刷新队列"}
              </button>
              <button type="button" class="btn primary" on:click={() => (currentPage = "new-job")}>继续新建</button>
            </div>
          </div>

          {#if queueMessage}
            <div class={`notice ${queueTone}`}>{queueMessage}</div>
          {/if}

          <div class="panel">
            <div class="panel-body">
              <table class="table">
                <thead>
                  <tr>
                    <th>任务</th>
                    <th>来源</th>
                    <th>模式</th>
                    <th>阶段</th>
                    <th>进度</th>
                    <th>状态</th>
                    <th>Token 预估</th>
                    <th>成本</th>
                    <th>操作</th>
                  </tr>
                </thead>
                <tbody>
                  {#if jobs.length}
                    {#each jobs as job}
                      <tr class:selected-log-row={selectedLogJob?.id === job.id}>
                        <td>
                          <div class="queue-name">{job.name}</div>
                          <div class="small muted mono">{job.id}</div>
                        </td>
                        <td>
                          <div>{sourceKindLabel(job.sourceKind)}</div>
                          <div class="small muted">{jobSourceSummary(job)}</div>
                          {#if job.mode === "competitor"}
                            <div class="small muted mono">{jobSourceDetail(job)}</div>
                          {/if}
                        </td>
                        <td>{jobModeLabel(job.mode)}</td>
                        <td>
                          <div>{stageLabel(job.stageKey)}</div>
                          <div class="small muted">{formatDateTime(job.updatedAtMs)}</div>
                        </td>
                        <td class="progress-cell">
                          <div class="progress-track">
                            <div class="progress-bar" style={`width: ${job.progress}%`}></div>
                          </div>
                          <div class="small muted">{job.progress}%</div>
                        </td>
                        <td><span class={`status ${job.status}`}>{statusLabel(job.status)}</span></td>
                        <td>
                          <div>{formatTokenCount(job.estimatedTotalTokens)}</div>
                          <div class="small muted">
                            Prompt {formatTokenCount(job.estimatedPromptTokens)} / Completion {formatTokenCount(job.estimatedCompletionTokens)}
                          </div>
                        </td>
                        <td>
                          <div>{formatCurrency(job.estimatedCostCny)}</div>
                          <div class="small muted">链路预估</div>
                        </td>
                        <td>
                          <div class="table-actions">
                            <button
                              type="button"
                              class="btn mini"
                              on:click={() => void openJobArtifactDir(job)}
                            >
                              打开目录
                            </button>
                            <button
                              type="button"
                              class="btn mini"
                              disabled={!job.materialPackPath}
                              on:click={() => void openJobMaterialPack(job)}
                            >
                              打开素材包
                            </button>
                            <button
                              type="button"
                              class="btn mini"
                              on:click={() => void showJobStageLog(job)}
                            >
                              {selectedLogJob?.id === job.id ? "收起日志" : "阶段日志"}
                            </button>
                            <button
                              type="button"
                              class="btn mini"
                              disabled={activeJobActionId === job.id || !(job.status === "waiting" || job.status === "running")}
                              on:click={() => void cancelExistingJob(job.id)}
                            >
                              取消
                            </button>
                            <button
                              type="button"
                              class="btn mini"
                              disabled={activeJobActionId === job.id || job.status === "running"}
                              on:click={() => void retryExistingJob(job.id)}
                            >
                              重试
                            </button>
                          </div>
                        </td>
                      </tr>
                    {/each}
                  {:else}
                    <tr>
                      <td colspan="9">
                        <div class="empty-state">当前队列为空。把第一条抖音链接或者本地视频丢进来就能开始跑。</div>
                      </td>
                    </tr>
                  {/if}
                </tbody>
              </table>
            </div>
          </div>

          {#if selectedLogJob}
            <div class="panel">
              <div class="panel-head">
                <h3>阶段日志</h3>
                <div class="panel-head-meta">
                  <div class="log-context">
                    <span class="small muted">当前查看任务</span>
                    <strong>{selectedLogJob.name}</strong>
                    <span class={`status ${selectedLogJob.status}`}>{statusLabel(selectedLogJob.status)}</span>
                    <span class="small muted mono">{selectedLogJob.id}</span>
                  </div>
                  <div class="small muted">
                    {refreshingStageLog
                      ? "Auto-refreshing..."
                      : stageLogStickToBottom
                        ? "Following latest line"
                        : "Auto-follow paused while you scroll"}
                  </div>
                  {#if !loadingStageLog && !stageLogStickToBottom && (selectedStageLog.trim() || selectedLogJob.notes.length)}
                    <div class="panel-head-actions">
                      <button
                        type="button"
                        class="btn mini"
                        on:click={() => void jumpStageLogToLatest()}
                      >
                        Jump to latest
                      </button>
                    </div>
                  {/if}
                </div>
              </div>
              <div class="panel-body stack">
                <div class="kv"><div class="k">任务目录</div><div class="v mono">{selectedLogJob.artifactDir}</div></div>
                <div class="kv"><div class="k">日志文件</div><div class="v mono">{selectedLogJob.stageLogPath}</div></div>
                <div class="kv"><div class="k">素材包</div><div class="v mono">{selectedLogJob.materialPackPath ?? "尚未生成"}</div></div>
                {#if selectedLogJob.error}
                  <div class="notice warn">{selectedLogJob.error}</div>
                {/if}
                {#if loadingStageLog}
                  <div class="empty-state compact">正在读取阶段日志…</div>
                {:else if selectedStageLog.trim()}
                  <div
                    bind:this={stageLogViewport}
                    class="code-block log-block"
                    on:scroll={handleStageLogScroll}
                  >
                    {selectedStageLog}
                  </div>
                {:else if selectedLogJob.notes.length}
                  <div
                    bind:this={stageLogViewport}
                    class="code-block log-block"
                    on:scroll={handleStageLogScroll}
                  >
                    {selectedLogJob.notes.join("\n")}
                  </div>
                {:else}
                  <div class="empty-state compact">这个任务还没有阶段日志。</div>
                {/if}
              </div>
            </div>
          {/if}
        </section>
      {/if}

      {#if currentPage === "material"}
        <section class="page-grid">
          <div class="page-header">
            <div>
              <h2>素材包编辑</h2>
              <p>这里直接读取真实素材包，并基于提炼结果生成可直接喂给外部视频工具的完整提示词。</p>
            </div>
            {#if materialJobs.length}
              <div class="actions material-header-actions">
                <select class="material-select" bind:value={selectedMaterialJobId}>
                  {#each materialJobs as job}
                    <option value={job.id}>{job.name}</option>
                  {/each}
                </select>
                <button
                  type="button"
                  class="btn"
                  disabled={!selectedMaterialJob}
                  on:click={() => selectedMaterialJob && void openJobArtifactDir(selectedMaterialJob)}
                >
                  打开任务目录
                </button>
                <button
                  type="button"
                  class="btn"
                  disabled={!selectedMaterialJob}
                  on:click={() => selectedMaterialJob && void openJobMaterialPack(selectedMaterialJob)}
                >
                  打开素材包
                </button>
                <button
                  type="button"
                  class="btn primary"
                  disabled={!materialPack || materialPromptRegenerating}
                  on:click={() => void regenerateMaterialPrompt(true)}
                >
                  {materialPromptRegenerating
                    ? "正在重写提示词…"
                    : materialPromptText
                      ? "重新生成完整提示词"
                      : "生成完整提示词"}
                </button>
              </div>
            {/if}
          </div>

          {#if selectedMaterialJob}
            <div class="notice good">当前载入素材包：{selectedMaterialJob.name}。你可以直接查看结构化结果，并生成完整提示词。</div>
          {:else if latestFinishedJob}
            <div class="notice good">最近完成的任务是“{latestFinishedJob.name}”，等素材包生成后会自动映射到这里。</div>
          {/if}

          {#if materialMessage}
            <div class={`notice ${materialTone}`}>{materialMessage}</div>
          {/if}

          <div class="panel">
            <div class="panel-head">
              <h3>提示词控制</h3>
              <div class="small muted">按平台、版本和质量目标切换完整提示词</div>
            </div>
            <div class="panel-body stack">
              <div class="prompt-control-grid">
                <div class="field">
                  <div class="field-title">目标平台</div>
                  <div class="segmented wrap">
                    {#each promptPlatformOptions as option}
                      <button
                        type="button"
                        class:active={promptPlatform === option.key}
                        on:click={() => (promptPlatform = option.key)}
                      >
                        {option.label}
                      </button>
                    {/each}
                  </div>
                  <div class="small muted">{currentPromptPlatformOption().summary}</div>
                </div>

                <div class="field">
                  <div class="field-title">提示词版本</div>
                  <div class="segmented wrap">
                    {#each promptVersionOptions as option}
                      <button
                        type="button"
                        class:active={promptVersion === option.key}
                        on:click={() => (promptVersion = option.key)}
                      >
                        {option.label}
                      </button>
                    {/each}
                  </div>
                  <div class="small muted">{currentPromptVersionOption().summary}</div>
                </div>

                <div class="field">
                  <div class="field-title">优化目标</div>
                  <div class="segmented wrap">
                    {#each promptFocusOptions as option}
                      <button
                        type="button"
                        class:active={promptFocus === option.key}
                        on:click={() => (promptFocus = option.key)}
                      >
                        {option.label}
                      </button>
                    {/each}
                  </div>
                  <div class="small muted">{currentPromptFocusOption().summary}</div>
                </div>
              </div>

              <div class="field">
                <div class="field-title">引导式调优</div>
                <div class="tweak-chip-group">
                  {#each promptTweakOptions as option}
                    <button
                      type="button"
                      class:active={activePromptTweaks.includes(option.key)}
                      class="toggle-chip"
                      on:click={() => togglePromptTweak(option.key)}
                    >
                      <span>{option.label}</span>
                      <span class="small muted">{option.summary}</span>
                    </button>
                  {/each}
                </div>
              </div>

              <div class="prompt-guidance">
                <div class="section-label">当前平台引导</div>
                <div class="guide-line">{currentPromptPlatformOption().instruction}</div>
                <div class="section-label">当前版本引导</div>
                <div class="guide-line">{currentPromptVersionOption().summary}</div>
                <div class="section-label">当前优化目标</div>
                <div class="guide-line">{currentPromptFocusOption().instruction}</div>
                {#if activePromptTweakOptions().length}
                  <div class="section-label">调优建议</div>
                  <div class="guide-list">
                    {#each activePromptTweakOptions() as option}
                      <div class="guide-item">
                        <strong>{option.label}</strong>
                        <div>{option.guide}</div>
                        <div class="small muted mono">{option.promptLine}</div>
                      </div>
                    {/each}
                  </div>
                {:else}
                  <div class="section-label">调优建议</div>
                  <div class="guide-line">先选一个你想强化的方向，比如“背景更生动”或“口型同步优先”，下方完整提示词会跟着自动调整。</div>
                {/if}
              </div>
            </div>
          </div>

          <div class="panel">
            <div class="panel-head">
              <h3>素材包标签</h3>
              <div class="editor-tabs">
                {#each materialTabs as tab}
                  <button
                    type="button"
                    class:active={materialTab === tab.key}
                    class="editor-tab"
                    on:click={() => (materialTab = tab.key)}
                  >
                    {tab.label}
                  </button>
                {/each}
              </div>
            </div>
            <div class="panel-body">
              {#if !materialJobs.length}
                <div class="empty-state">还没有可用的素材包。先完成至少一个“素材提炼”任务。</div>
              {:else if loadingMaterialPack}
                <div class="empty-state">正在读取真实素材包…</div>
              {:else if materialPack}
                {#if materialTab === "prompt"}
                  <div class="stack">
                    <div class="kv"><div class="k">来源任务</div><div class="v">{selectedMaterialJob?.name ?? "未选择"}</div></div>
                    <div class="kv"><div class="k">当前主题</div><div class="v">{materialPack.topic}</div></div>
                    <div class="kv"><div class="k">目标平台</div><div class="v">{currentPromptPlatformOption().label}</div></div>
                    <div class="kv"><div class="k">提示词版本</div><div class="v">{currentPromptVersionOption().label}</div></div>
                    <div class="kv"><div class="k">优化目标</div><div class="v">{currentPromptFocusOption().label}</div></div>
                    <div class="kv"><div class="k">已启用调优</div><div class="v">{activePromptTweakOptions().length ? activePromptTweakOptions().map((option) => option.label).join(" / ") : "未启用"}</div></div>
                    <div class="kv"><div class="k">生成方式</div><div class="v">{materialPromptGeneratedByModel ? `AI 重写 · ${materialPromptGeneratedByModel}` : "本地模板草稿"}</div></div>
                    <div class="kv"><div class="k">提示词状态</div><div class="v">{materialPromptRegenerating ? "正在重写" : "已就绪"}</div></div>
                    {#if materialPromptUsage?.totalTokens}
                      <div class="kv"><div class="k">本次用量</div><div class="v">Prompt {materialPromptUsage.promptTokens} / Completion {materialPromptUsage.completionTokens} / Total {materialPromptUsage.totalTokens}</div></div>
                      <div class="kv"><div class="k">本次成本</div><div class="v">{formatCurrency(materialPromptUsage.costCny)}</div></div>
                    {/if}
                    <div class="section-label">完整提示词</div>
                    <div class="code-block prompt-block">{materialPromptText}</div>
                    <div class="section-label">素材包内置草稿</div>
                    <div class="code-block prompt-block">{buildPromptDraftBlock(materialPack)}</div>
                    {#if manualMaterialDraft}
                      <div class="section-label">人工改稿 / 新视频素材草稿</div>
                      <div class="manual-draft-surface stack">
                        <div class="manual-draft-toolbar">
                          <div class="small muted">
                            这里可以把当前提示词复刻出来，人工修改后保存成新视频素材草稿。
                          </div>
                          <div class="table-actions">
                            <button type="button" class="btn" on:click={() => cloneManualMaterialDraftFromCurrentPack()}>
                              从当前素材包复刻
                            </button>
                            <button type="button" class="btn" on:click={() => syncManualMaterialPromptFromCurrent()}>
                              带入当前完整提示词
                            </button>
                            <button
                              type="button"
                              class="btn primary"
                              on:click={() => void saveManualMaterialDraft()}
                              disabled={manualMaterialSaving}
                            >
                              {manualMaterialSaving ? "保存中..." : "保存新素材草稿"}
                            </button>
                          </div>
                        </div>

                        <div class="manual-sync-row">
                          <span class={`status ${manualMaterialAutoSyncPrompt ? "ok" : "warn"}`}>
                            {manualMaterialAutoSyncPrompt ? "提示词跟随当前版本" : "提示词已切到手工改稿"}
                          </span>
                          <div class="small muted">
                            {manualMaterialAutoSyncPrompt
                              ? "上方平台 / 版本 / 调优切换后，这里的完整提示词会自动跟随。"
                              : "你已经手工改过完整提示词；需要重新同步时，点“带入当前完整提示词”。"}
                          </div>
                        </div>

                        <div class="manual-draft-grid">
                          <div class="stack">
                            <div class="field-grid">
                              <div class="field">
                                <label for="manual-topic">新视频主题</label>
                                <input id="manual-topic" bind:value={manualMaterialDraft.topic} />
                              </div>
                              <div class="field">
                                <label for="manual-audience">目标受众</label>
                                <input id="manual-audience" bind:value={manualMaterialDraft.audience} />
                              </div>
                            </div>

                            <div class="field-grid">
                              <div class="field">
                                <label for="manual-persona">人物设定</label>
                                <input id="manual-persona" bind:value={manualMaterialDraft.persona} />
                              </div>
                              <div class="field">
                                <label for="manual-tone">表达气质</label>
                                <input id="manual-tone" bind:value={manualMaterialDraft.tone} />
                              </div>
                            </div>

                            <div class="field">
                              <label for="manual-full-prompt">完整提示词（可直接复制去生成）</label>
                              <textarea
                                id="manual-full-prompt"
                                class="prompt-editor-input"
                                bind:value={manualMaterialDraft.fullPrompt}
                                on:input={() => (manualMaterialAutoSyncPrompt = false)}
                              ></textarea>
                            </div>

                            <div class="field">
                              <label for="manual-hook">开场钩子</label>
                              <textarea id="manual-hook" bind:value={manualMaterialDraft.hook}></textarea>
                            </div>

                            <div class="field">
                              <label for="manual-body">正文要点（每行 1 条）</label>
                              <textarea id="manual-body" class="tall-textarea" bind:value={manualMaterialDraft.bodyText}></textarea>
                            </div>

                            <div class="field">
                              <label for="manual-ending">收尾</label>
                              <textarea id="manual-ending" bind:value={manualMaterialDraft.ending}></textarea>
                            </div>

                            <div class="field">
                              <label for="manual-visual">visual_brief</label>
                              <textarea id="manual-visual" class="tall-textarea" bind:value={manualMaterialDraft.visualBrief}></textarea>
                            </div>

                            <div class="field">
                              <label for="manual-spoken">spoken_brief</label>
                              <textarea id="manual-spoken" bind:value={manualMaterialDraft.spokenBrief}></textarea>
                            </div>

                            <div class="field">
                              <label for="manual-reusable">reusable_prompt</label>
                              <textarea id="manual-reusable" class="tall-textarea" bind:value={manualMaterialDraft.reusablePrompt}></textarea>
                            </div>

                            <div class="field">
                              <label for="manual-titles">标题候选（每行 1 条）</label>
                              <textarea id="manual-titles" bind:value={manualMaterialDraft.titleCandidatesText}></textarea>
                            </div>

                            <div class="field">
                              <label for="manual-covers">封面文案（每行 1 条）</label>
                              <textarea id="manual-covers" bind:value={manualMaterialDraft.coverCopyCandidatesText}></textarea>
                            </div>

                            <div class="field">
                              <label for="manual-promo">宣传短句（每行 1 条）</label>
                              <textarea id="manual-promo" bind:value={manualMaterialDraft.promoCopyText}></textarea>
                            </div>
                          </div>

                          <div class="stack">
                            <div class="manual-preview-meta">
                              <div class="kv"><div class="k">目标平台</div><div class="v">{currentPromptPlatformOption().label}</div></div>
                              <div class="kv"><div class="k">提示词版本</div><div class="v">{currentPromptVersionOption().label}</div></div>
                              <div class="kv"><div class="k">优化目标</div><div class="v">{currentPromptFocusOption().label}</div></div>
                              <div class="kv"><div class="k">调优项</div><div class="v">{activePromptTweakOptions().length ? activePromptTweakOptions().map((option) => option.label).join(" / ") : "未启用"}</div></div>
                              <div class="kv"><div class="k">来源任务</div><div class="v">{selectedMaterialJob?.name ?? "未选择"}</div></div>
                            </div>

                            <div class="section-label">新素材预览</div>
                            <div class="code-block prompt-block">{buildManualMaterialPreview(manualMaterialDraft)}</div>

                            {#if manualMaterialLastSaveResult}
                              <div class="guide-list">
                                <div class="guide-item">
                                  <strong>已落盘文件</strong>
                                  <div class="small muted mono">{manualMaterialLastSaveResult.draftPath}</div>
                                  <div class="small muted mono">{manualMaterialLastSaveResult.promptPath}</div>
                                  <div class="small muted mono">{manualMaterialLastSaveResult.markdownPath}</div>
                                </div>
                              </div>
                            {/if}
                          </div>
                        </div>
                      </div>
                    {/if}
                  </div>
                {:else}
                  <div class="code-block prompt-block">{buildMaterialTabContent(materialTab, materialPack)}</div>
                {/if}
              {:else}
                <div class="empty-state">当前任务的素材包还没有准备好。</div>
              {/if}
            </div>
          </div>
        </section>
      {/if}

      {#if currentPage === "review"}
        <section class="page-grid">
          <div class="page-header">
            <div>
              <h2>成片复盘</h2>
              <p>这页后续接上传成片、抽帧分析和问题导出。现在先保留分析维度和最终呈现风格。</p>
            </div>
          </div>

          <div class="cards-3">
            <div class="panel stat-card"><div class="label">钩子强度</div><div class="value">6.0</div><div class="sub">前三秒冲突感偏弱</div></div>
            <div class="panel stat-card"><div class="label">字幕可读性</div><div class="value">7.0</div><div class="sub">单屏字数仍可再压</div></div>
            <div class="panel stat-card"><div class="label">平台适配度</div><div class="value">7.5</div><div class="sub">信息密度还能更靠前</div></div>
          </div>

          <div class="panel">
            <div class="panel-body stack">
              <div class="notice warn">首屏冲突句还不够锋利，难在 2-3 秒内形成明显问题感。</div>
              <div class="notice">建议把“少吃也会胖”提前到第一句，字幕每屏压到 10-14 字以内。</div>
            </div>
          </div>
        </section>
      {/if}

      {#if currentPage === "competitor"}
        <section class="page-grid">
          <div class="page-header">
            <div>
              <h2>竞品分析</h2>
              <p>左侧勾选指标和权重，中间看当前视频 vs 竞品基准，右侧直接输出改写建议和提示词优化项。</p>
            </div>
            {#if materialJobs.length}
              <div class="actions material-header-actions">
                <select class="material-select" bind:value={selectedCompetitorJobId}>
                  {#each competitorSelectableJobs as job}
                    <option value={job.id}>{job.name}</option>
                  {/each}
                </select>
                <button
                  type="button"
                  class="btn"
                  disabled={!selectedCompetitorJob}
                  on:click={() => selectedCompetitorJob && void openJobArtifactDir(selectedCompetitorJob)}
                >
                  打开任务目录
                </button>
                <button
                  type="button"
                  class="btn"
                  disabled={!selectedCompetitorJob}
                  on:click={() => selectedCompetitorJob && void openJobMaterialPack(selectedCompetitorJob)}
                >
                  打开素材包
                </button>
              </div>
            {/if}
          </div>

          <div class="panel">
            <div class="panel-head">
              <h3>真实竞品样本输入</h3>
              <div class="panel-head-actions">
                <button type="button" class="btn" on:click={estimateCompetitorJob} disabled={estimating}>
                  {estimating ? "预估中…" : "刷新预估"}
                </button>
                <button
                  type="button"
                  class="btn primary"
                  on:click={createCompetitorJob}
                  disabled={creatingJob || startWouldBeBlocked()}
                >
                  {creatingJob ? "正在入队…" : "启动真实竞品分析"}
                </button>
              </div>
            </div>
            <div class="panel-body stack">
              {#if newJobMessage}
                <div class={`notice ${newJobTone}`}>{newJobMessage}</div>
              {/if}
              {#if estimateStale && estimate}
                <div class="notice warn">竞品输入已经变化，当前成本预估已过期，建议先刷新预估再启动。</div>
              {/if}
              <div class="field-grid">
                <div class="field">
                  <label for="competitor-job-name">任务名称</label>
                  <input id="competitor-job-name" bind:value={draftJob.name} />
                </div>
                <div class="field">
                  <div class="field-title">当前视频来源</div>
                  <div class="segmented">
                    <button
                      type="button"
                      class:active={draftJob.sourceKind === "douyin_url"}
                      on:click={() => (draftJob.sourceKind = "douyin_url")}
                    >
                      抖音链接
                    </button>
                    <button
                      type="button"
                      class:active={draftJob.sourceKind === "local_video"}
                      on:click={() => (draftJob.sourceKind = "local_video")}
                    >
                      本地视频
                    </button>
                  </div>
                </div>
              </div>

              {#if draftJob.sourceKind === "douyin_url"}
                <div class="field">
                  <label for="competitor-primary-url">当前视频链接</label>
                  <textarea
                    id="competitor-primary-url"
                    bind:value={draftJob.sourceValue}
                    placeholder="粘贴当前要对比的抖音链接"
                  ></textarea>
                </div>
              {:else}
                <div class="field">
                  <label for="competitor-primary-file">当前视频路径</label>
                  <input
                    id="competitor-primary-file"
                    bind:value={draftJob.sourceValue}
                    placeholder="选择当前要对比的本地视频"
                  />
                  <div>
                    <button type="button" class="btn" on:click={chooseLocalVideo}>选择视频</button>
                  </div>
                </div>
              {/if}

              <div class="field">
                <label for="competitor-page-sources">竞品样本</label>
                <textarea
                  id="competitor-page-sources"
                  bind:value={draftCompetitorSourcesText}
                  placeholder="每行 1 条，可混合抖音链接和本地视频路径。"
                ></textarea>
                <div class="small muted">当前已录入 {effectiveCompetitorCount()} 条竞品样本，后端会逐条真实跑提炼链路。</div>
              </div>

              <div class="field-grid-3">
                <div class="field">
                  <label for="competitor-duration">预计时长（分钟）</label>
                  <input id="competitor-duration" type="number" min="1" bind:value={draftJob.durationMinutes} />
                </div>
                <div class="field">
                  <label for="competitor-frames">抽帧数</label>
                  <input
                    id="competitor-frames"
                    type="number"
                    min="1"
                    max={settings?.limits.maxFrames ?? 16}
                    bind:value={draftJob.frameCount}
                  />
                </div>
                <div class="field">
                  <label for="competitor-count-display">竞品数量</label>
                  <input id="competitor-count-display" type="number" value={effectiveCompetitorCount()} disabled />
                </div>
              </div>

              <div class="notice">当前任务路由：{routeSummary()}</div>
            </div>
          </div>

          {#if !materialJobs.length}
            <div class="panel">
              <div class="empty-state competitor-empty">先完成至少一个“素材提炼”任务，这里才能对真实素材包做竞品分析。</div>
            </div>
          {:else}
            <div class="notice">
              {competitorJobs.length
                ? "已检测到真实竞品任务。当前页会优先读取后端生成的 competitor_report；没有报告的旧任务会回退到启发式分析。"
                : "还没有完成的真实竞品任务。当前页会先用素材包做启发式分析，等真实竞品任务跑完后会自动切到后端报告。"}
            </div>

            {#if competitorMessage}
              <div class={`notice ${competitorTone}`}>{competitorMessage}</div>
            {/if}

            {#if competitorReport?.generated_by_model}
              <div class="notice good">
                报告生成模型：{competitorReport.generated_by_model}
                {#if competitorReport.llm_usage?.total_tokens}
                  · Prompt {competitorReport.llm_usage.prompt_tokens} / Completion {competitorReport.llm_usage.completion_tokens} / Total {competitorReport.llm_usage.total_tokens}
                {/if}
              </div>
            {/if}

            {#if loadingCompetitorPack}
              <div class="panel">
                <div class="empty-state competitor-empty">正在读取当前视频的素材包并生成竞品对比...</div>
              </div>
            {:else if competitorPack}
              <div class="competitor-layout">
                <div class="stack">
                  <div class="panel">
                    <div class="panel-head">
                      <h3>分析指标</h3>
                      <div class="small muted">勾选纳入本轮报告的维度，并用权重表达优先级。</div>
                    </div>
                    <div class="panel-body">
                      <div class="metric-config-list">
                        {#each competitorMetricOptions as option}
                          <div class="metric-config-card">
                            <div class="metric-config-head">
                              <label class="metric-config-title">
                                <input
                                  class="metric-checkbox"
                                  type="checkbox"
                                  checked={activeCompetitorMetrics.includes(option.key)}
                                  on:change={() => toggleCompetitorMetric(option.key)}
                                />
                                <span class="metric-config-copy">
                                  <strong>{option.label}</strong>
                                  <span class="metric-config-summary">{option.summary}</span>
                                </span>
                              </label>
                              <span class:quiet={!activeCompetitorMetrics.includes(option.key)} class="gap-pill">
                                {activeCompetitorMetrics.includes(option.key) ? "已纳入" : "已忽略"}
                              </span>
                            </div>
                            <div class="metric-weight-row">
                              <span class="small muted">权重</span>
                              <input
                                type="range"
                                min="1"
                                max="5"
                                step="1"
                                value={competitorMetricWeights[option.key]}
                                disabled={!activeCompetitorMetrics.includes(option.key)}
                                on:input={(event) =>
                                  setCompetitorMetricWeight(
                                    option.key,
                                    Number((event.currentTarget as HTMLInputElement).value),
                                  )}
                              />
                              <span class="metric-weight-value">x{competitorMetricWeights[option.key]}</span>
                            </div>
                          </div>
                        {/each}
                      </div>
                    </div>
                  </div>
                </div>

                <div class="stack">
                  <div class="competitor-summary-grid">
                    <div class="panel stat-card">
                      <div class="label">当前综合分</div>
                      <div class="value">{competitorCurrentComposite.toFixed(1)}</div>
                      <div class="sub">{selectedCompetitorJob?.name ?? "未选择任务"}</div>
                    </div>
                    <div class="panel stat-card">
                      <div class="label">竞品基准</div>
                      <div class="value">{competitorBenchmarkComposite.toFixed(1)}</div>
                      <div class="sub">按已选指标和权重生成</div>
                    </div>
                    <div class="panel stat-card">
                      <div class="label">最大优先项</div>
                      <div class="value compact">{selectedCompetitorInsightsList[0]?.label ?? "未选择指标"}</div>
                      <div class="sub">
                        {selectedCompetitorInsightsList[0]
                          ? `加权差距 ${selectedCompetitorInsightsList[0].weightedGap.toFixed(1)}`
                          : "至少勾选 1 个指标"}
                      </div>
                    </div>
                  </div>

                  <div class="panel">
                    <div class="panel-head">
                      <h3>当前视频 vs 竞品基准</h3>
                      <div class="small muted">按加权差距排序，越靠前越值得先改。</div>
                    </div>
                    <div class="panel-body">
                      {#if selectedCompetitorInsightsList.length}
                        <table class="table">
                          <thead>
                            <tr>
                              <th>维度</th>
                              <th>权重</th>
                              <th>当前视频</th>
                              <th>竞品基准</th>
                              <th>差距</th>
                              <th>建议动作</th>
                            </tr>
                          </thead>
                          <tbody>
                            {#each selectedCompetitorInsightsList as insight}
                              <tr>
                                <td>
                                  <strong>{insight.label}</strong>
                                  <div class="small muted">{insight.summary}</div>
                                </td>
                                <td>x{insight.weight}</td>
                                <td>
                                  <div class="score-cell">
                                    <div class="score-line">
                                      <div class="score-track">
                                        <div class="score-fill current" style={`width: ${insight.currentScore * 10}%`}></div>
                                      </div>
                                      <span class="score-value">{insight.currentScore.toFixed(1)}</span>
                                    </div>
                                    <div class="small muted">{insight.currentNote}</div>
                                  </div>
                                </td>
                                <td>
                                  <div class="score-cell">
                                    <div class="score-line">
                                      <div class="score-track">
                                        <div
                                          class="score-fill benchmark"
                                          style={`width: ${insight.benchmarkScore * 10}%`}
                                        ></div>
                                      </div>
                                      <span class="score-value">{insight.benchmarkScore.toFixed(1)}</span>
                                    </div>
                                    <div class="small muted">{insight.benchmarkNote}</div>
                                  </div>
                                </td>
                                <td>
                                  <span class:quiet={insight.gap < 1} class="gap-pill">+{insight.gap.toFixed(1)}</span>
                                  <div class="small muted">加权 {insight.weightedGap.toFixed(1)}</div>
                                </td>
                                <td>{insight.action}</td>
                              </tr>
                            {/each}
                          </tbody>
                        </table>
                      {:else}
                        <div class="empty-state compact">至少勾选 1 个指标，这里才会生成对比结果。</div>
                      {/if}
                    </div>
                  </div>
                </div>

                <div class="stack">
                  <div class="panel">
                    <div class="panel-head">
                      <h3>结论</h3>
                      <div class="small muted">根据已选指标自动汇总</div>
                    </div>
                    <div class="panel-body stack">
                      {#if selectedCompetitorInsightsList.length}
                        <div class="notice warn">
                          当前综合分 {competitorCurrentComposite.toFixed(1)}，距离竞品基准
                          {competitorBenchmarkComposite.toFixed(1)} 还差
                          {(competitorBenchmarkComposite - competitorCurrentComposite).toFixed(1)} 分。
                        </div>
                        {#if competitorReport?.top_findings?.length}
                          <div class="guide-list">
                            {#each competitorReport.top_findings as finding}
                              <div class="guide-item">{finding}</div>
                            {/each}
                          </div>
                        {/if}
                        <div class="guide-list">
                          <div class="guide-item">
                            <strong>最优先处理</strong>
                            <div>
                              {selectedCompetitorInsightsList[0].label}：{selectedCompetitorInsightsList[0].action}
                            </div>
                          </div>
                          {#if selectedCompetitorInsightsList[1]}
                            <div class="guide-item">
                              <strong>第二顺位</strong>
                              <div>
                                {selectedCompetitorInsightsList[1].label}：
                                {selectedCompetitorInsightsList[1].action}
                              </div>
                            </div>
                          {/if}
                          <div class="guide-item">
                            <strong>建议的提示词优化目标</strong>
                            <div>{promptFocusLabel(competitorRecommendedFocusKey)}</div>
                          </div>
                        </div>
                      {:else}
                        <div class="empty-state compact">先选分析指标，结论才会出现。</div>
                      {/if}
                    </div>
                  </div>

                  <div class="panel">
                    <div class="panel-head">
                      <h3>改写建议</h3>
                      <div class="small muted">直接给到可执行的调整方向</div>
                    </div>
                    <div class="panel-body">
                      {#if selectedCompetitorInsightsList.length}
                        <div class="rewrite-list">
                          {#each selectedCompetitorInsightsList.slice(0, 3) as insight}
                            <div class="rewrite-item">
                              <strong>{insight.label}</strong>
                              <div>{insight.rewriteHint}</div>
                            </div>
                          {/each}
                        </div>
                      {:else}
                        <div class="empty-state compact">没有已选指标时，暂时没有改写建议。</div>
                      {/if}
                    </div>
                  </div>

                  <div class="panel">
                    <div class="panel-head">
                      <h3>提示词优化项</h3>
                      <div class="small muted">可以直接同步到素材包编辑器</div>
                    </div>
                    <div class="panel-body stack">
                      <div class="kv">
                        <div class="k">推荐优化目标</div>
                        <div class="v">{promptFocusLabel(competitorRecommendedFocusKey)}</div>
                      </div>
                      <div class="section-label">建议启用的调优项</div>
                      {#if competitorRecommendedTweakOptionsList.length}
                        <div class="tweak-pill-group">
                          {#each competitorRecommendedTweakOptionsList as option}
                            <span class="tweak-pill">{option.label}</span>
                          {/each}
                        </div>
                        <div class="guide-list">
                          {#each competitorRecommendedTweakOptionsList as option}
                            <div class="guide-item">
                              <strong>{option.label}</strong>
                              <div>{option.guide}</div>
                              <div class="small muted mono">{option.promptLine}</div>
                            </div>
                          {/each}
                        </div>
                      {:else}
                        <div class="guide-line">当前这组选项更偏结构和节奏优化，可以保持现有调优项不动。</div>
                      {/if}
                      <div class="footer-actions">
                        <button
                          type="button"
                          class="btn primary"
                          disabled={!competitorPack || !selectedCompetitorInsightsList.length}
                          on:click={applyCompetitorRecommendations}
                        >
                          同步并打开提示词编辑器
                        </button>
                      </div>
                    </div>
                  </div>
                </div>
              </div>
            {:else}
              <div class="panel">
                <div class="empty-state competitor-empty">当前任务的素材包还没有准备好，暂时无法生成竞品分析。</div>
              </div>
            {/if}
          {/if}
        </section>
      {/if}

      {#if currentPage === "settings" && settingsDraft && settings}
        <section class="page-grid">
          <div class="page-header">
            <div>
              <h2>配置中心</h2>
              <p>这里只保留部署给客户需要的业务配置，不暴露通用 Coding Agent 的内部控制项。</p>
            </div>
            <div class="actions">
              <button type="button" class="btn" on:click={openSettingsDir}>打开配置目录</button>
              <button type="button" class="btn primary" on:click={saveSettings} disabled={savingSettings}>
                {savingSettings ? "保存中…" : "保存设置"}
              </button>
            </div>
          </div>

          <div class="notice">
            配置文件：<span class="mono">{settings.settingsPath}</span> · Schema v{settings.schemaVersion} · 最近更新 {formatDateTime(settings.updatedAtMs)}
          </div>

          {#if settingsMessage}
            <div class={`notice ${settingsTone}`}>{settingsMessage}</div>
          {/if}

          <div class="panel">
            <div class="panel-head">
              <h3>部署环境自检</h3>
              <div class="actions">
                <button type="button" class="btn" on:click={() => void runDouyinCookieLogin()}>抖音登录 / 获取 Cookie</button>
                <button type="button" class="btn" on:click={() => void openEnvironmentSetupScript()}>打开引导脚本</button>
                <button type="button" class="btn primary" on:click={() => void loadEnvironmentReport()} disabled={loadingEnvironmentReport}>
                  {loadingEnvironmentReport ? "检查中…" : "重新检查"}
                </button>
              </div>
            </div>
            <div class="panel-body stack">
              {#if environmentReport}
                <div class={`notice ${environmentReport.overallStatus === "ok" ? "good" : "warn"}`}>
                  {environmentHeadline()}
                </div>
                <div class="kv"><div class="k">引导脚本</div><div class="v mono">{environmentReport.helperScriptPath}</div></div>
                <div class="environment-list">
                  {#each environmentReport.items as item}
                    <div class="environment-item">
                      <div class="env-row">
                        <strong>{item.label}</strong>
                        <span class={`status ${item.status}`}>{item.status === "ok" ? "已就绪" : item.status === "warn" ? "需留意" : "缺失"}</span>
                      </div>
                      <div>{item.detail}</div>
                      <div class="small muted">{item.actionHint}</div>
                    </div>
                  {/each}
                </div>
              {:else}
                <div class="empty-state compact">环境检查结果还没加载出来。</div>
              {/if}
            </div>
          </div>

          <div class="grid-2">
            <div class="stack">
              <div class="panel">
                <div class="panel-head"><h3>文本模型设置</h3></div>
                <div class="panel-body stack">
                  <div class="field">
                    <div class="field-title">默认档位</div>
                    <div class="segmented">
                      <button type="button" class:active={settingsDraft.textProvider.defaultTier === "flash"} on:click={() => (settingsDraft.textProvider.defaultTier = "flash")}>Flash</button>
                      <button type="button" class:active={settingsDraft.textProvider.defaultTier === "pro"} on:click={() => (settingsDraft.textProvider.defaultTier = "pro")}>Pro</button>
                    </div>
                  </div>

                  <div class="field">
                    <div class="field-title">URI 路由类型</div>
                    <div class="segmented">
                      <button type="button" class:active={settingsDraft.textProvider.routeKind === "official"} on:click={() => (settingsDraft.textProvider.routeKind = "official")}>官方 Beta</button>
                      <button type="button" class:active={settingsDraft.textProvider.routeKind === "custom"} on:click={() => (settingsDraft.textProvider.routeKind = "custom")}>自定义兼容端点</button>
                    </div>
                  </div>

                  <div class="field-grid">
                    <div class="field">
                      <label for="flash-model">Flash 模型名</label>
                      <input id="flash-model" bind:value={settingsDraft.textProvider.presets.flash.model} />
                    </div>
                    <div class="field">
                      <label for="flash-url">Flash URL</label>
                      <input id="flash-url" bind:value={settingsDraft.textProvider.presets.flash.baseUrl} disabled />
                    </div>
                    <div class="field">
                      <label for="pro-model">Pro 模型名</label>
                      <input id="pro-model" bind:value={settingsDraft.textProvider.presets.pro.model} />
                    </div>
                    <div class="field">
                      <label for="pro-url">Pro URL</label>
                      <input id="pro-url" bind:value={settingsDraft.textProvider.presets.pro.baseUrl} disabled />
                    </div>
                  </div>

                  <div class="field">
                    <label for="custom-url">自定义 URL</label>
                    <input
                      id="custom-url"
                      bind:value={settingsDraft.textProvider.customBaseUrl}
                      disabled={settingsDraft.textProvider.routeKind !== "custom"}
                      placeholder="仅在自定义兼容端点模式下可编辑"
                    />
                  </div>

                  <div class="field">
                    <label for="text-api-key">DeepSeek API Key</label>
                    <input
                      id="text-api-key"
                      bind:value={pendingTextApiKey}
                      placeholder={settings.textProvider.hasApiKey ? `保持当前（${settings.textProvider.apiKeyMasked}）` : "输入新的 API Key"}
                    />
                  </div>
                </div>
              </div>

              <div class="panel">
                <div class="panel-head"><h3>视觉模型设置</h3></div>
                <div class="panel-body stack">
                  <div class="field">
                    <div class="field-title">允许高级覆盖</div>
                    <div class="segmented">
                      <button type="button" class:active={!settingsDraft.visionProvider.allowAdvancedOverride} on:click={() => (settingsDraft.visionProvider.allowAdvancedOverride = false)}>关闭</button>
                      <button type="button" class:active={settingsDraft.visionProvider.allowAdvancedOverride} on:click={() => (settingsDraft.visionProvider.allowAdvancedOverride = true)}>开启</button>
                    </div>
                  </div>

                  <div class="field-grid">
                    <div class="field">
                      <label for="vision-model">VL 模型</label>
                      <input id="vision-model" bind:value={settingsDraft.visionProvider.model} disabled={!settingsDraft.visionProvider.allowAdvancedOverride} />
                    </div>
                    <div class="field">
                      <label for="vision-url">VL URL</label>
                      <input id="vision-url" bind:value={settingsDraft.visionProvider.baseUrl} disabled={!settingsDraft.visionProvider.allowAdvancedOverride} />
                    </div>
                  </div>

                  <div class="field">
                    <label for="vision-api-key">Qwen API Key</label>
                    <input
                      id="vision-api-key"
                      bind:value={pendingVisionApiKey}
                      placeholder={settings.visionProvider.hasApiKey ? `保持当前（${settings.visionProvider.apiKeyMasked}）` : "输入新的 Vision Key"}
                    />
                  </div>
                </div>
              </div>
            </div>

            <div class="stack">
              <div class="panel">
                <div class="panel-head"><h3>成本与预算</h3></div>
                <div class="panel-body stack">
                  <div class="field-grid">
                    <div class="field">
                      <label for="budget-job">单任务预算（CNY）</label>
                      <input id="budget-job" type="number" min="0" step="0.1" bind:value={settingsDraft.budget.perJobCny} />
                    </div>
                    <div class="field">
                      <label for="budget-batch">单批次预算（CNY）</label>
                      <input id="budget-batch" type="number" min="0" step="0.1" bind:value={settingsDraft.budget.perBatchCny} />
                    </div>
                  </div>

                  <div class="field">
                    <div class="field-title">超预算策略</div>
                    <div class="segmented">
                      <button type="button" class:active={settingsDraft.budget.blockWhenOverBudget} on:click={() => (settingsDraft.budget.blockWhenOverBudget = true)}>禁止启动</button>
                      <button type="button" class:active={!settingsDraft.budget.blockWhenOverBudget} on:click={() => (settingsDraft.budget.blockWhenOverBudget = false)}>仅警告</button>
                    </div>
                  </div>

                  <div class="field-grid">
                    <div class="field"><label for="flash-in">Flash 输入 / 1M Tokens</label><input id="flash-in" type="number" min="0" step="0.01" bind:value={settingsDraft.budget.flashInputPerMTokensCny} /></div>
                    <div class="field"><label for="flash-out">Flash 输出 / 1M Tokens</label><input id="flash-out" type="number" min="0" step="0.01" bind:value={settingsDraft.budget.flashOutputPerMTokensCny} /></div>
                    <div class="field"><label for="pro-in">Pro 输入 / 1M Tokens</label><input id="pro-in" type="number" min="0" step="0.01" bind:value={settingsDraft.budget.proInputPerMTokensCny} /></div>
                    <div class="field"><label for="pro-out">Pro 输出 / 1M Tokens</label><input id="pro-out" type="number" min="0" step="0.01" bind:value={settingsDraft.budget.proOutputPerMTokensCny} /></div>
                    <div class="field"><label for="vl-in">VL 每帧输入成本</label><input id="vl-in" type="number" min="0" step="0.001" bind:value={settingsDraft.budget.vlInputPerFrameCny} /></div>
                    <div class="field"><label for="vl-out">VL 每帧输出成本</label><input id="vl-out" type="number" min="0" step="0.001" bind:value={settingsDraft.budget.vlOutputPerFrameCny} /></div>
                  </div>
                </div>
              </div>

              <div class="panel">
                <div class="panel-head"><h3>运行限制</h3></div>
                <div class="panel-body stack">
                  <div class="field-grid">
                    <div class="field"><label for="limit-frames">最大抽帧数</label><input id="limit-frames" type="number" min="1" bind:value={settingsDraft.limits.maxFrames} /></div>
                    <div class="field"><label for="limit-competitors">最大竞品数</label><input id="limit-competitors" type="number" min="1" bind:value={settingsDraft.limits.maxCompetitors} /></div>
                    <div class="field"><label for="limit-minutes">最长转写分钟数</label><input id="limit-minutes" type="number" min="1" bind:value={settingsDraft.limits.maxTranscriptionMinutes} /></div>
                  </div>

                  <div class="field-grid">
                    <div class="field">
                      <div class="field-title">自动 OCR</div>
                      <div class="segmented">
                        <button type="button" class:active={settingsDraft.limits.autoOcr} on:click={() => (settingsDraft.limits.autoOcr = true)}>开启</button>
                        <button type="button" class:active={!settingsDraft.limits.autoOcr} on:click={() => (settingsDraft.limits.autoOcr = false)}>关闭</button>
                      </div>
                    </div>
                    <div class="field">
                      <div class="field-title">自动 ASR</div>
                      <div class="segmented">
                        <button type="button" class:active={settingsDraft.limits.autoAsr} on:click={() => (settingsDraft.limits.autoAsr = true)}>开启</button>
                        <button type="button" class:active={!settingsDraft.limits.autoAsr} on:click={() => (settingsDraft.limits.autoAsr = false)}>关闭</button>
                      </div>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </section>
      {/if}
    {/if}
  </main>
</div>
