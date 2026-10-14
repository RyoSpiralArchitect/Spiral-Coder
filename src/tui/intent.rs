#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntentUpdateKind {
    Replace,
    Refine,
    Continue,
    VagueModifier,
}

#[derive(Debug, Clone)]
pub struct IntentUpdate {
    pub kind: IntentUpdateKind,
    pub goal: Option<String>,
    pub target: Option<String>,
    pub constraints: Vec<String>,
    pub success_criteria: Vec<String>,
    pub optimization_hint: Option<String>,
    pub ambiguity: f32,
    pub confidence: f32,
    pub clarification_question: Option<String>,
    pub no_op: bool,
}

#[derive(Debug, Clone)]
pub struct IntentAnchor {
    pub revision: u64,
    pub raw_user_prompt: String,
    pub goal: String,
    pub target: Option<String>,
    pub constraints: Vec<String>,
    pub success_criteria: Vec<String>,
    pub non_goals: Vec<String>,
    pub optimization_hints: Vec<String>,
    pub ambiguity: f32,
    pub confidence: f32,
    pub requires_human_confirmation: bool,
    pub last_update_kind: IntentUpdateKind,
    pub last_update_no_op: bool,
}

fn compact_line(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    let out: String = trimmed.chars().take(max_chars).collect();
    if trimmed.chars().count() > max_chars {
        format!("{out}...")
    } else {
        out
    }
}

fn split_clauses(text: &str) -> Vec<String> {
    text.split(|ch: char| matches!(ch, '\n' | '.' | ';' | '!' | '?'))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| compact_line(s, 220))
        .collect()
}

fn contains_any(hay: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| hay.contains(needle))
}

fn first_pathish_token(text: &str) -> Option<String> {
    text.split_whitespace().find_map(|token| {
        let trimmed = token
            .trim_matches(|ch: char| matches!(ch, '`' | '"' | '\'' | ',' | ')' | '(' | '.' | ':'))
            .trim();
        if trimmed.is_empty() {
            return None;
        }
        let looks_like_path = trimmed.contains('/')
            || trimmed.ends_with(".rs")
            || trimmed.ends_with(".ts")
            || trimmed.ends_with(".tsx")
            || trimmed.ends_with(".js")
            || trimmed.ends_with(".json")
            || trimmed.ends_with(".md");
        if looks_like_path {
            Some(trimmed.to_string())
        } else {
            None
        }
    })
}

fn first_slash_command(text: &str) -> Option<String> {
    text.split_whitespace().find_map(|token| {
        let trimmed = token
            .trim_matches(|ch: char| {
                matches!(ch, '`' | '"' | '\'' | ',' | '.' | ':' | ';' | ')' | '(')
            })
            .trim();
        if trimmed.starts_with('/') && trimmed.len() > 1 {
            Some(trimmed.to_string())
        } else {
            None
        }
    })
}

fn extract_target(text: &str) -> Option<String> {
    crate::task_scope::artifact_scope(text)
        .and_then(|scope| scope.target_files.into_iter().next())
        .or_else(|| first_pathish_token(text))
        .or_else(|| first_slash_command(text))
}

fn extract_constraints(text: &str) -> Vec<String> {
    let low = text.to_ascii_lowercase();
    split_clauses(text)
        .into_iter()
        .filter(|clause| {
            let c = clause.to_ascii_lowercase();
            contains_any(
                c.as_str(),
                &[
                    "do not",
                    "don't",
                    "avoid",
                    "without",
                    "keep ",
                    "within",
                    "only",
                    "no ",
                    "but ",
                    "however",
                    "ただ",
                    "ただし",
                    "でも",
                    "壊すな",
                    "触るな",
                ],
            ) || (low.contains("do not edit") && c.contains("edit"))
        })
        .take(6)
        .collect()
}

fn extract_success_criteria(text: &str) -> Vec<String> {
    split_clauses(text)
        .into_iter()
        .filter(|clause| {
            let c = clause.to_ascii_lowercase();
            contains_any(
                c.as_str(),
                &[
                    "must",
                    "include",
                    "identify",
                    "confirm",
                    "locate",
                    "report",
                    "final answer",
                    "success",
                ],
            )
        })
        .take(6)
        .collect()
}

fn extract_goal(text: &str) -> Option<String> {
    let goal = compact_line(text, 220);
    if goal.is_empty() {
        None
    } else {
        Some(goal)
    }
}

fn normalize_vague_hint(text: &str) -> Option<String> {
    let trimmed = compact_line(text, 160);
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn push_unique_capped(items: &mut Vec<String>, value: String, cap: usize) {
    if items
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(&value))
    {
        return;
    }
    items.push(value);
    if items.len() > cap {
        items.drain(0..items.len() - cap);
    }
}

fn section_or_dash(items: &[String], cap: usize) -> String {
    if items.is_empty() {
        "-".to_string()
    } else {
        items
            .iter()
            .take(cap)
            .map(|item| compact_line(item, 96))
            .collect::<Vec<_>>()
            .join(" ; ")
    }
}

pub fn normalize_intent_update(text: &str, prev: Option<&IntentAnchor>) -> IntentUpdate {
    let trimmed = text.trim();
    let low = trimmed.to_ascii_lowercase();
    let target = extract_target(trimmed);
    let constraints = extract_constraints(trimmed);
    let success_criteria = extract_success_criteria(trimmed);

    let continue_like = contains_any(
        low.as_str(),
        &[
            "continue",
            "keep going",
            "go on",
            "same direction",
            "続けて",
            "そのまま",
        ],
    );
    let vague_like = contains_any(
        low.as_str(),
        &[
            "more polish",
            "polish it",
            "improve it",
            "make it better",
            "better",
            "clean it up",
            "tighten it",
            "more robust",
            "整えて",
            "もっとよく",
            "改善して",
            "なんか変",
        ],
    );
    let refine_like = !constraints.is_empty()
        || contains_any(
            low.as_str(),
            &[
                "but ",
                "however",
                "except",
                "keep ",
                "within",
                "only",
                "ただ",
                "ただし",
                "でも",
            ],
        );

    let kind = if prev.is_none() {
        IntentUpdateKind::Replace
    } else if continue_like {
        IntentUpdateKind::Continue
    } else if vague_like
        && target.is_none()
        && constraints.is_empty()
        && success_criteria.is_empty()
    {
        IntentUpdateKind::VagueModifier
    } else if refine_like {
        IntentUpdateKind::Refine
    } else {
        IntentUpdateKind::Replace
    };

    let (ambiguity, confidence) = match kind {
        IntentUpdateKind::Replace => {
            let mut ambiguity = 0.18;
            let mut confidence = 0.88;
            if target.is_none() {
                ambiguity += 0.10;
                confidence -= 0.12;
            }
            (ambiguity, confidence)
        }
        IntentUpdateKind::Refine => {
            let mut ambiguity = 0.24;
            let mut confidence = 0.80;
            if constraints.is_empty() && success_criteria.is_empty() {
                ambiguity += 0.12;
                confidence -= 0.10;
            }
            (ambiguity, confidence)
        }
        IntentUpdateKind::Continue => (0.05, 0.96),
        IntentUpdateKind::VagueModifier => (0.72, 0.62),
    };

    let clarification_question = if ambiguity >= 0.85 && confidence <= 0.45 {
        Some("What should improve within the current scope, in one sentence?".to_string())
    } else {
        None
    };

    IntentUpdate {
        kind,
        goal: match kind {
            IntentUpdateKind::Replace => extract_goal(trimmed),
            IntentUpdateKind::Refine => None,
            IntentUpdateKind::Continue => None,
            IntentUpdateKind::VagueModifier => None,
        },
        target,
        constraints,
        success_criteria,
        optimization_hint: if matches!(kind, IntentUpdateKind::VagueModifier) {
            normalize_vague_hint(trimmed)
        } else {
            None
        },
        ambiguity,
        confidence,
        clarification_question,
        no_op: matches!(kind, IntentUpdateKind::Continue),
    }
}

pub fn apply_intent_update(
    prev: Option<&IntentAnchor>,
    upd: IntentUpdate,
    raw_user_prompt: &str,
) -> IntentAnchor {
    match upd.kind {
        IntentUpdateKind::Replace => IntentAnchor {
            revision: prev.map(|a| a.revision + 1).unwrap_or(1),
            raw_user_prompt: raw_user_prompt.trim().to_string(),
            goal: upd.goal.unwrap_or_else(|| "unspecified".to_string()),
            target: upd.target,
            constraints: upd.constraints,
            success_criteria: upd.success_criteria,
            non_goals: Vec::new(),
            optimization_hints: upd.optimization_hint.into_iter().collect(),
            ambiguity: upd.ambiguity,
            confidence: upd.confidence,
            requires_human_confirmation: upd.clarification_question.is_some(),
            last_update_kind: upd.kind,
            last_update_no_op: upd.no_op,
        },
        IntentUpdateKind::Refine => {
            let mut a = prev.cloned().unwrap_or(IntentAnchor {
                revision: 0,
                raw_user_prompt: String::new(),
                goal: "unspecified".to_string(),
                target: None,
                constraints: Vec::new(),
                success_criteria: Vec::new(),
                non_goals: Vec::new(),
                optimization_hints: Vec::new(),
                ambiguity: 1.0,
                confidence: 0.0,
                requires_human_confirmation: false,
                last_update_kind: IntentUpdateKind::Replace,
                last_update_no_op: false,
            });
            if let Some(goal) = upd.goal {
                a.goal = goal;
            }
            if upd.target.is_some() {
                a.target = upd.target;
            }
            for item in upd.constraints {
                push_unique_capped(&mut a.constraints, item, 8);
            }
            for item in upd.success_criteria {
                push_unique_capped(&mut a.success_criteria, item, 8);
            }
            a.raw_user_prompt = raw_user_prompt.trim().to_string();
            a.ambiguity = a.ambiguity.max(upd.ambiguity);
            a.confidence = a.confidence.min(upd.confidence);
            a.requires_human_confirmation |= upd.clarification_question.is_some();
            a.revision += 1;
            a.last_update_kind = IntentUpdateKind::Refine;
            a.last_update_no_op = upd.no_op;
            a
        }
        IntentUpdateKind::Continue => {
            let mut a = prev.cloned().unwrap_or(IntentAnchor {
                revision: 0,
                raw_user_prompt: String::new(),
                goal: extract_goal(raw_user_prompt).unwrap_or_else(|| "unspecified".to_string()),
                target: extract_target(raw_user_prompt),
                constraints: extract_constraints(raw_user_prompt),
                success_criteria: extract_success_criteria(raw_user_prompt),
                non_goals: Vec::new(),
                optimization_hints: Vec::new(),
                ambiguity: 0.20,
                confidence: 0.75,
                requires_human_confirmation: false,
                last_update_kind: IntentUpdateKind::Replace,
                last_update_no_op: false,
            });
            a.raw_user_prompt = raw_user_prompt.trim().to_string();
            a.revision += 1;
            a.last_update_kind = IntentUpdateKind::Continue;
            a.last_update_no_op = upd.no_op;
            a
        }
        IntentUpdateKind::VagueModifier => {
            let mut a = prev.cloned().unwrap_or(IntentAnchor {
                revision: 0,
                raw_user_prompt: String::new(),
                goal: extract_goal(raw_user_prompt).unwrap_or_else(|| "unspecified".to_string()),
                target: extract_target(raw_user_prompt),
                constraints: extract_constraints(raw_user_prompt),
                success_criteria: extract_success_criteria(raw_user_prompt),
                non_goals: Vec::new(),
                optimization_hints: Vec::new(),
                ambiguity: 0.20,
                confidence: 0.75,
                requires_human_confirmation: false,
                last_update_kind: IntentUpdateKind::Replace,
                last_update_no_op: false,
            });
            if let Some(hint) = upd.optimization_hint {
                push_unique_capped(&mut a.optimization_hints, hint, 6);
            }
            a.raw_user_prompt = raw_user_prompt.trim().to_string();
            a.ambiguity = a.ambiguity.max(upd.ambiguity);
            a.confidence = a.confidence.min(upd.confidence);
            a.requires_human_confirmation |= upd.clarification_question.is_some();
            a.revision += 1;
            a.last_update_kind = IntentUpdateKind::VagueModifier;
            a.last_update_no_op = upd.no_op;
            a
        }
    }
}

pub fn render_intent_anchor(anchor: &IntentAnchor) -> String {
    let mut out = String::from("[Intent Anchor]\n");
    out.push_str(&format!("revision: {}\n", anchor.revision));
    out.push_str(&format!("update_kind: {:?}\n", anchor.last_update_kind));
    out.push_str(&format!("update_no_op: {}\n", anchor.last_update_no_op));
    out.push_str(&format!("baseline: {}\n", anchor_baseline(anchor)));
    out.push_str(&format!("goal: {}\n", anchor.goal));
    out.push_str(&format!(
        "target: {}\n",
        anchor.target.as_deref().unwrap_or("-")
    ));
    out.push_str("constraints:\n");
    if anchor.constraints.is_empty() {
        out.push_str("- none\n");
    } else {
        for item in &anchor.constraints {
            out.push_str("- ");
            out.push_str(item);
            out.push('\n');
        }
    }
    out.push_str("success:\n");
    if anchor.success_criteria.is_empty() {
        out.push_str("- none\n");
    } else {
        for item in &anchor.success_criteria {
            out.push_str("- ");
            out.push_str(item);
            out.push('\n');
        }
    }
    out.push_str("non_goals:\n");
    if anchor.non_goals.is_empty() {
        out.push_str("- none\n");
    } else {
        for item in &anchor.non_goals {
            out.push_str("- ");
            out.push_str(item);
            out.push('\n');
        }
    }
    out.push_str("optimization_hints:\n");
    if anchor.optimization_hints.is_empty() {
        out.push_str("- none\n");
    } else {
        for item in &anchor.optimization_hints {
            out.push_str("- ");
            out.push_str(item);
            out.push('\n');
        }
    }
    out.push_str(&format!("ambiguity: {:.2}\n", anchor.ambiguity));
    out.push_str(&format!("confidence: {:.2}\n", anchor.confidence));
    out.push_str(&format!(
        "requires_human_confirmation: {}\n",
        anchor.requires_human_confirmation
    ));
    out.push_str("Rules:\n");
    out.push_str("- Stay within this goal/target unless the user explicitly replaces scope.\n");
    out.push_str("- Treat optimization_hints as modifiers, not a new task.\n");
    out.push_str("- Do not widen into unrelated refactors.\n");
    out
}

pub fn anchor_baseline(anchor: &IntentAnchor) -> String {
    format!(
        "goal: {} | target: {} | constraints: {} | success: {} | opt: {}",
        compact_line(anchor.goal.as_str(), 120),
        anchor
            .target
            .as_deref()
            .map(|s| compact_line(s, 96))
            .unwrap_or_else(|| "-".to_string()),
        section_or_dash(&anchor.constraints, 3),
        section_or_dash(&anchor.success_criteria, 3),
        section_or_dash(&anchor.optimization_hints, 3),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vague_modifier_keeps_scope_and_adds_hint() {
        let prev = IntentAnchor {
            revision: 1,
            raw_user_prompt: "Find /realize handler".to_string(),
            goal: "Locate the /realize handler".to_string(),
            target: Some("src/tui/events.rs".to_string()),
            constraints: vec!["Do not edit anything".to_string()],
            success_criteria: vec!["Final answer includes the file path".to_string()],
            non_goals: Vec::new(),
            optimization_hints: Vec::new(),
            ambiguity: 0.15,
            confidence: 0.85,
            requires_human_confirmation: false,
            last_update_kind: IntentUpdateKind::Replace,
            last_update_no_op: false,
        };
        let upd = normalize_intent_update("make it a bit better", Some(&prev));
        assert_eq!(upd.kind, IntentUpdateKind::VagueModifier);
        let next = apply_intent_update(Some(&prev), upd, "make it a bit better");
        assert_eq!(next.goal, prev.goal);
        assert_eq!(next.target, prev.target);
        assert!(!next.optimization_hints.is_empty());
    }

    #[test]
    fn continue_marks_no_op() {
        let upd = normalize_intent_update("continue", None);
        assert_eq!(upd.kind, IntentUpdateKind::Replace);
        let prev = IntentAnchor {
            revision: 1,
            raw_user_prompt: "Find /realize handler".to_string(),
            goal: "Locate the /realize handler".to_string(),
            target: Some("src/tui/events.rs".to_string()),
            constraints: Vec::new(),
            success_criteria: Vec::new(),
            non_goals: Vec::new(),
            optimization_hints: Vec::new(),
            ambiguity: 0.15,
            confidence: 0.85,
            requires_human_confirmation: false,
            last_update_kind: IntentUpdateKind::Replace,
            last_update_no_op: false,
        };
        let upd2 = normalize_intent_update("continue", Some(&prev));
        assert_eq!(upd2.kind, IntentUpdateKind::Continue);
        assert!(upd2.no_op);
    }

    #[test]
    fn refine_extracts_constraints() {
        let prev = IntentAnchor {
            revision: 1,
            raw_user_prompt: "Find /realize handler".to_string(),
            goal: "Locate the /realize handler".to_string(),
            target: Some("src/tui/events.rs".to_string()),
            constraints: Vec::new(),
            success_criteria: Vec::new(),
            non_goals: Vec::new(),
            optimization_hints: Vec::new(),
            ambiguity: 0.15,
            confidence: 0.85,
            requires_human_confirmation: false,
            last_update_kind: IntentUpdateKind::Replace,
            last_update_no_op: false,
        };
        let upd =
            normalize_intent_update("but do not widen scope and keep it read-only", Some(&prev));
        assert_eq!(upd.kind, IntentUpdateKind::Refine);
        let next = apply_intent_update(
            Some(&prev),
            upd,
            "but do not widen scope and keep it read-only",
        );
        assert!(!next.constraints.is_empty());
    }

    #[test]
    fn anchor_baseline_stays_scoped() {
        let anchor = IntentAnchor {
            revision: 2,
            raw_user_prompt: "make it better".to_string(),
            goal: "Locate the /realize handler".to_string(),
            target: Some("src/tui/events.rs".to_string()),
            constraints: vec!["Do not edit anything".to_string()],
            success_criteria: vec!["Final answer includes the file path".to_string()],
            non_goals: Vec::new(),
            optimization_hints: vec!["improve readability within current scope".to_string()],
            ambiguity: 0.24,
            confidence: 0.78,
            requires_human_confirmation: false,
            last_update_kind: IntentUpdateKind::VagueModifier,
            last_update_no_op: false,
        };
        let baseline = anchor_baseline(&anchor);
        assert!(baseline.contains("goal: Locate the /realize handler"));
        assert!(baseline.contains("target: src/tui/events.rs"));
        assert!(baseline.contains("opt: improve readability within current scope"));
    }
}
