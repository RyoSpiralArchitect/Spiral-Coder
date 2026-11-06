use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DriftMetric {
    Cos,
    Kl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RealizePreset {
    Off,
    Low,
    Mid,
    High,
}

impl RealizePreset {
    pub const fn tui_default() -> Self {
        Self::Mid
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Low => "low",
            Self::Mid => "mid",
            Self::High => "high",
        }
    }

    pub(crate) fn config(self) -> RealizeOnDemandConfig {
        match self {
            Self::Off => {
                let mut cfg = RealizeOnDemandConfig::default();
                cfg.enabled = false;
                cfg
            }
            Self::Low => RealizeOnDemandConfig {
                enabled: true,
                defer_threshold: 0.62,
                window_start: 1,
                window_end: 2,
                drift_metric: DriftMetric::Cos,
                lambda_min: 0.08,
                lambda_max: 0.45,
            },
            Self::Mid => RealizeOnDemandConfig {
                enabled: true,
                defer_threshold: 0.45,
                window_start: 1,
                window_end: 3,
                drift_metric: DriftMetric::Cos,
                lambda_min: 0.15,
                lambda_max: 0.90,
            },
            Self::High => RealizeOnDemandConfig {
                enabled: true,
                defer_threshold: 0.30,
                window_start: 1,
                window_end: 4,
                drift_metric: DriftMetric::Cos,
                lambda_min: 0.25,
                lambda_max: 1.30,
            },
        }
    }

    pub fn summary(self) -> String {
        if self == Self::Off {
            return "off (latent claim/plan defer disabled)".to_string();
        }
        let cfg = self.config();
        format!(
            "{} (threshold={:.2}, window={}..{}, drift={}, lambda={:.2}..{:.2})",
            self.label(),
            cfg.defer_threshold,
            cfg.window_start,
            cfg.window_end,
            match cfg.drift_metric {
                DriftMetric::Cos => "cos",
                DriftMetric::Kl => "kl",
            },
            cfg.lambda_min,
            cfg.lambda_max
        )
    }
}

impl std::str::FromStr for RealizePreset {
    type Err = ();

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" => Ok(Self::Off),
            "low" => Ok(Self::Low),
            "mid" | "medium" => Ok(Self::Mid),
            "high" => Ok(Self::High),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RealizeOnDemandConfig {
    pub(crate) enabled: bool,
    pub(crate) defer_threshold: f64,
    pub(crate) window_start: usize,
    pub(crate) window_end: usize,
    pub(crate) drift_metric: DriftMetric,
    pub(crate) lambda_min: f64,
    pub(crate) lambda_max: f64,
}

impl Default for RealizeOnDemandConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            defer_threshold: 0.45,
            window_start: 1,
            window_end: 3,
            drift_metric: DriftMetric::Cos,
            lambda_min: 0.15,
            lambda_max: 0.90,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LatentPlanBuffer {
    pub(crate) raw_text: String,
    pub(crate) plan: PlanBlock,
    pub(crate) summary: String,
    pub(crate) latest_intent: Option<String>,
    pub(crate) anchor_baseline: String,
    pub(crate) created_iter: usize,
    pub(crate) defer_score: f64,
    pub(crate) tail_updates: usize,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct RealizeMetrics {
    pub(crate) within_window_turns: usize,
    pub(crate) early_leakage: usize,
    pub(crate) missing: usize,
    pub(crate) total_drift: f64,
    pub(crate) drift_samples: usize,
    pub(crate) realize_count: usize,
    pub(crate) total_realize_latency: usize,
}

impl RealizeMetrics {
    pub(crate) fn mean_drift(&self) -> f64 {
        if self.drift_samples == 0 {
            0.0
        } else {
            self.total_drift / self.drift_samples as f64
        }
    }

    pub(crate) fn mean_realize_latency(&self) -> f64 {
        if self.realize_count == 0 {
            0.0
        } else {
            self.total_realize_latency as f64 / self.realize_count as f64
        }
    }
}

pub(crate) fn env_bool(name: &str, default: bool) -> bool {
    match std::env::var(name) {
        Ok(value) => matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => default,
    }
}

pub(crate) fn env_f64(name: &str, default: f64) -> f64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse::<f64>().ok())
        .unwrap_or(default)
}

pub(crate) fn parse_realization_window(raw: &str) -> Option<(usize, usize)> {
    let trimmed = raw.trim();
    let (lhs, rhs) = trimmed
        .split_once(':')
        .or_else(|| trimmed.split_once(','))
        .or_else(|| trimmed.split_once('-'))?;
    let start = lhs.trim().parse::<usize>().ok()?;
    let end = rhs.trim().parse::<usize>().ok()?;
    if start > end || end == 0 {
        return None;
    }
    Some((start, end))
}

impl RealizeOnDemandConfig {
    pub(crate) fn resolve(preset: Option<RealizePreset>) -> Self {
        match preset {
            Some(preset) => preset.config(),
            None => Self::from_env(),
        }
    }

    pub(crate) fn from_env() -> Self {
        let mut cfg = Self::default();
        cfg.enabled = env_bool("SPIRAL_CODER_REALIZE_ON_DEMAND", false);
        cfg.defer_threshold =
            env_f64("SPIRAL_CODER_REALIZE_DEFER_THRESHOLD", cfg.defer_threshold).clamp(0.05, 1.0);
        if let Ok(raw) = std::env::var("SPIRAL_CODER_REALIZATION_WINDOW") {
            if let Some((start, end)) = parse_realization_window(&raw) {
                cfg.window_start = start;
                cfg.window_end = end.max(start);
            }
        }
        cfg.drift_metric = match std::env::var("SPIRAL_CODER_REALIZE_DRIFT_METRIC")
            .unwrap_or_else(|_| "cos".to_string())
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "kl" => DriftMetric::Kl,
            _ => DriftMetric::Cos,
        };
        cfg.lambda_min = env_f64("SPIRAL_CODER_REALIZE_LAMBDA_MIN", cfg.lambda_min).clamp(0.0, 4.0);
        cfg.lambda_max = env_f64("SPIRAL_CODER_REALIZE_LAMBDA_MAX", cfg.lambda_max).clamp(0.0, 4.0);
        if cfg.lambda_max < cfg.lambda_min {
            cfg.lambda_max = cfg.lambda_min;
        }
        cfg
    }

    pub(crate) fn within_window(&self, age_turns: usize) -> bool {
        age_turns >= self.window_start && age_turns < self.window_end
    }

    pub(crate) fn lambda_for_age(&self, age_turns: usize) -> f64 {
        if self.window_end <= self.window_start {
            return self.lambda_max;
        }
        let numer = age_turns.saturating_sub(self.window_start) as f64;
        let denom = (self.window_end - self.window_start).max(1) as f64;
        let frac = (numer / denom).clamp(0.0, 1.0);
        self.lambda_min + (self.lambda_max - self.lambda_min) * frac
    }
}

pub(crate) fn token_freqs(s: &str) -> BTreeMap<String, f64> {
    let mut out = BTreeMap::<String, f64>::new();
    let mut cur = String::new();
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            cur.push(ch.to_ascii_lowercase());
        } else if !cur.is_empty() {
            *out.entry(std::mem::take(&mut cur)).or_insert(0.0) += 1.0;
        }
    }
    if !cur.is_empty() {
        *out.entry(cur).or_insert(0.0) += 1.0;
    }
    out
}

pub(crate) fn cosine_token_distance(a: &str, b: &str) -> f64 {
    let fa = token_freqs(a);
    let fb = token_freqs(b);
    if fa.is_empty() || fb.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0;
    let mut na = 0.0;
    let mut nb = 0.0;
    for value in fa.values() {
        na += value * value;
    }
    for value in fb.values() {
        nb += value * value;
    }
    for (token, av) in &fa {
        if let Some(bv) = fb.get(token) {
            dot += av * bv;
        }
    }
    let denom = na.sqrt() * nb.sqrt();
    if denom <= f64::EPSILON {
        0.0
    } else {
        (1.0 - (dot / denom)).clamp(0.0, 1.0)
    }
}

pub(crate) fn kl_token_distance(a: &str, b: &str) -> f64 {
    let fa = token_freqs(a);
    let fb = token_freqs(b);
    if fa.is_empty() || fb.is_empty() {
        return 0.0;
    }
    let sum_a: f64 = fa.values().sum();
    let sum_b: f64 = fb.values().sum();
    let eps = 1e-6;
    let mut all_keys: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    all_keys.extend(fa.keys().cloned());
    all_keys.extend(fb.keys().cloned());
    let mut kl = 0.0;
    for key in all_keys {
        let pa = fa.get(&key).copied().unwrap_or(0.0) / sum_a + eps;
        let pb = fb.get(&key).copied().unwrap_or(0.0) / sum_b + eps;
        kl += pa * (pa / pb).ln();
    }
    kl.clamp(0.0, 4.0) / 4.0
}

pub(crate) fn drift_distance(metric: DriftMetric, tail: &str, anchor: &str) -> f64 {
    match metric {
        DriftMetric::Cos => cosine_token_distance(tail, anchor),
        DriftMetric::Kl => kl_token_distance(tail, anchor),
    }
}

pub(crate) fn strip_tag_block_owned(text: &str, tag: &str) -> String {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let Some(start) = text.find(&open) else {
        return text.to_string();
    };
    let rest = &text[start + open.len()..];
    let Some(end_rel) = rest.find(&close) else {
        return text.to_string();
    };
    let end = start + open.len() + end_rel + close.len();
    let mut out = String::new();
    out.push_str(text[..start].trim_end());
    if !out.is_empty() && !text[end..].trim_start().is_empty() {
        out.push('\n');
    }
    out.push_str(text[end..].trim_start());
    out
}

pub(crate) fn parse_realize_block(text: &str) -> Option<String> {
    let body = extract_tag_block(text, "realize")?;
    let fields = parse_tag_fields(body, "realize");
    for (key, value) in fields {
        if key == "reason" && !value.trim().is_empty() {
            return Some(compact_one_line(value.trim(), 160));
        }
    }
    if body.trim().is_empty() {
        None
    } else {
        Some(compact_one_line(body.trim(), 160))
    }
}

pub(crate) fn strip_known_latent_tags(text: &str) -> String {
    let no_plan = strip_tag_block_owned(text, "plan");
    let no_think = strip_tag_block_owned(&no_plan, "think");
    strip_tag_block_owned(&no_think, "realize")
}

pub(crate) fn think_intent_summary(think: &ThinkBlock) -> String {
    format!(
        "tool: {}; next: {}; verify: {}",
        compact_one_line(think.tool.as_str(), 24),
        compact_one_line(think.next.as_str(), 80),
        compact_one_line(think.verify.as_str(), 70)
    )
}

pub(crate) fn latent_plan_summary(
    plan: &PlanBlock,
    raw_text: &str,
    latest_intent: Option<&str>,
) -> String {
    let mut out = format!("goal: {}", compact_one_line(plan.goal.as_str(), 120));
    if !plan.steps.is_empty() {
        let steps = plan
            .steps
            .iter()
            .take(2)
            .map(|s| compact_one_line(s, 80))
            .collect::<Vec<_>>()
            .join(" | ");
        out.push_str(&format!("; steps: {steps}"));
    }
    if !plan.acceptance_criteria.is_empty() {
        let acc = plan
            .acceptance_criteria
            .iter()
            .take(2)
            .map(|s| compact_one_line(s, 70))
            .collect::<Vec<_>>()
            .join(" | ");
        out.push_str(&format!("; acceptance: {acc}"));
    }
    let tail = compact_one_line(strip_known_latent_tags(raw_text).as_str(), 120);
    if tail != "-" {
        out.push_str(&format!("; tail: {tail}"));
    }
    if let Some(intent) = latest_intent.map(str::trim).filter(|s| !s.is_empty()) {
        out.push_str(&format!("; intent: {}", compact_one_line(intent, 120)));
    }
    out
}

pub(crate) fn latest_intent_anchor_baseline(messages: &[serde_json::Value]) -> Option<String> {
    messages.iter().rev().find_map(|message| {
        if message["role"].as_str() != Some("system") {
            return None;
        }
        let content = message["content"].as_str()?;
        if !content.starts_with("[Intent Anchor]") {
            return None;
        }
        content.lines().find_map(|line| {
            line.strip_prefix("baseline: ")
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToOwned::to_owned)
        })
    })
}

pub(crate) fn build_anchor_baseline(
    messages: &[serde_json::Value],
    root_user_text: &str,
    active_plan: Option<&PlanBlock>,
    working_mem: &WorkingMemory,
) -> String {
    let mut parts = vec![latest_intent_anchor_baseline(messages)
        .unwrap_or_else(|| compact_one_line(root_user_text, 220))];
    if let Some(plan) = active_plan {
        parts.push(format!(
            "active_goal: {}",
            compact_one_line(plan.goal.as_str(), 120)
        ));
    }
    if let Some(strategy) = working_mem.chosen_strategy.as_deref() {
        parts.push(format!("strategy: {}", compact_one_line(strategy, 120)));
    }
    parts.join(" | ")
}

pub(crate) fn latent_plan_defer_score(raw_text: &str, plan: &PlanBlock) -> f64 {
    let plan_bonus = if plan.steps.len() >= 2 { 0.45 } else { 0.35 };
    let acceptance_bonus = (plan.acceptance_criteria.len().min(2) as f64) * 0.10;
    let narrative = strip_known_latent_tags(raw_text);
    let narrative_bonus = (narrative.trim().chars().count().min(160) as f64 / 160.0) * 0.15;
    (plan_bonus + acceptance_bonus + narrative_bonus).clamp(0.0, 1.0)
}

pub(crate) fn latent_intent_defer_score(raw_text: &str, think: &ThinkBlock) -> f64 {
    let tool_bonus = 0.18;
    let next_bonus = if think.next.trim().is_empty() {
        0.0
    } else {
        0.16
    };
    let verify_bonus = if think.verify.trim().is_empty() {
        0.0
    } else {
        0.12
    };
    let narrative = strip_known_latent_tags(raw_text);
    let narrative_bonus = (narrative.trim().chars().count().min(120) as f64 / 120.0) * 0.10;
    (tool_bonus + next_bonus + verify_bonus + narrative_bonus).clamp(0.0, 1.0)
}

pub(crate) fn build_realize_pending_hint(
    cfg: &RealizeOnDemandConfig,
    latent: &LatentPlanBuffer,
    metrics: &RealizeMetrics,
    age_turns: usize,
    drift: f64,
) -> String {
    let window_state = if cfg.within_window(age_turns) {
        "within_window"
    } else {
        "deadline_near"
    };
    let lambda = cfg.lambda_for_age(age_turns);
    format!(
        "[Realize-on-Demand]\n\
There is a pending latent plan/claim buffer that is NOT committed yet.\n\
window_state: {window_state}\n\
age_turns: {age_turns}/{window_end}\n\
defer_score: {defer_score:.2}\n\
drift_metric: {metric}\n\
drift: {drift:.3}\n\
drift_penalty_lambda: {lambda:.2}\n\
mean_drift: {mean_drift:.3}\n\
summary: {summary}\n\
latest_intent: {intent}\n\
Rule: stay close to that summary. If you are ready to commit it, emit <realize>reason: why now</realize>.\n\
A non-done tool call also counts as realization. `done` does NOT; commit first, then finalize.",
        window_end = cfg.window_end,
        defer_score = latent.defer_score,
        metric = match cfg.drift_metric {
            DriftMetric::Cos => "cos",
            DriftMetric::Kl => "kl",
        },
        mean_drift = metrics.mean_drift(),
        summary = latent.summary,
        intent = latent.latest_intent.as_deref().unwrap_or("-")
    )
}

pub(crate) fn build_realize_banner(
    reason: &str,
    latency: usize,
    drift: f64,
    metrics: &RealizeMetrics,
) -> String {
    format!(
        "\n[realize] {reason} latency={latency} mean_latency={mean_latency:.2} drift={drift:.3} mean_drift={mean_drift:.3} missing={missing} leakage={leakage}\n",
        mean_latency = metrics.mean_realize_latency(),
        mean_drift = metrics.mean_drift(),
        missing = metrics.missing,
        leakage = metrics.early_leakage
    )
}

pub(crate) fn tool_call_realizes_latent(
    realize_reason: Option<&str>,
    tool_call: Option<&ToolCallData>,
) -> bool {
    if realize_reason.is_some() {
        return true;
    }
    tool_call
        .map(|tc| tc.name.as_str() != "done")
        .unwrap_or(false)
}

pub(crate) fn build_realize_done_gate_message(latent: &LatentPlanBuffer) -> String {
    format!(
        "[Realize Gate] Pending latent content must be committed before `done`.\n\
summary: {}\n\
latest_intent: {}\n\
Required now:\n\
- Emit <realize>reason: enough evidence to commit</realize>.\n\
- On the following turn, call `done` with the finalized summary/evidence.\n\
Do not call `done` while latent_pending=true.",
        compact_one_line(latent.summary.as_str(), 180),
        latent.latest_intent.as_deref().unwrap_or("-")
    )
}

pub(crate) fn build_realize_state(
    cfg: &RealizeOnDemandConfig,
    latent: Option<&LatentPlanBuffer>,
    iter: usize,
    latest_drift: Option<f64>,
    metrics: &RealizeMetrics,
) -> RealizeState {
    RealizeState {
        pending: latent.is_some(),
        age_turns: latent
            .map(|pending| iter.saturating_sub(pending.created_iter))
            .unwrap_or(0),
        window_end: cfg.window_end,
        latest_drift: if latent.is_some() { latest_drift } else { None },
        mean_drift: metrics.mean_drift(),
        mean_realize_latency: metrics.mean_realize_latency(),
        missing: metrics.missing,
        early_leakage: metrics.early_leakage,
    }
}

pub(crate) async fn emit_realize_state(
    tx: &mpsc::Sender<StreamToken>,
    cfg: &RealizeOnDemandConfig,
    latent: Option<&LatentPlanBuffer>,
    iter: usize,
    latest_drift: Option<f64>,
    metrics: &RealizeMetrics,
) {
    let _ = tx
        .send(StreamToken::RealizeState(build_realize_state(
            cfg,
            latent,
            iter,
            latest_drift,
            metrics,
        )))
        .await;
}

pub(crate) fn realize_on_demand_addon(preset: Option<RealizePreset>) -> Option<String> {
    let cfg = RealizeOnDemandConfig::resolve(preset);
    if !cfg.enabled {
        return None;
    }
    let preset_label = match preset {
        Some(preset) => preset.label(),
        None => "env",
    };
    Some(format!(
        "Realize-on-demand (experimental) is enabled.\n\
Plan-bearing no-tool turns may be held latent before they are committed.\n\
- If you want to commit a pending latent plan/claim explicitly, emit <realize>reason: why now</realize>.\n\
- A non-done tool call also counts as realization.\n\
- `done` requires latent content to be committed first.\n\
- Stay close to the latest latent summary while it is pending; wandering raises drift.\n\
- Session preset: {}.\n\
- Current defaults: defer_threshold={:.2}, realization_window={}..{}, drift_metric={}, lambda={}..{}.",
        preset_label,
        cfg.defer_threshold,
        cfg.window_start,
        cfg.window_end,
        match cfg.drift_metric {
            DriftMetric::Cos => "cos",
            DriftMetric::Kl => "kl",
        },
        cfg.lambda_min,
        cfg.lambda_max
    ))
}
