<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  export let sourcePrompt = "";
  export let sourceJobId = "";
  export let textTier = "flash";
  export let hasApiKey = false;
  export let perJobBudgetCny = 0;

  type StoryAct = {
    index: number;
    title: string;
    dramaticFunction: string;
    startSecond: number;
    endSecond: number;
    durationSeconds: number;
    spokenCharBudget: number;
    shotBudget: number;
    chapterGoal: string;
    rewrittenPrompt: string;
    narrationOutline: string;
    openingFrame: string;
    closingFrame: string;
    continuityFromPrevious: string;
    continuityToNext: string;
    transition: string;
  };

  type StoryPlan = {
    title: string;
    sourceSummary: string;
    designRequirements: string;
    targetDurationSeconds: number;
    actCount: number;
    continuityBible: {
      characterAnchor: string;
      wardrobeAnchor: string;
      settingAnchor: string;
      visualStyleAnchor: string;
      cameraAnchor: string;
      immutableRules: string[];
    };
    acts: StoryAct[];
    budget: {
      estimatedPromptTokens: number;
      estimatedCompletionTokens: number;
      actualPromptTokens: number;
      actualCompletionTokens: number;
      estimatedCostCny: number;
      actualCostCny: number;
      budgetLimitCny: number;
      exceedsBudget: boolean;
    };
    generatedByModel: string;
    generationWarning: string;
    generatedAtMs: number;
  };

  type SaveResult = {
    jsonPath: string;
    markdownPath: string;
    savedAtMs: number;
  };

  let plannerInput = "";
  let userRequirements = "";
  let targetMinutes = 3;
  let actCount = 5;
  let generating = false;
  let saving = false;
  let message = "";
  let tone: "good" | "warn" | "" = "";
  let plan: StoryPlan | null = null;
  let saveResult: SaveResult | null = null;

  function syncSourcePrompt() {
    plannerInput = sourcePrompt.trim();
    message = plannerInput ? "已带入当前完整提示词，可以继续修改后再拆分。" : "当前没有可带入的提示词，请直接粘贴。";
    tone = plannerInput ? "good" : "warn";
  }

  function errorText(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
  }

  function formatDuration(seconds: number): string {
    const minutes = Math.floor(seconds / 60);
    const rest = seconds % 60;
    return minutes ? `${minutes}分${rest ? `${rest}秒` : ""}` : `${rest}秒`;
  }

  function formatCost(value: number): string {
    return `¥${Number(value || 0).toFixed(4)}`;
  }

  async function generatePlan() {
    if (plannerInput.trim().length < 12) {
      tone = "warn";
      message = "请至少输入 12 个字符的故事或视频提示词。";
      return;
    }
    generating = true;
    saveResult = null;
    message = hasApiKey ? "正在调用文本模型拆分并重写连续章节…" : "未配置文本 Key，正在生成本地连续章节草稿…";
    tone = "";
    try {
      plan = await invoke<StoryPlan>("generate_story_plan", {
        request: {
          sourcePrompt: plannerInput.trim(),
          userRequirements: userRequirements.trim(),
          targetDurationSeconds: Math.round(Number(targetMinutes) * 60),
          actCount: Math.round(Number(actCount)),
          textTier,
        },
      });
      tone = plan.generationWarning || plan.budget.exceedsBudget ? "warn" : "good";
      message = plan.generationWarning || `已生成 ${plan.actCount} 幕连续剧情规划；相邻章节首尾帧已锁定一致。`;
    } catch (error) {
      tone = "warn";
      message = `章节规划失败：${errorText(error)}`;
    } finally {
      generating = false;
    }
  }

  function updateClosingFrame(index: number, value: string) {
    if (!plan) return;
    plan.acts[index].closingFrame = value;
    if (plan.acts[index + 1]) {
      plan.acts[index + 1].openingFrame = value;
      plan.acts[index + 1].continuityFromPrevious = `首帧已锁定为第 ${index + 1} 幕尾帧：${value}`;
    }
    plan = { ...plan, acts: [...plan.acts] };
  }

  function renderChapterDraft(value: StoryPlan): string {
    const lines = [`《${value.title}》`, ""];
    for (const act of value.acts) {
      lines.push(
        `【${act.title}】`,
        "",
        act.rewrittenPrompt,
        "",
        `口播 / 对白：${act.narrationOutline}`,
        "",
        `开场画面：${act.openingFrame}`,
        `收尾画面：${act.closingFrame}`,
        "",
        "──────────",
        "",
      );
    }
    return lines.join("\n").trim();
  }

  async function copyChapterDraft() {
    if (!plan) return;
    await navigator.clipboard.writeText(renderChapterDraft(plan));
    tone = "good";
    message = "分章节重写成稿已复制到剪贴板。";
  }

  async function savePlan() {
    if (!plan) return;
    saving = true;
    try {
      saveResult = await invoke<SaveResult>("save_story_plan", {
        plan,
        sourceJobId: sourceJobId || null,
      });
      tone = "good";
      message = "已另存为新的 JSON 和 Markdown 版本，没有覆盖旧稿。";
    } catch (error) {
      tone = "warn";
      message = `保存章节规划失败：${errorText(error)}`;
    } finally {
      saving = false;
    }
  }
</script>

<div class="story-planner stack">
  <div class="planner-toolbar">
    <div>
      <div class="section-label">长视频章节规划器</div>
      <div class="small muted">把提炼提示词或粘贴文本按总时长拆成连续剧情章节。</div>
    </div>
    <button type="button" class="btn" on:click={syncSourcePrompt}>带入当前完整提示词</button>
  </div>

  <div class="field">
    <label for="story-planner-source">原始提示词 / 故事底稿</label>
    <textarea id="story-planner-source" class="planner-source" bind:value={plannerInput} placeholder="粘贴需要拆分的完整提示词、故事梗概或素材提炼结果…"></textarea>
  </div>

  <div class="field">
    <label for="story-planner-requirements">自然语言创作要求（可选）</label>
    <textarea
      id="story-planner-requirements"
      class="planner-requirements"
      bind:value={userRequirements}
      placeholder="例如：第二幕加强冲突；整体保持温暖治愈；每幕结尾留悬念；人物不要换装；最后反转但不要悲剧。"
    ></textarea>
    <div class="small muted">模型会先理解这些要求，再重新设计章节结构、剧情节奏和各幕内容。</div>
  </div>

  <div class="planner-controls">
    <div class="field">
      <label for="story-duration">目标时长（分钟）</label>
      <input id="story-duration" type="number" min="0.5" max="30" step="0.5" bind:value={targetMinutes} />
    </div>
    <div class="field">
      <label for="story-acts">剧情章节 / 幕数</label>
      <input id="story-acts" type="number" min="2" max="12" step="1" bind:value={actCount} />
    </div>
    <div class="planner-budget-preview">
      <span class="small muted">规划预算上限</span>
      <strong>¥{Number(perJobBudgetCny || 0).toFixed(2)}</strong>
      <span class="small muted">默认示例：3 分钟 / 5 幕</span>
    </div>
    <button type="button" class="btn primary planner-generate" disabled={generating} on:click={() => void generatePlan()}>
      {generating ? "正在理解并重新设计…" : "理解需求并重新设计"}
    </button>
  </div>

  {#if message}
    <div class={`notice ${tone}`}>{message}</div>
  {/if}

  {#if plan}
    <div class="planner-summary">
      <div><span>总时长</span><strong>{formatDuration(plan.targetDurationSeconds)}</strong></div>
      <div><span>章节</span><strong>{plan.actCount} 幕</strong></div>
      <div><span>规划方式</span><strong>{plan.generatedByModel}</strong></div>
      <div><span>预估成本</span><strong>{formatCost(plan.budget.estimatedCostCny)}</strong></div>
      <div><span>实际成本</span><strong>{formatCost(plan.budget.actualCostCny)}</strong></div>
    </div>

    <div class="continuity-bible">
      <div class="section-label">全片连续性圣经</div>
      <div><strong>人物：</strong>{plan.continuityBible.characterAnchor}</div>
      <div><strong>服装：</strong>{plan.continuityBible.wardrobeAnchor}</div>
      <div><strong>场景：</strong>{plan.continuityBible.settingAnchor}</div>
      <div><strong>视觉：</strong>{plan.continuityBible.visualStyleAnchor}</div>
      <div><strong>镜头：</strong>{plan.continuityBible.cameraAnchor}</div>
    </div>

    <div class="act-list">
      {#each plan.acts as act, index}
        <article class="act-card">
          <div class="act-head">
            <div>
              <strong>{act.title}</strong>
              <div class="small muted">{act.startSecond}s–{act.endSecond}s · {act.durationSeconds} 秒</div>
            </div>
            <div class="act-badges">
              <span>{act.spokenCharBudget} 字口播</span>
              <span>{act.shotBudget} 镜头</span>
            </div>
          </div>
          <div class="field"><label for={`story-act-${act.index}-goal`}>章节任务</label><textarea id={`story-act-${act.index}-goal`} bind:value={act.chapterGoal}></textarea></div>
          <div class="field"><label for={`story-act-${act.index}-prompt`}>本幕重写内容</label><textarea id={`story-act-${act.index}-prompt`} class="act-prompt" bind:value={act.rewrittenPrompt}></textarea></div>
          <div class="field"><label for={`story-act-${act.index}-narration`}>口播 / 对白提纲</label><textarea id={`story-act-${act.index}-narration`} bind:value={act.narrationOutline}></textarea></div>
          <div class="frame-grid">
            <div class="field">
              <label for={`story-act-${act.index}-opening`}>本幕首帧 {index > 0 ? "（锁定为上一幕尾帧）" : "（全片基准）"}</label>
              <textarea id={`story-act-${act.index}-opening`} value={act.openingFrame} readonly={index > 0} on:input={(event) => (act.openingFrame = event.currentTarget.value)}></textarea>
            </div>
            <div class="field">
              <label for={`story-act-${act.index}-closing`}>本幕尾帧 {index + 1 < plan.acts.length ? "（自动同步到下一幕首帧）" : "（全片结尾）"}</label>
              <textarea id={`story-act-${act.index}-closing`} value={act.closingFrame} on:input={(event) => updateClosingFrame(index, event.currentTarget.value)}></textarea>
            </div>
          </div>
          <div class="continuity-note">{act.continuityToNext}</div>
          <div class="field"><label for={`story-act-${act.index}-transition`}>转场方式</label><textarea id={`story-act-${act.index}-transition`} bind:value={act.transition}></textarea></div>
        </article>
      {/each}
    </div>

    <div class="final-draft">
      <div class="final-draft-head">
        <div>
          <div class="section-label">分章节重写成稿</div>
          <div class="small muted">这里只汇总各章节重写正文、口播/对白和首尾画面，不包含规划预算。</div>
        </div>
        <button type="button" class="btn primary" on:click={() => void copyChapterDraft()}>一键复制分章节成稿</button>
      </div>
      <textarea class="final-draft-output" readonly value={renderChapterDraft(plan)} aria-label="分章节重写成稿汇总"></textarea>
    </div>

    <div class="planner-actions">
      <button type="button" class="btn" disabled={saving} on:click={() => void savePlan()}>{saving ? "保存中…" : "保存为新版本"}</button>
    </div>
    {#if saveResult}
      <div class="code-block">{saveResult.jsonPath}\n{saveResult.markdownPath}</div>
    {/if}
  {/if}
</div>

<style>
  .story-planner { padding: 16px; border: 1px solid var(--line); border-radius: 8px; background: var(--panel-soft); }
  .planner-toolbar, .planner-actions, .act-head { display: flex; align-items: flex-start; justify-content: space-between; gap: 12px; flex-wrap: wrap; }
  .planner-source { min-height: 180px; font-family: ui-monospace, "Cascadia Code", Consolas, monospace; }
  .planner-requirements { min-height: 110px; }
  .planner-controls { display: grid; grid-template-columns: 150px 150px minmax(190px, 1fr) auto; gap: 12px; align-items: end; }
  .planner-budget-preview { min-height: 62px; display: flex; flex-direction: column; justify-content: center; padding: 8px 12px; border: 1px solid var(--line); border-radius: 8px; background: white; }
  .planner-generate { min-height: 40px; }
  .planner-summary { display: grid; grid-template-columns: repeat(5, minmax(0, 1fr)); border: 1px solid var(--line); border-radius: 8px; background: white; }
  .planner-summary div { display: flex; flex-direction: column; gap: 5px; padding: 12px; border-right: 1px solid var(--line); }
  .planner-summary div:last-child { border-right: 0; }
  .planner-summary span { color: var(--text-soft); font-size: 12px; }
  .continuity-bible { display: grid; gap: 8px; padding: 14px; border: 1px solid #bfd3ff; border-radius: 8px; background: var(--accent-soft); font-size: 13px; line-height: 1.55; }
  .act-list { display: flex; flex-direction: column; gap: 14px; }
  .act-card { display: flex; flex-direction: column; gap: 12px; padding: 16px; border: 1px solid var(--line); border-radius: 8px; background: white; }
  .act-badges { display: flex; gap: 8px; flex-wrap: wrap; }
  .act-badges span { padding: 5px 8px; border-radius: 999px; background: #eef2f7; color: var(--text-soft); font-size: 12px; font-weight: 700; }
  .act-prompt { min-height: 140px; }
  .frame-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  .frame-grid textarea { min-height: 120px; }
  .frame-grid textarea[readonly] { background: #f1f5f9; color: #526173; }
  .continuity-note { padding: 10px 12px; border-left: 3px solid var(--accent); background: #f6f9ff; color: var(--text-soft); font-size: 12px; line-height: 1.55; }
  .final-draft { display: flex; flex-direction: column; gap: 12px; padding: 16px; border: 1px solid #bfd3ff; border-radius: 8px; background: white; }
  .final-draft-head { display: flex; align-items: flex-start; justify-content: space-between; gap: 12px; flex-wrap: wrap; }
  .final-draft-output { min-height: 420px; font-family: ui-monospace, "Cascadia Code", Consolas, monospace; font-size: 13px; line-height: 1.7; background: #f8fafc; }
  @media (max-width: 1200px) {
    .planner-controls, .planner-summary, .frame-grid { grid-template-columns: 1fr; }
    .planner-summary div { border-right: 0; border-bottom: 1px solid var(--line); }
  }
</style>
