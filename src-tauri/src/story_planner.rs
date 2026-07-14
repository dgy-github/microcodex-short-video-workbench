use serde::{Deserialize, Serialize};

const MIN_DURATION_SECONDS: u32 = 30;
const MAX_DURATION_SECONDS: u32 = 1_800;
const MIN_ACTS: u32 = 2;
const MAX_ACTS: u32 = 12;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoryPlanRequest {
    pub(crate) source_prompt: String,
    #[serde(default)]
    pub(crate) user_requirements: String,
    pub(crate) target_duration_seconds: u32,
    pub(crate) act_count: u32,
    pub(crate) text_tier: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContinuityBible {
    #[serde(default)]
    pub(crate) character_anchor: String,
    #[serde(default)]
    pub(crate) wardrobe_anchor: String,
    #[serde(default)]
    pub(crate) setting_anchor: String,
    #[serde(default)]
    pub(crate) visual_style_anchor: String,
    #[serde(default)]
    pub(crate) camera_anchor: String,
    #[serde(default)]
    pub(crate) immutable_rules: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoryAct {
    #[serde(default)]
    pub(crate) index: u32,
    #[serde(default)]
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) dramatic_function: String,
    #[serde(default)]
    pub(crate) start_second: u32,
    #[serde(default)]
    pub(crate) end_second: u32,
    #[serde(default)]
    pub(crate) duration_seconds: u32,
    #[serde(default)]
    pub(crate) spoken_char_budget: u32,
    #[serde(default)]
    pub(crate) shot_budget: u32,
    #[serde(default)]
    pub(crate) chapter_goal: String,
    #[serde(default)]
    pub(crate) rewritten_prompt: String,
    #[serde(default)]
    pub(crate) narration_outline: String,
    #[serde(default)]
    pub(crate) opening_frame: String,
    #[serde(default)]
    pub(crate) closing_frame: String,
    #[serde(default)]
    pub(crate) continuity_from_previous: String,
    #[serde(default)]
    pub(crate) continuity_to_next: String,
    #[serde(default)]
    pub(crate) transition: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoryPlanningBudget {
    pub(crate) estimated_prompt_tokens: u32,
    pub(crate) estimated_completion_tokens: u32,
    pub(crate) actual_prompt_tokens: u32,
    pub(crate) actual_completion_tokens: u32,
    pub(crate) estimated_cost_cny: f64,
    pub(crate) actual_cost_cny: f64,
    pub(crate) budget_limit_cny: f64,
    pub(crate) exceeds_budget: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoryPlan {
    #[serde(default)]
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) source_summary: String,
    #[serde(default)]
    pub(crate) design_requirements: String,
    #[serde(default)]
    pub(crate) target_duration_seconds: u32,
    #[serde(default)]
    pub(crate) act_count: u32,
    #[serde(default)]
    pub(crate) continuity_bible: ContinuityBible,
    #[serde(default)]
    pub(crate) acts: Vec<StoryAct>,
    #[serde(default)]
    pub(crate) budget: StoryPlanningBudget,
    #[serde(default)]
    pub(crate) generated_by_model: String,
    #[serde(default)]
    pub(crate) generation_warning: String,
    #[serde(default)]
    pub(crate) generated_at_ms: u64,
}

pub(crate) fn validate_request(request: &StoryPlanRequest) -> Result<(), String> {
    if request.source_prompt.trim().chars().count() < 12 {
        return Err("提示词太短，至少需要 12 个字符。".to_string());
    }
    if request.user_requirements.chars().count() > 8_000 {
        return Err("自然语言创作要求不能超过 8000 个字符。".to_string());
    }
    if !(MIN_DURATION_SECONDS..=MAX_DURATION_SECONDS).contains(&request.target_duration_seconds) {
        return Err("目标时长必须在 30 秒到 30 分钟之间。".to_string());
    }
    if !(MIN_ACTS..=MAX_ACTS).contains(&request.act_count) {
        return Err("章节数必须在 2 到 12 幕之间。".to_string());
    }
    Ok(())
}

pub(crate) fn estimated_token_budget(request: &StoryPlanRequest) -> (u32, u32) {
    let source_chars = request
        .source_prompt
        .trim()
        .chars()
        .count()
        .saturating_add(request.user_requirements.trim().chars().count())
        as f64;
    let prompt_tokens = (source_chars * 0.65).ceil() as u32 + request.act_count * 180;
    let completion_tokens = request.act_count * 620 + 500;
    (prompt_tokens, completion_tokens)
}

pub(crate) fn build_local_plan(
    request: &StoryPlanRequest,
    budget: StoryPlanningBudget,
    generated_at_ms: u64,
) -> Result<StoryPlan, String> {
    validate_request(request)?;
    let durations = allocate_durations(request.target_duration_seconds, request.act_count);
    let excerpts = split_source_prompt(&request.source_prompt, request.act_count as usize);
    let summary = compact(&request.source_prompt, 120);
    let requirements = request.user_requirements.trim();
    let mut acts = Vec::with_capacity(request.act_count as usize);
    let mut cursor = 0;

    for (offset, duration) in durations.into_iter().enumerate() {
        let index = offset as u32 + 1;
        let role = dramatic_role(index, request.act_count);
        let source = excerpts
            .get(offset)
            .cloned()
            .unwrap_or_else(|| summary.clone());
        let closing_frame = format!(
            "第 {index} 幕尾帧：主角保持统一外貌、服装和空间方位，在“{role}”动作落点处停住；记录人物姿态、视线、手部位置、道具位置、光线方向和镜头景别。"
        );
        acts.push(StoryAct {
            index,
            title: format!("第 {index} 幕 · {role}"),
            dramatic_function: role.to_string(),
            start_second: cursor,
            end_second: cursor + duration,
            duration_seconds: duration,
            spoken_char_budget: duration.saturating_mul(4),
            shot_budget: duration.div_ceil(8).max(1),
            chapter_goal: chapter_goal(role),
            rewritten_prompt: format!(
                "{source}。本幕以“{role}”为叙事功能，从明确可见的动作开始，围绕人物当前目标推进事件和情绪；{}所有人物身份、服装、场景空间、时间、色调和镜头轴线保持一致，结尾停在可直接承接下一幕的动作上。",
                if requirements.is_empty() {
                    String::new()
                } else {
                    format!("按用户要求重新设计：{}。", compact(requirements, 300))
                }
            ),
            narration_outline: format!(
                "口播控制在约 {} 个中文字：先给本幕目标，再推进信息或冲突，结尾留下可接续的动作。",
                duration.saturating_mul(4)
            ),
            opening_frame: String::new(),
            closing_frame,
            continuity_from_previous: String::new(),
            continuity_to_next: String::new(),
            transition: "优先使用动作承接、视线承接或同构图切换，避免人物瞬移和场景跳变。".to_string(),
        });
        cursor += duration;
    }

    enforce_frame_continuity(&mut acts);
    Ok(StoryPlan {
        title: format!(
            "{} 秒 · {} 幕连续剧情规划",
            request.target_duration_seconds, request.act_count
        ),
        source_summary: summary,
        design_requirements: request.user_requirements.trim().to_string(),
        target_duration_seconds: request.target_duration_seconds,
        act_count: request.act_count,
        continuity_bible: default_continuity_bible(),
        acts,
        budget,
        generated_by_model: "本地章节规划器".to_string(),
        generation_warning: String::new(),
        generated_at_ms,
    })
}

pub(crate) fn merge_ai_plan(
    request: &StoryPlanRequest,
    ai_plan: StoryPlan,
    budget: StoryPlanningBudget,
    model: &str,
    generated_at_ms: u64,
) -> Result<StoryPlan, String> {
    let mut plan = build_local_plan(request, budget, generated_at_ms)?;
    replace_if_present(&mut plan.title, ai_plan.title);
    replace_if_present(&mut plan.source_summary, ai_plan.source_summary);
    merge_bible(&mut plan.continuity_bible, ai_plan.continuity_bible);

    for (target, source) in plan.acts.iter_mut().zip(ai_plan.acts) {
        replace_if_present(&mut target.title, source.title);
        replace_if_present(&mut target.dramatic_function, source.dramatic_function);
        replace_if_present(&mut target.chapter_goal, source.chapter_goal);
        replace_if_present(&mut target.rewritten_prompt, source.rewritten_prompt);
        replace_if_present(&mut target.narration_outline, source.narration_outline);
        replace_if_present(&mut target.opening_frame, source.opening_frame);
        replace_if_present(&mut target.closing_frame, source.closing_frame);
        replace_if_present(&mut target.transition, source.transition);
    }
    enforce_frame_continuity(&mut plan.acts);
    plan.generated_by_model = model.to_string();
    plan.generation_warning.clear();
    Ok(plan)
}

pub(crate) fn render_markdown(plan: &StoryPlan) -> String {
    let mut lines = vec![
        format!("# {}", plan.title),
        String::new(),
        format!("- 总时长：{} 秒", plan.target_duration_seconds),
        format!("- 章节数：{} 幕", plan.act_count),
        format!(
            "- 用户创作要求：{}",
            if plan.design_requirements.trim().is_empty() {
                "未提供"
            } else {
                plan.design_requirements.trim()
            }
        ),
        format!("- 生成方式：{}", plan.generated_by_model),
        format!("- 实际规划成本：¥{:.4}", plan.budget.actual_cost_cny),
        String::new(),
        "## 连续性圣经".to_string(),
        String::new(),
        format!("- 人物：{}", plan.continuity_bible.character_anchor),
        format!("- 服装：{}", plan.continuity_bible.wardrobe_anchor),
        format!("- 场景：{}", plan.continuity_bible.setting_anchor),
        format!("- 视觉：{}", plan.continuity_bible.visual_style_anchor),
        format!("- 镜头：{}", plan.continuity_bible.camera_anchor),
    ];
    for act in &plan.acts {
        lines.extend([
            String::new(),
            format!("## {}", act.title),
            String::new(),
            format!(
                "- 时间：{}s–{}s（{} 秒）",
                act.start_second, act.end_second, act.duration_seconds
            ),
            format!("- 口播预算：约 {} 字", act.spoken_char_budget),
            format!("- 镜头预算：{} 个", act.shot_budget),
            format!("- 剧情任务：{}", act.chapter_goal),
            format!("- 首帧：{}", act.opening_frame),
            format!("- 尾帧：{}", act.closing_frame),
            format!("- 转场：{}", act.transition),
            String::new(),
            "### 本幕提示词".to_string(),
            String::new(),
            act.rewritten_prompt.clone(),
            String::new(),
            "### 口播提纲".to_string(),
            String::new(),
            act.narration_outline.clone(),
        ]);
    }
    lines.join("\n")
}

fn allocate_durations(total: u32, count: u32) -> Vec<u32> {
    let weights = if count == 5 {
        vec![15, 20, 25, 25, 15]
    } else {
        (0..count)
            .map(|index| {
                if index == 0 || index + 1 == count {
                    15
                } else {
                    20
                }
            })
            .collect::<Vec<_>>()
    };
    let weight_total: u32 = weights.iter().sum();
    let mut durations = weights
        .iter()
        .map(|weight| total.saturating_mul(*weight) / weight_total)
        .collect::<Vec<_>>();
    let assigned: u32 = durations.iter().sum();
    let duration_count = durations.len();
    for index in 0..total.saturating_sub(assigned) {
        durations[index as usize % duration_count] += 1;
    }
    durations
}

fn split_source_prompt(source: &str, count: usize) -> Vec<String> {
    let segments = source
        .split(['。', '！', '？', ';', '；', '\n'])
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if segments.is_empty() {
        return vec![compact(source, 160); count];
    }
    (0..count)
        .map(|index| {
            let start = index * segments.len() / count;
            let mut end = (index + 1) * segments.len() / count;
            if end <= start {
                end = (start + 1).min(segments.len());
            }
            compact(
                &segments[start.min(segments.len() - 1)..end].join("；"),
                180,
            )
        })
        .collect()
}

fn dramatic_role(index: u32, count: u32) -> &'static str {
    if count == 5 {
        return ["开端", "发展", "转折", "高潮", "收束"][(index - 1) as usize];
    }
    if index == 1 {
        "开端"
    } else if index == count {
        "收束"
    } else if index * 2 > count {
        "冲突与高潮"
    } else {
        "发展"
    }
}

fn chapter_goal(role: &str) -> String {
    match role {
        "开端" => "建立人物、场景、目标和第一处悬念，让观众理解故事从哪里开始。",
        "发展" => "推进因果与行动，增加信息密度，但保持人物动机和空间关系清楚。",
        "转折" => "引入改变方向的新事实或事件，让前面的铺垫产生新的解释。",
        "高潮" | "冲突与高潮" => "集中兑现核心冲突和情绪峰值，完成最重要的动作或观点。",
        _ => "回收伏笔、完成情绪落点，并给结尾画面留下明确、稳定的余韵。",
    }
    .to_string()
}

fn default_continuity_bible() -> ContinuityBible {
    ContinuityBible {
        character_anchor: "主角的年龄、脸型、发型、体型、声音和行为气质全片保持一致。".to_string(),
        wardrobe_anchor: "服装颜色、材质、配饰及其穿戴位置保持一致，剧情明确换装时除外。"
            .to_string(),
        setting_anchor: "场景布局、门窗和主要道具位置固定，人物移动必须符合空间路径。".to_string(),
        visual_style_anchor: "画幅、色彩、光线方向、颗粒和整体质感保持统一。".to_string(),
        camera_anchor: "保持镜头轴线和人物朝向连续，跨章首尾优先复用同一景别和机位。".to_string(),
        immutable_rules: vec![
            "后一幕首帧必须逐字复用前一幕尾帧描述。".to_string(),
            "跨章不得无解释地改变人物、服装、道具数量、时间或天气。".to_string(),
            "允许变化的内容必须在上一幕尾部通过动作或镜头明确发起。".to_string(),
        ],
    }
}

fn enforce_frame_continuity(acts: &mut [StoryAct]) {
    if let Some(first) = acts.first_mut() {
        if first.opening_frame.trim().is_empty() {
            first.opening_frame = "全片首帧：清楚建立主角外貌、服装、场景布局、时间、光线方向和镜头景别，作为后续连续性基准。".to_string();
        }
        first.continuity_from_previous = "这是全片基准首帧，无上一幕。".to_string();
    }
    for index in 1..acts.len() {
        let previous_closing = acts[index - 1].closing_frame.clone();
        acts[index].opening_frame = previous_closing.clone();
        acts[index - 1].continuity_to_next = format!(
            "尾帧必须原样作为第 {} 幕首帧；人物姿态、视线、手部、服装、道具、光线和机位不得跳变。",
            index + 1
        );
        acts[index].continuity_from_previous =
            format!("首帧已锁定为第 {} 幕尾帧：{}", index, previous_closing);
    }
    if let Some(last) = acts.last_mut() {
        last.continuity_to_next =
            "这是全片结尾，无下一幕；保持尾帧稳定以便停留或淡出。".to_string();
    }
}

fn merge_bible(target: &mut ContinuityBible, source: ContinuityBible) {
    replace_if_present(&mut target.character_anchor, source.character_anchor);
    replace_if_present(&mut target.wardrobe_anchor, source.wardrobe_anchor);
    replace_if_present(&mut target.setting_anchor, source.setting_anchor);
    replace_if_present(&mut target.visual_style_anchor, source.visual_style_anchor);
    replace_if_present(&mut target.camera_anchor, source.camera_anchor);
    if !source.immutable_rules.is_empty() {
        target.immutable_rules = source.immutable_rules;
    }
}

fn replace_if_present(target: &mut String, source: String) {
    if !source.trim().is_empty() {
        *target = source.trim().to_string();
    }
}

fn compact(value: &str, max_chars: usize) -> String {
    let cleaned = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.chars().count() <= max_chars {
        return cleaned;
    }
    cleaned.chars().take(max_chars).collect::<String>() + "…"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> StoryPlanRequest {
        StoryPlanRequest {
            source_prompt: "一个年轻人在旧车站寻找失踪多年的父亲，途中发现一封没有寄出的信。故事需要克制、温暖，并在结尾完成和解。".to_string(),
            user_requirements: String::new(),
            target_duration_seconds: 180,
            act_count: 5,
            text_tier: "flash".to_string(),
        }
    }

    #[test]
    fn five_act_duration_budget_is_exact() {
        let durations = allocate_durations(180, 5);
        assert_eq!(durations, vec![27, 36, 45, 45, 27]);
        assert_eq!(durations.iter().sum::<u32>(), 180);
    }

    #[test]
    fn local_plan_locks_each_handoff_frame() {
        let plan = build_local_plan(&request(), StoryPlanningBudget::default(), 1).unwrap();
        assert_eq!(plan.acts.len(), 5);
        for index in 1..plan.acts.len() {
            assert_eq!(
                plan.acts[index - 1].closing_frame,
                plan.acts[index].opening_frame
            );
        }
    }

    #[test]
    fn rejects_invalid_duration_and_act_count() {
        let mut value = request();
        value.target_duration_seconds = 20;
        assert!(validate_request(&value).is_err());
        value.target_duration_seconds = 180;
        value.act_count = 13;
        assert!(validate_request(&value).is_err());
    }

    #[test]
    fn local_plan_records_natural_language_requirements() {
        let mut value = request();
        value.user_requirements = "第二幕增加冲突，结尾保持治愈。".to_string();
        let plan = build_local_plan(&value, StoryPlanningBudget::default(), 1).unwrap();
        assert_eq!(plan.design_requirements, value.user_requirements);
        assert!(plan.acts[0].rewritten_prompt.contains("第二幕增加冲突"));
    }
}
