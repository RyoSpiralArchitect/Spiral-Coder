use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
    Frame,
};

use crate::config::{
    provider_preset_for_run, provider_preset_keys, representative_models_for_run, ProviderKind,
    RunConfig,
};
use crate::harness_gate::HarnessPromotionBoardStatus;
use crate::merge_gate::MergeGateBoardStatus;

use super::app::{App, Focus, Message, RightTab, Role, TaskPhase, TaskTarget};
use super::merge_gate;
use super::promotion_gate;

// ── Brand palette (mirrors web UI) ────────────────────────────────────────────

const CODER_BLUE: Color = Color::Rgb(96, 165, 250); // blue-400
const OBS_MAG: Color = Color::Rgb(217, 70, 239); // fuchsia-500
const ACCENT: Color = Color::Rgb(45, 212, 191); // teal-400
const WARN: Color = Color::Rgb(251, 191, 36); // amber-400
const DANGER: Color = Color::Rgb(248, 113, 113); // red-400
const SUCCESS: Color = Color::Rgb(74, 222, 128); // green-400
const PROMO: Color = Color::Rgb(129, 140, 248); // indigo-400
const MERGE: Color = Color::Rgb(56, 189, 248); // sky-400
const ACTION: Color = Color::Rgb(163, 230, 53); // lime-400
const MUTED: Color = Color::Rgb(100, 116, 139); // slate-500
const TEXT_BODY: Color = Color::Rgb(226, 232, 240); // slate-200
const BG_DARK: Color = Color::Rgb(15, 23, 42); // slate-950
const UNFOCUSED: Color = Color::Rgb(51, 65, 85); // slate-700

// ── Animation ─────────────────────────────────────────────────────────────────

const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧'];

fn spinner_char(tick: u64) -> char {
    SPINNER[(tick as usize / 2) % SPINNER.len()]
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum PaneView {
    Coder,
    Observer,
    Chat,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ActivePicker {
    Provider,
    Model,
}

pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // header (2 rows)
            Constraint::Min(1),
            Constraint::Length(4), // input box
            Constraint::Length(1), // footer shortcuts
        ])
        .split(area);

    render_header(frame, vert[0], app);
    render_body(frame, vert[1], app);
    render_input(frame, vert[2], app);
    render_footer(frame, vert[3], app);
}

// ── Header ────────────────────────────────────────────────────────────────────

fn iter_progress(n: u32) -> String {
    const MAX: u32 = 12;
    let n = n.min(MAX) as usize;
    let bar: String = (0..MAX as usize)
        .map(|i| if i < n { '█' } else { '░' })
        .collect();
    format!(" {bar} {n}/{MAX}")
}

fn render_header(frame: &mut Frame, area: Rect, app: &App) {
    let c_spin = if app.coder.streaming {
        format!(" {}", spinner_char(app.tick_count))
    } else {
        String::new()
    };
    let iter = if app.coder_iter > 0 {
        iter_progress(app.coder_iter)
    } else {
        String::new()
    };

    let c_m = truncate_model(&app.coder_cfg.model, 20);
    let c_p = provider_abbrev(&app.coder_cfg);
    let c_mode = truncate_model(app.coder_cfg.mode.label(), 8);
    let (right_label, right_model, right_provider, right_mode, right_spin, tabs_badge, right_brand) =
        match app.right_tab {
            RightTab::Observer => (
                "OBS",
                truncate_model(&app.observer_cfg.model, 20),
                provider_abbrev(&app.observer_cfg),
                truncate_model(app.observer_cfg.mode.label(), 8),
                if app.observer.streaming {
                    format!(" {}", spinner_char(app.tick_count))
                } else {
                    String::new()
                },
                "TABS:[OBS] CHAT TASKS REVIEW MERGE".to_string(),
                OBS_MAG,
            ),
            RightTab::Chat => (
                "CHAT",
                truncate_model(&app.chat_cfg.model, 20),
                provider_abbrev(&app.chat_cfg),
                truncate_model(app.chat_cfg.mode.label(), 8),
                if app.chat.streaming {
                    format!(" {}", spinner_char(app.tick_count))
                } else {
                    String::new()
                },
                "TABS: OBS [CHAT] TASKS REVIEW MERGE".to_string(),
                ACCENT,
            ),
            RightTab::Tasks => (
                "TASKS",
                format!("{} items", app.tasks.len()),
                "---",
                "read-only".to_string(),
                if app.planning_tasks {
                    format!(" {}", spinner_char(app.tick_count))
                } else {
                    String::new()
                },
                "TABS: OBS CHAT [TASKS] REVIEW MERGE".to_string(),
                WARN,
            ),
            RightTab::Promotions => (
                "REVIEW",
                format!(
                    "{} queued",
                    app.harness_promotions.summary.needs_review
                        + app.harness_promotions.summary.approved
                ),
                "---",
                "human-gate".to_string(),
                String::new(),
                "TABS: OBS CHAT TASKS [REVIEW] MERGE".to_string(),
                PROMO,
            ),
            RightTab::MergeGate => (
                "MERGE",
                format!(
                    "{} queued",
                    app.merge_gate.summary.needs_review
                        + app.merge_gate.summary.rollback_available
                        + app.merge_gate.summary.blocked
                ),
                "---",
                "human-gate".to_string(),
                String::new(),
                "TABS: OBS CHAT TASKS REVIEW [MERGE]".to_string(),
                MERGE,
            ),
        };
    let key_badge = if active_key_missing(app) {
        Span::styled(
            "  KEYS?",
            Style::default().fg(WARN).add_modifier(Modifier::BOLD),
        )
    } else {
        Span::raw("")
    };

    let row1 = Line::from(vec![
        Span::styled(
            "  ◈ Spiral-Coder ",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("│ ", Style::default().fg(UNFOCUSED)),
        Span::styled("C: ", Style::default().fg(MUTED)),
        Span::styled(
            format!("{c_m}{c_spin}"),
            Style::default().fg(CODER_BLUE).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("@{c_p}"), Style::default().fg(MUTED)),
        Span::styled(format!(" ·{c_mode}"), Style::default().fg(MUTED)),
        Span::styled(iter, Style::default().fg(ACCENT)),
        Span::styled("  │  R: ", Style::default().fg(MUTED)),
        Span::styled(
            format!("{right_label} "),
            Style::default().fg(MUTED).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{right_model}{right_spin}"),
            Style::default()
                .fg(right_brand)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            if right_provider == "---" {
                String::new()
            } else {
                format!("@{right_provider}")
            },
            Style::default().fg(MUTED),
        ),
        Span::styled(
            if right_mode.is_empty() {
                String::new()
            } else {
                format!(" ·{right_mode}")
            },
            Style::default().fg(MUTED),
        ),
        if app.auto_observe {
            Span::styled(
                "  ◉ AUTO",
                Style::default().fg(WARN).add_modifier(Modifier::BOLD),
            )
        } else {
            Span::raw("")
        },
        if app.auto_fix_mode {
            Span::styled(
                "  ◈ FIX",
                Style::default().fg(SUCCESS).add_modifier(Modifier::BOLD),
            )
        } else {
            Span::raw("")
        },
        key_badge,
        Span::styled(
            format!("  LANG: {}", app.lang.to_ascii_uppercase()),
            Style::default().fg(MUTED),
        ),
        Span::styled(format!("  {tabs_badge}"), Style::default().fg(MUTED)),
        Span::styled(
            app.project_stack_label
                .as_deref()
                .map(|l| format!("  ▸ {l}"))
                .unwrap_or_default(),
            Style::default().fg(ACCENT),
        ),
    ]);

    let row2 = Line::from(Span::styled(
        "  Tab=focus  Ctrl+R=right tab  Ctrl+A=auto  Ctrl+K=cancel  Ctrl+O=review  /=commands  /help=all",
        Style::default().fg(MUTED),
    ));

    frame.render_widget(
        Paragraph::new(Text::from(vec![row1, row2])).style(Style::default().bg(BG_DARK)),
        area,
    );
}

fn truncate_model(name: &str, max: usize) -> String {
    if name.chars().count() <= max {
        name.to_string()
    } else {
        format!("{}…", name.chars().take(max - 1).collect::<String>())
    }
}

fn provider_abbrev(cfg: &RunConfig) -> &'static str {
    match provider_preset_for_run(cfg) {
        crate::config::ProviderPreset::OpenAi => "oai",
        crate::config::ProviderPreset::Gemini => "gem",
        crate::config::ProviderPreset::AnthropicCompat => "cla",
        crate::config::ProviderPreset::OpenAiCompatibleCustom => "oac",
        crate::config::ProviderPreset::Mistral => "mis",
        crate::config::ProviderPreset::Anthropic => "ant",
        crate::config::ProviderPreset::HfLocal => "hf",
    }
}

fn active_key_missing(app: &App) -> bool {
    let cfg = match app.focus {
        Focus::Coder => &app.coder_cfg,
        Focus::Right => match app.right_tab {
            RightTab::Observer => &app.observer_cfg,
            RightTab::Chat => &app.chat_cfg,
            RightTab::Tasks | RightTab::Promotions | RightTab::MergeGate => return false,
        },
    };
    provider_requires_key(&cfg.provider) && cfg.api_key.is_none()
}

fn provider_requires_key(provider: &ProviderKind) -> bool {
    !matches!(provider, ProviderKind::Hf)
}

// ── Body: two-pane split ──────────────────────────────────────────────────────

fn render_body(frame: &mut Frame, area: Rect, app: &App) {
    // Give the focused side more space; long Observer critiques are otherwise painful to read.
    let (left_pct, right_pct) = if app.focus == Focus::Right {
        (40u16, 60u16)
    } else {
        (55u16, 45u16)
    };

    let horiz = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(left_pct),
            Constraint::Percentage(right_pct),
        ])
        .split(area);

    render_message_pane(
        frame,
        horiz[0],
        app,
        PaneView::Coder,
        app.focus == Focus::Coder,
    );

    let right_focused = app.focus == Focus::Right;
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(horiz[1]);
    render_right_tab_bar(frame, right[0], app, right_focused);
    match app.right_tab {
        RightTab::Observer => {
            render_message_pane(frame, right[1], app, PaneView::Observer, right_focused);
        }
        RightTab::Chat => {
            render_message_pane(frame, right[1], app, PaneView::Chat, right_focused);
        }
        RightTab::Tasks => {
            render_tasks_pane(frame, right[1], app, right_focused);
        }
        RightTab::Promotions => {
            render_promotions_pane(frame, right[1], app, right_focused);
        }
        RightTab::MergeGate => {
            render_merge_gate_pane(frame, right[1], app, right_focused);
        }
    }
}

fn render_right_tab_bar(frame: &mut Frame, area: Rect, app: &App, focused: bool) {
    let style_for = |tab: RightTab, color: Color| {
        if app.right_tab == tab {
            Style::default().fg(color).add_modifier(Modifier::BOLD)
        } else if focused {
            Style::default().fg(TEXT_BODY)
        } else {
            Style::default().fg(MUTED)
        }
    };
    let line = Line::from(vec![
        Span::styled("  Right Pane ", Style::default().fg(MUTED)),
        Span::styled("[Observer]", style_for(RightTab::Observer, OBS_MAG)),
        Span::raw("  "),
        Span::styled("[Chat]", style_for(RightTab::Chat, ACCENT)),
        Span::raw("  "),
        Span::styled("[Tasks]", style_for(RightTab::Tasks, WARN)),
        Span::raw("  "),
        Span::styled(
            if app.harness_promotions.summary.needs_review + app.harness_promotions.summary.approved
                > 0
            {
                format!(
                    "[Review {}]",
                    app.harness_promotions.summary.needs_review
                        + app.harness_promotions.summary.approved
                )
            } else {
                "[Review]".to_string()
            },
            style_for(RightTab::Promotions, PROMO),
        ),
        Span::raw("  "),
        Span::styled(
            if app.merge_gate.summary.needs_review
                + app.merge_gate.summary.rollback_available
                + app.merge_gate.summary.blocked
                > 0
            {
                format!(
                    "[Merge {}]",
                    app.merge_gate.summary.needs_review
                        + app.merge_gate.summary.rollback_available
                        + app.merge_gate.summary.blocked
                )
            } else {
                "[Merge]".to_string()
            },
            style_for(RightTab::MergeGate, MERGE),
        ),
        Span::styled("   Ctrl+R or /tab", Style::default().fg(MUTED)),
    ]);
    frame.render_widget(
        Paragraph::new(line).style(Style::default().bg(BG_DARK)),
        area,
    );
}

fn render_message_pane(frame: &mut Frame, area: Rect, app: &App, view: PaneView, focused: bool) {
    let (pane, brand, label) = match view {
        PaneView::Coder => (&app.coder, CODER_BLUE, "CODER"),
        PaneView::Observer => (&app.observer, OBS_MAG, "OBSERVER"),
        PaneView::Chat => (&app.chat, ACCENT, "CHAT"),
    };
    let prov = match view {
        PaneView::Coder => provider_abbrev(&app.coder_cfg),
        PaneView::Observer => provider_abbrev(&app.observer_cfg),
        PaneView::Chat => provider_abbrev(&app.chat_cfg),
    };
    let (mode, persona) = match view {
        PaneView::Coder => (app.coder_cfg.mode.label(), app.coder_cfg.persona.as_str()),
        PaneView::Observer => (
            app.observer_cfg.mode.label(),
            app.observer_cfg.persona.as_str(),
        ),
        PaneView::Chat => (app.chat_cfg.mode.label(), app.chat_cfg.persona.as_str()),
    };

    let border_style = if focused {
        Style::default().fg(brand)
    } else {
        Style::default().fg(UNFOCUSED)
    };

    let spin = if pane.streaming {
        format!(" {}", spinner_char(app.tick_count))
    } else {
        String::new()
    };
    let scroll_badge = if pane.scroll > 0 {
        format!(" ↑{}", pane.scroll.min(9999))
    } else {
        String::new()
    };
    let find_badge = if !pane.find_query.trim().is_empty() {
        let q0 = pane.find_query.trim();
        let mut q = q0.to_string();
        if q.chars().count() > 18 {
            q = q.chars().take(18).collect::<String>() + "…";
        }
        format!(" /find:{q}")
    } else {
        String::new()
    };
    let focus_dot = if focused { "◉" } else { "○" };
    // `label` is determined by PaneView.

    let mut spans = vec![
        Span::raw(" "),
        Span::styled(focus_dot, Style::default().fg(brand)),
        Span::raw(" "),
        Span::styled(
            label,
            Style::default().fg(brand).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" @{prov}"), Style::default().fg(MUTED)),
        Span::styled(
            format!(" mode:{}", truncate_model(mode, 10)),
            Style::default().fg(MUTED),
        ),
        Span::styled(
            format!(" persona:{}", truncate_model(persona, 12)),
            Style::default().fg(MUTED),
        ),
        Span::styled(spin, Style::default().fg(ACCENT)),
        Span::styled(scroll_badge, Style::default().fg(WARN)),
        Span::styled(find_badge, Style::default().fg(MUTED)),
    ];

    if view == PaneView::Coder {
        let realize_color = match app.coder_realize_preset {
            super::agent::RealizePreset::Off => MUTED,
            super::agent::RealizePreset::Low => SUCCESS,
            super::agent::RealizePreset::Mid => ACCENT,
            super::agent::RealizePreset::High => WARN,
        };
        spans.push(Span::styled(
            format!(" rz:{}", app.coder_realize_preset.label()),
            Style::default().fg(realize_color),
        ));
        if let Some(ref rz) = app.coder_realize_state {
            let status = if rz.pending {
                format!(
                    " latent:{}/{} d:{:.2}",
                    rz.age_turns.min(rz.window_end),
                    rz.window_end,
                    rz.latest_drift.unwrap_or(rz.mean_drift)
                )
            } else if rz.mean_drift > 0.0 || rz.mean_realize_latency > 0.0 {
                format!(" rzμ:{:.2}/l{:.1}", rz.mean_drift, rz.mean_realize_latency)
            } else {
                String::new()
            };
            if !status.is_empty() {
                let status_color = if rz.pending {
                    if rz.latest_drift.unwrap_or(0.0) >= 0.45 {
                        WARN
                    } else {
                        MUTED
                    }
                } else {
                    MUTED
                };
                spans.push(Span::styled(status, Style::default().fg(status_color)));
            }
        }
        if let Some(ref g) = app.coder_governor {
            if let Some(ref stage) = g.recovery_stage {
                spans.push(Span::styled(
                    format!(" rec:{stage}"),
                    Style::default().fg(WARN).add_modifier(Modifier::BOLD),
                ));
            }
            if g.done_verify_required {
                spans.push(Span::styled(
                    " VERIFY!",
                    Style::default().fg(DANGER).add_modifier(Modifier::BOLD),
                ));
            }
            if g.same_command_repeats >= 2 {
                let col = if g.same_command_repeats >= 3 {
                    DANGER
                } else {
                    WARN
                };
                spans.push(Span::styled(
                    format!(" same_cmd:{}", g.same_command_repeats),
                    Style::default().fg(col).add_modifier(Modifier::BOLD),
                ));
            }
            if let Some(ref r) = g.last_reflection {
                let goal = r.goal_delta.as_deref().unwrap_or("?");
                let strat = r.strategy_change.as_deref().unwrap_or("?");
                spans.push(Span::styled(
                    format!(" refl:{goal}/{strat}"),
                    Style::default().fg(MUTED),
                ));
            }
        }
    }

    spans.push(Span::raw(" "));

    let title = Line::from(spans);

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let input_is_empty = pane.textarea.lines().join("\n").trim().is_empty();
    if pane.messages.is_empty() && !pane.welcome_dismissed && input_is_empty {
        render_welcome(frame, inner, app, view);
        return;
    }

    let mut lines: Vec<Line> = Vec::new();

    let q = pane.find_query.trim();
    let messages_view: Vec<&Message> = if q.is_empty() {
        pane.messages.iter().collect()
    } else {
        let ql = q.to_ascii_lowercase();
        pane.messages
            .iter()
            .filter(|m| m.content.to_ascii_lowercase().contains(&ql))
            .collect()
    };

    if messages_view.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (no matches)  /find <text> filters, /find clears",
            Style::default().fg(MUTED),
        )));
        lines.push(Line::default());
    }

    for msg in messages_view {
        match msg.role {
            Role::User => {
                lines.push(Line::from(vec![
                    Span::styled(
                        "  you ",
                        Style::default().fg(SUCCESS).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("›", Style::default().fg(MUTED)),
                ]));
                for l in msg.content.lines() {
                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(l.to_string(), Style::default().fg(SUCCESS)),
                    ]));
                }
            }
            Role::Assistant => {
                let (lbl, lbl_color) = match view {
                    PaneView::Coder => ("coder", CODER_BLUE),
                    PaneView::Observer => ("obs", OBS_MAG),
                    PaneView::Chat => ("chat", ACCENT),
                };
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("  {lbl} "),
                        Style::default().fg(lbl_color).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("›", Style::default().fg(MUTED)),
                ]));
                let content_lines = match view {
                    PaneView::Coder => render_coder_content(&msg.content),
                    PaneView::Observer => render_observer_content(&msg.content),
                    PaneView::Chat => render_coder_content(&msg.content),
                };
                lines.extend(content_lines);
            }
            Role::Tool => {
                // Tool messages already formatted as "[TOOL] cmd" / "[RESULT] …"
                lines.extend(render_coder_content(&msg.content));
            }
        }

        // Streaming cursor.
        if !msg.complete {
            lines.push(Line::from(Span::styled(
                format!("  {} ", spinner_char(app.tick_count)),
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            )));
        }

        // Thin separator between messages.
        lines.push(Line::from(Span::styled(
            "  ·",
            Style::default().fg(UNFOCUSED),
        )));
        lines.push(Line::default());
    }

    // Scroll: scroll=0 pins to bottom; scroll=N shows N lines above bottom.
    let total = lines.len();
    let visible = inner.height as usize;
    let max_scroll = total.saturating_sub(visible);
    let from_bottom = pane.scroll.min(max_scroll);
    let from_top = max_scroll.saturating_sub(from_bottom);

    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((from_top as u16, 0)),
        inner,
    );
}

// ── Welcome / empty-pane hint ─────────────────────────────────────────────────

fn render_tasks_pane(frame: &mut Frame, area: Rect, app: &App, focused: bool) {
    let border_style = if focused {
        Style::default().fg(WARN)
    } else {
        Style::default().fg(UNFOCUSED)
    };

    let spin = if app.planning_tasks {
        format!(" {}", spinner_char(app.tick_count))
    } else {
        String::new()
    };
    let count = format!("  {} task(s)", app.tasks.len());

    let title = Line::from(vec![
        Span::raw(" "),
        Span::styled(
            "TASKS",
            Style::default().fg(WARN).add_modifier(Modifier::BOLD),
        ),
        Span::styled(spin, Style::default().fg(ACCENT)),
        Span::styled(count, Style::default().fg(MUTED)),
        Span::raw(" "),
    ]);

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();

    if app.tasks.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (no tasks yet) Use Chat tab to plan tasks.",
            Style::default().fg(MUTED),
        )));
        lines.push(Line::from(Span::styled(
            "  Enter=dispatch  Space=toggle done",
            Style::default().fg(MUTED),
        )));
    } else {
        let total = app.tasks.len();
        let cur = app.tasks_cursor.min(total.saturating_sub(1));

        // Window around cursor (no separate scroll state yet).
        let visible = inner.height as usize;
        let start = cur.saturating_sub(visible / 2);
        let end = (start + visible).min(total);

        for i in start..end {
            let t = &app.tasks[i];
            let selected = i == cur;
            let prefix = if selected { ">" } else { " " };
            let done = if t.done { "x" } else { " " };

            let tgt = match t.target {
                TaskTarget::Coder => "C",
                TaskTarget::Observer => "O",
            };
            let ph = match t.phase {
                TaskPhase::Core => "core",
                TaskPhase::Feature => "feat",
                TaskPhase::Polish => "pol",
                TaskPhase::Any => "any",
            };

            let mut title = t.title.clone();
            if title.chars().count() > 46 {
                title = title.chars().take(46).collect::<String>() + "...";
            }

            let line = format!(" {prefix}[{done}] {tgt} {ph} P{:02} {title}", t.priority);
            let style = if selected {
                Style::default().fg(TEXT_BODY).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(TEXT_BODY)
            };
            lines.push(Line::from(Span::styled(line, style)));
        }
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn promotion_status_style(status: HarnessPromotionBoardStatus, selected: bool) -> Style {
    let color = match status {
        HarnessPromotionBoardStatus::NeedsReview => WARN,
        HarnessPromotionBoardStatus::Approved => SUCCESS,
        HarnessPromotionBoardStatus::Held => DANGER,
        HarnessPromotionBoardStatus::Applied => PROMO,
        HarnessPromotionBoardStatus::UpToDate => ACCENT,
        HarnessPromotionBoardStatus::Blocked => MUTED,
    };
    let style = Style::default().fg(color);
    if selected {
        style.add_modifier(Modifier::BOLD)
    } else {
        style
    }
}

fn merge_gate_status_style(status: MergeGateBoardStatus, selected: bool) -> Style {
    let color = match status {
        MergeGateBoardStatus::NeedsReview => WARN,
        MergeGateBoardStatus::Approved => SUCCESS,
        MergeGateBoardStatus::Held => DANGER,
        MergeGateBoardStatus::RollbackAvailable => MERGE,
        MergeGateBoardStatus::Blocked => MUTED,
    };
    let style = Style::default().fg(color);
    if selected {
        style.add_modifier(Modifier::BOLD)
    } else {
        style
    }
}

fn render_promotions_pane(frame: &mut Frame, area: Rect, app: &App, focused: bool) {
    let border_style = if focused {
        Style::default().fg(PROMO)
    } else {
        Style::default().fg(UNFOCUSED)
    };
    let summary = &app.harness_promotions.summary;
    let title = Line::from(vec![
        Span::raw(" "),
        Span::styled(
            "REVIEW INBOX",
            Style::default().fg(PROMO).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                "  review:{} approved:{} applied:{}",
                summary.needs_review, summary.approved, summary.applied
            ),
            Style::default().fg(MUTED),
        ),
        Span::raw(" "),
    ]);

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    if app.harness_promotions.entries.is_empty() {
        lines.push(Line::from(Span::styled(
            format!(
                "  {}",
                app.harness_promotions
                    .status_message
                    .clone()
                    .unwrap_or_else(|| "no promotion candidates yet".to_string())
            ),
            Style::default().fg(MUTED),
        )));
        lines.push(Line::from(Span::styled(
            "  Run `spiral-coder promote-harness` to generate a candidate artifact.",
            Style::default().fg(MUTED),
        )));
    } else {
        let total = app.harness_promotions.entries.len();
        let cur = app.harness_promotions_cursor.min(total.saturating_sub(1));
        let (start, end) =
            promotion_gate::visible_window(&app.harness_promotions, cur, inner.height as usize);

        for i in start..end {
            let entry = &app.harness_promotions.entries[i];
            let selected = i == cur;
            let prefix = if selected { ">" } else { " " };
            let mut title = entry.title.clone();
            if title.chars().count() > 34 {
                title = title.chars().take(34).collect::<String>() + "...";
            }
            lines.push(Line::from(Span::styled(
                format!(
                    " {prefix}[{}] {:<6} {}",
                    entry.review_badge, entry.badge, title
                ),
                promotion_status_style(entry.review_status, selected),
            )));
            lines.push(Line::from(Span::styled(
                format!("   {}", entry.subtitle),
                Style::default().fg(MUTED),
            )));
        }
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn render_merge_gate_pane(frame: &mut Frame, area: Rect, app: &App, focused: bool) {
    let border_style = if focused {
        Style::default().fg(MERGE)
    } else {
        Style::default().fg(UNFOCUSED)
    };
    let summary = &app.merge_gate.summary;
    let title = Line::from(vec![
        Span::raw(" "),
        Span::styled(
            "MERGE GATE",
            Style::default().fg(MERGE).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                "  review:{} approved:{} rollback:{} blocked:{}",
                summary.needs_review, summary.approved, summary.rollback_available, summary.blocked
            ),
            Style::default().fg(MUTED),
        ),
        Span::raw(" "),
    ]);

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    if app.merge_gate.entries.is_empty() {
        lines.push(Line::from(Span::styled(
            format!(
                "  {}",
                app.merge_gate
                    .status_message
                    .clone()
                    .unwrap_or_else(|| "no merge gate artifact yet".to_string())
            ),
            Style::default().fg(MUTED),
        )));
        lines.push(Line::from(Span::styled(
            "  Run `spiral-coder eval --spec .spiral-coder/runtime_eval.json` to generate merge_gate.json.",
            Style::default().fg(MUTED),
        )));
    } else {
        let total = app.merge_gate.entries.len();
        let cur = app.merge_gate_cursor.min(total.saturating_sub(1));
        let (start, end) = merge_gate::visible_window(&app.merge_gate, cur, inner.height as usize);

        for i in start..end {
            let entry = &app.merge_gate.entries[i];
            let selected = i == cur;
            let prefix = if selected { ">" } else { " " };
            let mut root = entry.root.clone();
            if root.chars().count() > 36 {
                root = "…".to_string()
                    + &root
                        .chars()
                        .rev()
                        .take(35)
                        .collect::<String>()
                        .chars()
                        .rev()
                        .collect::<String>();
            }
            lines.push(Line::from(Span::styled(
                format!(
                    " {prefix}[{}] {:<7} {}",
                    entry.review_badge, entry.case_status, entry.id
                ),
                merge_gate_status_style(entry.review_status, selected),
            )));
            lines.push(Line::from(Span::styled(
                format!("   {}  {}", root, merge_gate::primary_action_hint(entry)),
                Style::default().fg(MUTED),
            )));
        }
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn key_row(key: &'static str, desc: &'static str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {key:<18}"), Style::default().fg(ACCENT)),
        Span::styled(desc, Style::default().fg(MUTED)),
    ])
}

fn render_welcome(frame: &mut Frame, area: Rect, app: &App, view: PaneView) {
    let (brand, heading, hint1, hint2, cfg, extra, quick_start) = match view {
        PaneView::Coder => (
            CODER_BLUE,
            " ◈ CODER",
            "Describe the coding task, then press Enter.",
            "Chat lives on the right pane by default. Use Ctrl+R or /tab observer|chat|tasks|promotions|merge.",
            &app.coder_cfg,
            "Coder defaults to a coding-first mode in the TUI.",
            [
                "1) Run /keys if your provider needs an API key",
                "2) Type the task here and press Enter",
                "3) Use Ctrl+R for Chat / Observer / Tasks / Review / Merge",
            ],
        ),
        PaneView::Observer => (
            OBS_MAG,
            " ◈ OBSERVER",
            "Ask for critique, diagnosis, or the next step, then press Enter.",
            "Ctrl+O reviews the latest Coder output. /meta-diagnose inspects failures.",
            &app.observer_cfg,
            "Observer is best for critique, not execution.",
            [
                "1) Use Ctrl+O to review the latest Coder output",
                "2) Ask for critique, diagnosis, or next steps",
                "3) Use /meta-diagnose for a failed Coder message",
            ],
        ),
        PaneView::Chat => (
            ACCENT,
            " ◈ CHAT",
            "Use Chat for brainstorming or quick questions, then press Enter.",
            "Ctrl+R cycles Observer / Chat / Tasks / Review on the right pane.",
            &app.chat_cfg,
            "Chat does not execute tools.",
            [
                "1) Use Chat for brainstorming or clarification",
                "2) Move to Coder when you want execution",
                "3) Use /tab observer|tasks|promotions|merge to inspect the runtime",
            ],
        ),
    };
    let api_status = if provider_requires_key(&cfg.provider) {
        if cfg.api_key.is_some() {
            "API key: set"
        } else {
            "API key: missing — run /keys for env vars and CLI flags"
        }
    } else {
        "API key: not required for hf/local"
    };
    let provider_line = format!(
        "Provider: {} ({})  Model: {}  Mode: {}",
        provider_preset_for_run(cfg).label(),
        provider_abbrev(cfg),
        cfg.model,
        cfg.mode.label()
    );

    let mut lines = vec![
        Line::from(Span::styled(
            heading,
            Style::default().fg(brand).add_modifier(Modifier::BOLD),
        )),
        Line::default(),
        Line::from(Span::styled(hint1, Style::default().fg(TEXT_BODY))),
        Line::from(Span::styled(hint2, Style::default().fg(MUTED))),
        Line::from(Span::styled(provider_line, Style::default().fg(MUTED))),
        Line::from(Span::styled(api_status, Style::default().fg(WARN))),
        Line::from(Span::styled(extra, Style::default().fg(MUTED))),
        Line::default(),
        Line::from(Span::styled(
            "Quick start",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )),
        Line::default(),
    ];
    for step in quick_start {
        lines.push(Line::from(Span::styled(
            format!("  {step}"),
            Style::default().fg(TEXT_BODY),
        )));
    }
    lines.push(Line::default());

    for (key, desc) in [
        ("Tab", "switch focus"),
        ("Ctrl+R", "switch right pane tab"),
        ("/", "show slash command suggestions"),
        ("/keys", "show API key setup and pane status"),
        ("Enter", "send"),
        ("Shift+Enter", "newline"),
        ("Ctrl+K", "cancel streaming"),
        ("Ctrl+L", "clear current pane"),
        ("Ctrl+A", "toggle auto-observe"),
        ("Ctrl+O", "send latest Coder output to Observer"),
        ("PageUp/Down", "scroll"),
        ("End", "jump to bottom"),
        ("Ctrl+C / Esc", "quit"),
    ] {
        lines.push(key_row(key, desc));
    }

    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        Rect {
            x: area.x + 1,
            y: area.y + 1,
            width: area.width.saturating_sub(2),
            height: area.height.saturating_sub(2),
        },
    );
}

// ── Observer content renderer ─────────────────────────────────────────────────
//
// Sections:
//   --- phase ---         → ACCENT banner
//   --- proposals ---     → OBS_MAG banner + card-like rendering
//   --- coder_diagnostic --- → ACTION banner + JSON packet for Coder handoff
//   --- benchmark_plan --- → PROMO banner + regression design packet
//   --- critical_path --- → DANGER banner  (⚠ highlights)
//   --- health ---        → health score bar

#[derive(Clone, Copy, PartialEq)]
enum ObsSection {
    Body,
    Phase,
    Proposals,
    CoderDiagnostic,
    BenchmarkPlan,
    CriticalPath,
    Health,
}

fn obs_section_header(label: &'static str, color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("── {label} "),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled("─".repeat(18), Style::default().fg(UNFOCUSED)),
    ])
}

fn render_observer_content(content: &str) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut section = ObsSection::Body;

    for raw in content.lines() {
        let trimmed = raw.trim();

        // Section boundaries.
        match trimmed {
            "--- phase ---" => {
                section = ObsSection::Phase;
                lines.push(obs_section_header("PHASE", ACCENT));
                continue;
            }
            "--- proposals ---" => {
                section = ObsSection::Proposals;
                lines.push(obs_section_header("PROPOSALS", OBS_MAG));
                continue;
            }
            "--- coder_diagnostic ---" => {
                section = ObsSection::CoderDiagnostic;
                lines.push(obs_section_header("CODER DIAGNOSTIC", ACTION));
                continue;
            }
            "--- benchmark_plan ---" => {
                section = ObsSection::BenchmarkPlan;
                lines.push(obs_section_header("BENCHMARK PLAN", PROMO));
                continue;
            }
            "--- critical_path ---" => {
                section = ObsSection::CriticalPath;
                lines.push(obs_section_header("CRITICAL PATH", DANGER));
                continue;
            }
            "--- health ---" => {
                section = ObsSection::Health;
                lines.push(obs_section_header("HEALTH", SUCCESS));
                continue;
            }
            _ => {}
        }
        if trimmed.starts_with("--- ") && trimmed.ends_with(" ---") {
            section = ObsSection::Body;
        }

        let line = match section {
            ObsSection::Body => Line::from(Span::styled(
                raw.to_string(),
                Style::default().fg(TEXT_BODY),
            )),
            ObsSection::Phase => Line::from(vec![
                Span::styled("  ◈ ", Style::default().fg(ACCENT)),
                Span::styled(
                    trimmed.to_string(),
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
            ]),
            ObsSection::Proposals => proposal_line(trimmed),
            ObsSection::CoderDiagnostic => coder_diagnostic_line(trimmed),
            ObsSection::BenchmarkPlan => benchmark_plan_line(trimmed),
            ObsSection::CriticalPath => {
                if trimmed.is_empty() {
                    Line::default()
                } else {
                    Line::from(vec![
                        Span::styled(
                            "  ⚠ ",
                            Style::default().fg(DANGER).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(trimmed.to_string(), Style::default().fg(DANGER)),
                    ])
                }
            }
            ObsSection::Health => health_line(trimmed),
        };
        lines.push(line);
    }
    lines
}

fn parse_score_from(s: &str) -> Option<u32> {
    let low = s.to_ascii_lowercase();
    let idx = low.find("score:")?;
    low[idx + 6..]
        .trim()
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

fn score_color(score: u32) -> Color {
    if score >= 70 {
        DANGER
    } else if score >= 40 {
        WARN
    } else {
        SUCCESS
    }
}

fn proposal_line(trimmed: &str) -> Line<'static> {
    let lower = trimmed.to_ascii_lowercase();

    // Numbered header: "1) title: ..."
    let first_char_digit = trimmed.chars().next().map_or(false, |c| c.is_ascii_digit());
    if first_char_digit && trimmed.contains(") ") {
        return Line::from(vec![
            Span::raw("  "),
            Span::styled(
                trimmed.to_string(),
                Style::default().fg(TEXT_BODY).add_modifier(Modifier::BOLD),
            ),
        ]);
    }

    // score: N  →  score bar
    if lower.starts_with("score:") {
        if let Some(s) = parse_score_from(&lower) {
            let c = score_color(s);
            let filled = (s / 10) as usize;
            let bar: String = (0..10)
                .map(|i| if i < filled { '█' } else { '░' })
                .collect();
            return Line::from(vec![
                Span::styled(
                    format!("    score: {s:>3} "),
                    Style::default().fg(c).add_modifier(Modifier::BOLD),
                ),
                Span::styled(bar, Style::default().fg(c)),
            ]);
        }
    }

    // severity: crit|warn|info
    if lower.starts_with("severity:") {
        let style = if lower.contains("crit") {
            Style::default().fg(DANGER).add_modifier(Modifier::BOLD)
        } else if lower.contains("warn") {
            Style::default().fg(WARN)
        } else {
            Style::default().fg(MUTED)
        };
        return Line::from(Span::styled(format!("    {trimmed}"), style));
    }

    // to_coder: msg
    if lower.starts_with("to_coder:") {
        let val = trimmed["to_coder:".len()..].trim().to_string();
        return Line::from(vec![
            Span::styled(
                "    ↳ ",
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(val, Style::default().fg(ACCENT)),
        ]);
    }

    // quote: snippet
    if lower.starts_with("quote:") {
        let q = trimmed["quote:".len()..].trim().to_string();
        return Line::from(vec![
            Span::styled("    ❝ ", Style::default().fg(MUTED)),
            Span::styled(q, Style::default().fg(WARN).add_modifier(Modifier::ITALIC)),
        ]);
    }

    // status: addressed|[UNRESOLVED]|[ESCALATED]
    if lower.starts_with("status:") {
        let val = lower["status:".len()..].trim().to_string();
        let style = if val.contains("unresolved") || val.contains("escalated") {
            Style::default().fg(DANGER)
        } else if val.contains("addressed") {
            Style::default().fg(SUCCESS)
        } else {
            Style::default().fg(MUTED)
        };
        return Line::from(Span::styled(format!("    {trimmed}"), style));
    }

    // impact / cost / phase labels (secondary info)
    if lower.starts_with("impact:") || lower.starts_with("cost:") || lower.starts_with("phase:") {
        return Line::from(Span::styled(
            format!("    {trimmed}"),
            Style::default().fg(MUTED),
        ));
    }

    // Default proposal line
    Line::from(Span::styled(
        format!("  {trimmed}"),
        Style::default().fg(TEXT_BODY),
    ))
}

fn coder_diagnostic_line(trimmed: &str) -> Line<'static> {
    if trimmed.is_empty() {
        return Line::default();
    }
    let lower = trimmed.to_ascii_lowercase();
    let style = if lower.contains("\"failure_mode\"")
        || lower.contains("\"mutation_anchor\"")
        || lower.contains("\"next_coder_action\"")
    {
        Style::default().fg(ACTION).add_modifier(Modifier::BOLD)
    } else if lower.contains("\"required_followups\"")
        || lower.contains("\"verification_cmd\"")
        || lower.contains("\"final_handoff_literals\"")
    {
        Style::default().fg(ACCENT)
    } else {
        Style::default().fg(TEXT_BODY)
    };
    Line::from(Span::styled(format!("  {trimmed}"), style))
}

fn benchmark_plan_line(trimmed: &str) -> Line<'static> {
    if trimmed.is_empty() {
        return Line::default();
    }
    let lower = trimmed.to_ascii_lowercase();
    let style = if lower.contains("\"lane\"")
        || lower.contains("\"objective\"")
        || lower.contains("\"case_id_hint\"")
    {
        Style::default().fg(PROMO).add_modifier(Modifier::BOLD)
    } else if lower.contains("\"required_checks\"")
        || lower.contains("\"success_criteria\"")
        || lower.contains("\"target_files\"")
    {
        Style::default().fg(ACCENT)
    } else {
        Style::default().fg(TEXT_BODY)
    };
    Line::from(Span::styled(format!("  {trimmed}"), style))
}

fn health_line(trimmed: &str) -> Line<'static> {
    if trimmed.is_empty() {
        return Line::default();
    }
    if let Some(score) = parse_score_from(&trimmed.to_ascii_lowercase()) {
        let c = if score >= 70 {
            SUCCESS
        } else if score >= 40 {
            WARN
        } else {
            DANGER
        };
        let filled = (score / 5) as usize; // 20-block bar
        let bar: String = (0..20)
            .map(|i| if i < filled { '█' } else { '░' })
            .collect();
        return Line::from(vec![
            Span::styled("  ❤ ", Style::default().fg(c).add_modifier(Modifier::BOLD)),
            Span::styled(
                format!("{score:>3}/100 "),
                Style::default().fg(c).add_modifier(Modifier::BOLD),
            ),
            Span::styled(bar, Style::default().fg(c)),
        ]);
    }
    Line::from(Span::styled(
        format!("    {trimmed}"),
        Style::default().fg(MUTED),
    ))
}

// ── Coder content renderer ────────────────────────────────────────────────────
//
// Visual zones:
//   <plan>…</plan>      Blue box-drawing frame (CODER_BLUE/UNFOCUSED)
//   <think>…</think>    Gray box-drawing frame; per-field colors
//   <reflect>…</reflect> Gray box-drawing frame; dim italic
//   ```lang…```         Bordered code block
//   [TOOL] cmd          ▶ EXEC label (WARN)
//   [RESULT] exit=0     ✓ OK (SUCCESS)
//   [RESULT] exit=N ⚠   ✗ exit=N (DANGER)
//   [agent] …           dim annotation (UNFOCUSED)
//   diff …              unified-diff coloring
//   else                TEXT_BODY

fn render_coder_content(content: &str) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut in_think = false;
    let mut in_plan = false;
    let mut in_reflect = false;
    let mut in_diff = false;
    let mut in_code = false;
    let mut code_lang = String::new();

    for raw in content.lines() {
        let trimmed = raw.trim();

        // ── <plan> block ──────────────────────────────────────────────────────
        if trimmed.starts_with("<plan>") {
            in_plan = true;
            in_think = false;
            in_reflect = false;
            lines.push(Line::from(vec![
                Span::styled("  ╭── ", Style::default().fg(CODER_BLUE)),
                Span::styled(
                    "PLAN",
                    Style::default()
                        .fg(CODER_BLUE)
                        .add_modifier(Modifier::BOLD | Modifier::ITALIC),
                ),
                Span::styled(" ─────────────────", Style::default().fg(UNFOCUSED)),
            ]));
            if trimmed.contains("</plan>") {
                in_plan = false;
                lines.push(Line::from(Span::styled(
                    "  ╰─────────────────────────",
                    Style::default().fg(UNFOCUSED),
                )));
            }
            continue;
        }
        if in_plan {
            if trimmed.contains("</plan>") {
                in_plan = false;
                lines.push(Line::from(Span::styled(
                    "  ╰─────────────────────────",
                    Style::default().fg(UNFOCUSED),
                )));
            } else {
                lines.push(Line::from(vec![
                    Span::styled("  │ ", Style::default().fg(UNFOCUSED)),
                    Span::styled(
                        trimmed.to_string(),
                        Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
                    ),
                ]));
            }
            continue;
        }

        // ── <reflect> block ───────────────────────────────────────────────────
        if trimmed.starts_with("<reflect>") {
            in_reflect = true;
            in_think = false;
            in_diff = false;
            lines.push(Line::from(vec![
                Span::styled("  ╭── ", Style::default().fg(UNFOCUSED)),
                Span::styled(
                    "reflect",
                    Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
                ),
                Span::styled(" ────────────────", Style::default().fg(UNFOCUSED)),
            ]));
            if trimmed.contains("</reflect>") {
                in_reflect = false;
                lines.push(Line::from(Span::styled(
                    "  ╰─────────────────────────",
                    Style::default().fg(UNFOCUSED),
                )));
            }
            continue;
        }
        if in_reflect {
            if trimmed.contains("</reflect>") {
                in_reflect = false;
                lines.push(Line::from(Span::styled(
                    "  ╰─────────────────────────",
                    Style::default().fg(UNFOCUSED),
                )));
            } else {
                let (label, rest) = split_field(trimmed);
                let label_style = Style::default().fg(MUTED).add_modifier(Modifier::ITALIC);
                let rest_style = Style::default().fg(MUTED).add_modifier(Modifier::ITALIC);
                if label.is_empty() {
                    lines.push(Line::from(vec![
                        Span::styled("  │ ", Style::default().fg(UNFOCUSED)),
                        Span::styled(trimmed.to_string(), rest_style),
                    ]));
                } else {
                    lines.push(Line::from(vec![
                        Span::styled("  │ ", Style::default().fg(UNFOCUSED)),
                        Span::styled(format!("{label}: "), label_style),
                        Span::styled(rest.to_string(), rest_style),
                    ]));
                }
            }
            continue;
        }

        // ── <think> block ─────────────────────────────────────────────────────
        if trimmed.starts_with("<think>") {
            in_think = true;
            in_diff = false;
            lines.push(Line::from(vec![
                Span::styled("  ╭── ", Style::default().fg(UNFOCUSED)),
                Span::styled(
                    "think",
                    Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
                ),
                Span::styled(" ──────────────────", Style::default().fg(UNFOCUSED)),
            ]));
            if trimmed.contains("</think>") {
                in_think = false;
                lines.push(Line::from(Span::styled(
                    "  ╰─────────────────────────",
                    Style::default().fg(UNFOCUSED),
                )));
            }
            continue;
        }
        if in_think {
            if trimmed.contains("</think>") {
                in_think = false;
                lines.push(Line::from(Span::styled(
                    "  ╰─────────────────────────",
                    Style::default().fg(UNFOCUSED),
                )));
            } else {
                let (label, rest) = split_field(trimmed);
                let (label_style, rest_style) = match label {
                    "goal" => (
                        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                        Style::default().fg(ACCENT).add_modifier(Modifier::ITALIC),
                    ),
                    "risk" => (
                        Style::default().fg(WARN).add_modifier(Modifier::BOLD),
                        Style::default().fg(WARN).add_modifier(Modifier::ITALIC),
                    ),
                    "next" => (
                        Style::default().fg(CODER_BLUE).add_modifier(Modifier::BOLD),
                        Style::default()
                            .fg(CODER_BLUE)
                            .add_modifier(Modifier::ITALIC),
                    ),
                    "verify" => (
                        Style::default().fg(SUCCESS).add_modifier(Modifier::BOLD),
                        Style::default().fg(SUCCESS).add_modifier(Modifier::ITALIC),
                    ),
                    _ => (
                        Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
                        Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
                    ),
                };
                if label.is_empty() {
                    lines.push(Line::from(vec![
                        Span::styled("  │ ", Style::default().fg(UNFOCUSED)),
                        Span::styled(trimmed.to_string(), rest_style),
                    ]));
                } else {
                    lines.push(Line::from(vec![
                        Span::styled("  │ ", Style::default().fg(UNFOCUSED)),
                        Span::styled(format!("{label}: "), label_style),
                        Span::styled(rest.to_string(), rest_style),
                    ]));
                }
            }
            continue;
        }

        // ── Code fences ───────────────────────────────────────────────────────
        if trimmed.starts_with("```") {
            if in_code {
                in_code = false;
                lines.push(Line::from(Span::styled(
                    "  ╰──────────────────────────",
                    Style::default().fg(UNFOCUSED),
                )));
                code_lang.clear();
            } else {
                in_code = true;
                code_lang = trimmed.trim_start_matches('`').to_string();
                let lang_label = if code_lang.is_empty() {
                    String::new()
                } else {
                    format!(" {}", code_lang)
                };
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("  ╭──{lang_label} "),
                        Style::default().fg(UNFOCUSED),
                    ),
                    Span::styled("──────────────────", Style::default().fg(UNFOCUSED)),
                ]));
            }
            continue;
        }
        if in_code {
            let style = code_line_style(&code_lang, trimmed);
            lines.push(Line::from(vec![
                Span::styled("  │ ", Style::default().fg(UNFOCUSED)),
                Span::styled(raw.to_string(), style),
            ]));
            continue;
        }

        // ── Special markers ───────────────────────────────────────────────────
        if trimmed.starts_with("[TOOL]") {
            in_diff = false;
            let cmd = trimmed.trim_start_matches("[TOOL]").trim();
            lines.push(Line::from(vec![
                Span::styled(
                    "  ▶ EXEC ",
                    Style::default().fg(WARN).add_modifier(Modifier::BOLD),
                ),
                Span::styled(cmd.to_string(), Style::default().fg(TEXT_BODY)),
            ]));
            continue;
        }
        if trimmed.starts_with("[READ_FILE]") {
            in_diff = false;
            let path = trimmed.trim_start_matches("[READ_FILE]").trim();
            lines.push(Line::from(vec![
                Span::styled(
                    "  📄 READ  ",
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Span::styled(path.to_string(), Style::default().fg(TEXT_BODY)),
            ]));
            continue;
        }
        if trimmed.starts_with("[WRITE_FILE]") {
            in_diff = false;
            let path = trimmed.trim_start_matches("[WRITE_FILE]").trim();
            lines.push(Line::from(vec![
                Span::styled(
                    "  ✎ WRITE ",
                    Style::default().fg(CODER_BLUE).add_modifier(Modifier::BOLD),
                ),
                Span::styled(path.to_string(), Style::default().fg(TEXT_BODY)),
            ]));
            continue;
        }
        if trimmed.starts_with("[PATCH_FILE]") {
            in_diff = false;
            let path = trimmed.trim_start_matches("[PATCH_FILE]").trim();
            lines.push(Line::from(vec![
                Span::styled(
                    "  ⟳ PATCH ",
                    Style::default().fg(OBS_MAG).add_modifier(Modifier::BOLD),
                ),
                Span::styled(path.to_string(), Style::default().fg(TEXT_BODY)),
            ]));
            continue;
        }
        if trimmed.starts_with("[SEARCH_FILES]") {
            in_diff = false;
            let pat = trimmed.trim_start_matches("[SEARCH_FILES]").trim();
            lines.push(Line::from(vec![
                Span::styled(
                    "  🔍 SEARCH ",
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Span::styled(pat.to_string(), Style::default().fg(TEXT_BODY)),
            ]));
            continue;
        }
        if trimmed.starts_with("[RESULT_SEARCH]") {
            in_diff = false;
            let rest = trimmed.trim_start_matches("[RESULT_SEARCH]").trim();
            lines.push(Line::from(vec![
                Span::styled(
                    "  ✓ ",
                    Style::default().fg(SUCCESS).add_modifier(Modifier::BOLD),
                ),
                Span::styled(rest.to_string(), Style::default().fg(SUCCESS)),
            ]));
            continue;
        }
        if trimmed.starts_with("[APPLY_DIFF]") {
            in_diff = false;
            let path = trimmed.trim_start_matches("[APPLY_DIFF]").trim();
            lines.push(Line::from(vec![
                Span::styled(
                    "  ⟁ DIFF ",
                    Style::default().fg(OBS_MAG).add_modifier(Modifier::BOLD),
                ),
                Span::styled(path.to_string(), Style::default().fg(TEXT_BODY)),
            ]));
            continue;
        }
        if trimmed.starts_with("[auto-test]") {
            in_diff = false;
            let rest = trimmed.trim_start_matches("[auto-test]").trim();
            let (icon, col) = if rest.starts_with('✓') {
                ("  ✓ TEST ", SUCCESS)
            } else {
                ("  ✗ TEST ", DANGER)
            };
            lines.push(Line::from(vec![
                Span::styled(icon, Style::default().fg(col).add_modifier(Modifier::BOLD)),
                Span::styled(rest.to_string(), Style::default().fg(col)),
            ]));
            continue;
        }
        if trimmed.starts_with("[git checkpoint]") {
            in_diff = false;
            let rest = trimmed.trim_start_matches("[git checkpoint]").trim();
            lines.push(Line::from(vec![
                Span::styled(
                    "  ◎ CHKPT ",
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Span::styled(rest.to_string(), Style::default().fg(ACCENT)),
            ]));
            continue;
        }
        if trimmed.starts_with("[GLOB]") {
            in_diff = false;
            let pat = trimmed.trim_start_matches("[GLOB]").trim();
            lines.push(Line::from(vec![
                Span::styled(
                    "  ❖ GLOB ",
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Span::styled(pat.to_string(), Style::default().fg(TEXT_BODY)),
            ]));
            continue;
        }
        if trimmed.starts_with("[RESULT_GLOB]") {
            in_diff = false;
            let rest = trimmed.trim_start_matches("[RESULT_GLOB]").trim();
            lines.push(Line::from(vec![
                Span::styled(
                    "  ✓ ",
                    Style::default().fg(SUCCESS).add_modifier(Modifier::BOLD),
                ),
                Span::styled(rest.to_string(), Style::default().fg(SUCCESS)),
            ]));
            continue;
        }
        if trimmed.starts_with("[CACHE_HIT]") {
            in_diff = false;
            let rest = trimmed.trim_start_matches("[CACHE_HIT]").trim();
            lines.push(Line::from(vec![
                Span::styled(
                    "  ⚡ ",
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Span::styled(rest.to_string(), Style::default().fg(ACCENT)),
            ]));
            continue;
        }
        if trimmed.starts_with("[RESULT_FILE]") {
            in_diff = false;
            let rest = trimmed.trim_start_matches("[RESULT_FILE]").trim();
            lines.push(Line::from(vec![
                Span::styled(
                    "  ✓ ",
                    Style::default().fg(SUCCESS).add_modifier(Modifier::BOLD),
                ),
                Span::styled(rest.to_string(), Style::default().fg(SUCCESS)),
            ]));
            continue;
        }
        if trimmed.starts_with("[RESULT_FILE_ERR]") {
            in_diff = false;
            let rest = trimmed.trim_start_matches("[RESULT_FILE_ERR]").trim();
            lines.push(Line::from(vec![
                Span::styled(
                    "  ✗ ",
                    Style::default().fg(DANGER).add_modifier(Modifier::BOLD),
                ),
                Span::styled(rest.to_string(), Style::default().fg(DANGER)),
            ]));
            continue;
        }
        if trimmed.starts_with("[RESULT] exit=0") {
            in_diff = false;
            lines.push(Line::from(Span::styled(
                "  ✓ OK",
                Style::default().fg(SUCCESS).add_modifier(Modifier::BOLD),
            )));
            continue;
        }
        if trimmed.starts_with("[RESULT]") {
            in_diff = false;
            let rest = trimmed.trim_start_matches("[RESULT]").trim();
            lines.push(Line::from(vec![
                Span::styled(
                    "  ✗ ",
                    Style::default().fg(DANGER).add_modifier(Modifier::BOLD),
                ),
                Span::styled(rest.to_string(), Style::default().fg(DANGER)),
            ]));
            continue;
        }
        if trimmed.starts_with("[agent]") {
            in_diff = false;
            lines.push(Line::from(Span::styled(
                format!("  {trimmed}"),
                Style::default().fg(UNFOCUSED),
            )));
            continue;
        }

        // ── Diff ──────────────────────────────────────────────────────────────
        if trimmed.starts_with("diff --git ")
            || trimmed.starts_with("diff -u ")
            || trimmed.starts_with("diff -r ")
        {
            in_diff = true;
        }
        if in_diff {
            let style = if trimmed.starts_with("diff ") {
                Style::default().fg(CODER_BLUE).add_modifier(Modifier::BOLD)
            } else if trimmed.starts_with("+++ ")
                || trimmed.starts_with("--- ")
                || trimmed.starts_with("index ")
                || trimmed.starts_with("new file")
                || trimmed.starts_with("deleted file")
            {
                Style::default().fg(TEXT_BODY).add_modifier(Modifier::BOLD)
            } else if trimmed.starts_with("@@ ") {
                Style::default().fg(ACCENT)
            } else if trimmed.starts_with('+') && !trimmed.starts_with("+++") {
                Style::default().fg(SUCCESS)
            } else if trimmed.starts_with('-') && !trimmed.starts_with("---") {
                Style::default().fg(DANGER)
            } else {
                Style::default().fg(TEXT_BODY)
            };
            lines.push(Line::from(Span::styled(raw.to_string(), style)));
            continue;
        }

        // Default
        lines.push(Line::from(Span::styled(
            raw.to_string(),
            Style::default().fg(TEXT_BODY),
        )));
    }
    lines
}

/// Split "label: rest" into ("label", "rest").  Returns ("", line) if no colon found.
fn split_field(line: &str) -> (&str, &str) {
    if let Some(idx) = line.find(':') {
        (line[..idx].trim(), line[idx + 1..].trim())
    } else {
        ("", line)
    }
}

/// Minimal per-language code line styling.
fn code_line_style(lang: &str, line: &str) -> Style {
    let l = lang.to_ascii_lowercase();
    if l == "diff" {
        if line.starts_with('+') {
            return Style::default().fg(SUCCESS);
        }
        if line.starts_with('-') {
            return Style::default().fg(DANGER);
        }
        if line.starts_with('@') {
            return Style::default().fg(ACCENT);
        }
    }
    if l == "powershell" || l == "ps1" || l == "ps" || l == "pwsh" {
        if line.trim_start().starts_with('#') {
            return Style::default().fg(MUTED).add_modifier(Modifier::ITALIC);
        }
        if line.trim_start().starts_with("$") || line.contains(" $") {
            return Style::default().fg(CODER_BLUE);
        }
    }
    if l == "bash" || l == "sh" || l == "shell" || l == "console" {
        if line.trim_start().starts_with('#') {
            return Style::default().fg(MUTED).add_modifier(Modifier::ITALIC);
        }
        if line.starts_with("$ ") || line.starts_with("PS> ") {
            return Style::default().fg(CODER_BLUE).add_modifier(Modifier::BOLD);
        }
    }
    Style::default().fg(TEXT_BODY)
}

// ── Input bar ─────────────────────────────────────────────────────────────────

fn render_input(frame: &mut Frame, area: Rect, app: &App) {
    let (label, brand, is_streaming, read_only) = match app.focus {
        Focus::Coder => ("CODER", CODER_BLUE, app.coder.streaming, false),
        Focus::Right => match app.right_tab {
            RightTab::Observer => ("OBSERVER", OBS_MAG, app.observer.streaming, false),
            RightTab::Chat => ("CHAT", ACCENT, app.chat.streaming, false),
            RightTab::Tasks => ("TASKS", WARN, false, true),
            RightTab::Promotions => ("REVIEW", PROMO, false, true),
            RightTab::MergeGate => ("MERGE", MERGE, false, true),
        },
    };

    let input_text = current_input_text(app);
    let active_picker = active_picker(&input_text);
    let slash_items = if active_picker.is_none() {
        slash_suggestions(&input_text)
    } else {
        None
    };
    let hint = if is_streaming {
        "Ctrl+K=cancel"
    } else if read_only {
        match app.right_tab {
            RightTab::Tasks => "Enter=dispatch  Space=done  Ctrl+R=tab",
            RightTab::Promotions => "Enter=primary  A=approve  H=hold  P=apply  R=refresh",
            RightTab::MergeGate => "Enter=approve  A=approve  H=hold  R=refresh  Ctrl+Y=copy",
            RightTab::Observer | RightTab::Chat => "Ctrl+R=tab",
        }
    } else if active_picker.is_some() {
        "Up/Down=select  Enter=apply"
    } else if slash_items.is_some() {
        "Enter=run command"
    } else {
        "Enter=send  Shift+Enter=newline  /=commands"
    };

    let title = Line::from(vec![
        Span::raw(" "),
        Span::styled(">", Style::default().fg(brand).add_modifier(Modifier::BOLD)),
        Span::raw(" "),
        Span::styled(
            label,
            Style::default().fg(brand).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(hint, Style::default().fg(MUTED)),
        Span::raw(" "),
    ]);

    let border_color = border_color(is_streaming, brand);

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if read_only {
        let mut lines: Vec<Line> = Vec::new();
        match app.right_tab {
            RightTab::Tasks => {
                if app.tasks.is_empty() {
                    lines.push(Line::from(Span::styled(
                        "  (no tasks)",
                        Style::default().fg(MUTED),
                    )));
                } else {
                    let idx = app.tasks_cursor.min(app.tasks.len().saturating_sub(1));
                    let t = &app.tasks[idx];
                    lines.push(Line::from(Span::styled(
                        format!("  {}", t.title),
                        Style::default().fg(TEXT_BODY).add_modifier(Modifier::BOLD),
                    )));
                    lines.push(Line::from(Span::styled(
                        format!("  priority: {}", t.priority),
                        Style::default().fg(MUTED),
                    )));
                    lines.push(Line::default());
                    for l in t.body.lines() {
                        lines.push(Line::from(Span::styled(
                            format!("  {l}"),
                            Style::default().fg(TEXT_BODY),
                        )));
                    }
                }
            }
            RightTab::Promotions => {
                if app.harness_promotions.entries.is_empty() {
                    lines.push(Line::from(Span::styled(
                        format!(
                            "  {}",
                            app.harness_promotions
                                .status_message
                                .clone()
                                .unwrap_or_else(|| "no promotion candidates".to_string())
                        ),
                        Style::default().fg(MUTED),
                    )));
                } else {
                    let idx = app
                        .harness_promotions_cursor
                        .min(app.harness_promotions.entries.len().saturating_sub(1));
                    let entry = &app.harness_promotions.entries[idx];
                    lines.push(Line::from(Span::styled(
                        format!("  {}", entry.title),
                        Style::default().fg(TEXT_BODY).add_modifier(Modifier::BOLD),
                    )));
                    lines.push(Line::from(Span::styled(
                        format!(
                            "  status:{}  decision:{}  green_cases:{}",
                            entry.review_badge,
                            entry.badge,
                            entry.green_case_ids.len()
                        ),
                        Style::default().fg(MUTED),
                    )));
                    lines.push(Line::from(Span::styled(
                        format!(
                            "  primary:{}  contract:{}",
                            promotion_gate::primary_action_hint(entry),
                            entry.contract_path
                        ),
                        Style::default().fg(MUTED),
                    )));
                    if let Some(status) = app.harness_promotions_status.as_deref() {
                        lines.push(Line::from(Span::styled(
                            format!("  note: {status}"),
                            Style::default().fg(ACCENT),
                        )));
                    }
                    lines.push(Line::default());
                    for reason in &entry.reasons {
                        lines.push(Line::from(Span::styled(
                            format!("  - {reason}"),
                            Style::default().fg(TEXT_BODY),
                        )));
                    }
                    if let Some(path) = entry.patch_path.as_deref() {
                        lines.push(Line::default());
                        lines.push(Line::from(Span::styled(
                            format!("  patch: {path}"),
                            Style::default().fg(MUTED),
                        )));
                    }
                    lines.push(Line::default());
                    lines.push(Line::from(Span::styled(
                        format!(
                            "  keys: enter=primary  a=approve ({})  h=hold ({})  p=apply ({})  r=refresh",
                            if entry.can_approve { "ready" } else { "locked" },
                            if entry.can_hold { "ready" } else { "locked" },
                            if entry.can_apply { "ready" } else { "locked" }
                        ),
                        Style::default().fg(MUTED),
                    )));
                }
            }
            RightTab::MergeGate => {
                if app.merge_gate.entries.is_empty() {
                    lines.push(Line::from(Span::styled(
                        format!(
                            "  {}",
                            app.merge_gate
                                .status_message
                                .clone()
                                .unwrap_or_else(|| "no merge gate artifact yet".to_string())
                        ),
                        Style::default().fg(MUTED),
                    )));
                } else {
                    let idx = app
                        .merge_gate_cursor
                        .min(app.merge_gate.entries.len().saturating_sub(1));
                    let entry = &app.merge_gate.entries[idx];
                    lines.push(Line::from(Span::styled(
                        format!("  {}", entry.id),
                        Style::default().fg(TEXT_BODY).add_modifier(Modifier::BOLD),
                    )));
                    lines.push(Line::from(Span::styled(
                        format!(
                            "  case:{}  review:{}  root:{}",
                            entry.case_status, entry.review_badge, entry.root
                        ),
                        Style::default().fg(MUTED),
                    )));
                    lines.push(Line::from(Span::styled(
                        format!("  primary:{}", merge_gate::primary_action_hint(entry)),
                        Style::default().fg(MUTED),
                    )));
                    if let Some(status) = app.merge_gate_status.as_deref() {
                        lines.push(Line::from(Span::styled(
                            format!("  note: {status}"),
                            Style::default().fg(ACCENT),
                        )));
                    }
                    if let Some(path) = entry.promoted_overlay_path.as_deref() {
                        lines.push(Line::default());
                        lines.push(Line::from(Span::styled(
                            format!("  overlay: {path}"),
                            Style::default().fg(MUTED),
                        )));
                    }
                    if let Some(command) = entry.rollback_command.as_deref() {
                        lines.push(Line::default());
                        lines.push(Line::from(Span::styled(
                            "  rollback preview (not executed):",
                            Style::default().fg(WARN),
                        )));
                        lines.push(Line::from(Span::styled(
                            format!("  {command}"),
                            Style::default().fg(TEXT_BODY),
                        )));
                    }
                    lines.push(Line::default());
                    lines.push(Line::from(Span::styled(
                        format!(
                            "  keys: enter=approve  a=approve ({})  h=hold ({})  r=refresh  ctrl+y=copy preview",
                            if entry.can_approve { "ready" } else { "locked" },
                            if entry.can_hold { "ready" } else { "locked" }
                        ),
                        Style::default().fg(MUTED),
                    )));
                }
            }
            RightTab::Observer | RightTab::Chat => {}
        }
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
        return;
    }
    if let Some(kind) = active_picker {
        render_picker_input(frame, inner, app, kind);
        return;
    }
    if let Some(items) = slash_items {
        let inner_split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(inner);
        let line = Line::from(vec![
            Span::styled("  Commands ", Style::default().fg(MUTED)),
            Span::styled(items.join("  "), Style::default().fg(ACCENT)),
        ]);
        frame.render_widget(Paragraph::new(line), inner_split[0]);
        render_active_textarea(frame, inner_split[1], app);
        return;
    }
    if input_text.trim().is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("  {}", input_placeholder(app)),
                Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
            ))),
            inner,
        );
        return;
    }
    render_active_textarea(frame, inner, app);
}

fn border_color(is_streaming: bool, brand: Color) -> Color {
    if is_streaming {
        WARN
    } else {
        brand
    }
}

fn current_input_text(app: &App) -> String {
    match app.focus {
        Focus::Coder => app.coder.textarea.lines().join("\n"),
        Focus::Right => match app.right_tab {
            RightTab::Observer => app.observer.textarea.lines().join("\n"),
            RightTab::Chat => app.chat.textarea.lines().join("\n"),
            RightTab::Tasks | RightTab::Promotions | RightTab::MergeGate => String::new(),
        },
    }
}

fn active_picker(input: &str) -> Option<ActivePicker> {
    match input.trim() {
        "/provider" => Some(ActivePicker::Provider),
        "/model" => Some(ActivePicker::Model),
        _ => None,
    }
}

fn picker_items(app: &App, kind: ActivePicker) -> Vec<String> {
    match kind {
        ActivePicker::Provider => {
            let allow_all = !matches!(app.focus, Focus::Coder);
            provider_preset_keys(!allow_all)
                .into_iter()
                .map(str::to_string)
                .collect()
        }
        ActivePicker::Model => {
            let cfg = active_run_config(app);
            representative_models_for_run(cfg)
                .iter()
                .map(|s| s.to_string())
                .collect()
        }
    }
}

fn render_picker_input(frame: &mut Frame, area: Rect, app: &App, kind: ActivePicker) {
    let items = picker_items(app, kind);
    let pane = active_pane(app);
    let selected = pane.picker_index.min(items.len().saturating_sub(1));
    let mut lines = Vec::new();
    let header = match kind {
        ActivePicker::Provider => "  Select provider".to_string(),
        ActivePicker::Model => format!(
            "  Select model for {}",
            provider_preset_for_run(active_run_config(app)).label()
        ),
    };
    lines.push(Line::from(Span::styled(
        header,
        Style::default().fg(MUTED).add_modifier(Modifier::BOLD),
    )));
    for (idx, item) in items
        .iter()
        .take(area.height.saturating_sub(1) as usize)
        .enumerate()
    {
        let prefix = if idx == selected { "›" } else { " " };
        let style = if idx == selected {
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT_BODY)
        };
        lines.push(Line::from(Span::styled(
            format!("  {prefix} {item}"),
            style,
        )));
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn slash_suggestions(input: &str) -> Option<Vec<&'static str>> {
    let trimmed = input.trim_start();
    if !trimmed.starts_with('/') {
        return None;
    }
    let cmd = trimmed
        .split_whitespace()
        .next()
        .unwrap_or("/")
        .to_ascii_lowercase();
    let commands = [
        "/help",
        "/keys",
        "/tab",
        "/provider",
        "/base_url",
        "/model",
        "/mode",
        "/persona",
        "/temp",
        "/lang",
        "/root",
        "/find",
        "/meta-diagnose",
        "/autofix",
        "/diff",
        "/init",
        "/rollback",
    ];
    let mut matches: Vec<&str> = commands
        .iter()
        .copied()
        .filter(|candidate| candidate.starts_with(&cmd))
        .collect();
    if matches.is_empty() {
        matches = commands.into_iter().take(6).collect();
    }
    if matches.len() > 6 {
        matches.truncate(6);
    }
    Some(matches)
}

fn render_active_textarea(frame: &mut Frame, area: Rect, app: &App) {
    match app.focus {
        Focus::Coder => frame.render_widget(&app.coder.textarea, area),
        Focus::Right => match app.right_tab {
            RightTab::Observer => frame.render_widget(&app.observer.textarea, area),
            RightTab::Chat => frame.render_widget(&app.chat.textarea, area),
            RightTab::Tasks | RightTab::Promotions | RightTab::MergeGate => {}
        },
    }
}

fn active_pane(app: &App) -> &super::app::Pane {
    match app.focus {
        Focus::Coder => &app.coder,
        Focus::Right => match app.right_tab {
            RightTab::Observer => &app.observer,
            RightTab::Chat => &app.chat,
            RightTab::Tasks | RightTab::Promotions | RightTab::MergeGate => &app.observer,
        },
    }
}

fn active_run_config(app: &App) -> &crate::config::RunConfig {
    match app.focus {
        Focus::Coder => &app.coder_cfg,
        Focus::Right => match app.right_tab {
            RightTab::Observer => &app.observer_cfg,
            RightTab::Chat => &app.chat_cfg,
            RightTab::Tasks | RightTab::Promotions | RightTab::MergeGate => &app.observer_cfg,
        },
    }
}

fn input_placeholder(app: &App) -> &'static str {
    match app.focus {
        Focus::Coder => "Describe the coding task…",
        Focus::Right => match app.right_tab {
            RightTab::Observer => "Ask for critique, diagnosis, or /meta-diagnose…",
            RightTab::Chat => "Ask a question or brainstorm…",
            RightTab::Tasks | RightTab::Promotions | RightTab::MergeGate => "",
        },
    }
}

fn render_footer(frame: &mut Frame, area: Rect, app: &App) {
    let current = match app.focus {
        Focus::Coder => "Coder",
        Focus::Right => match app.right_tab {
            RightTab::Observer => "Observer",
            RightTab::Chat => "Chat",
            RightTab::Tasks => "Tasks",
            RightTab::Promotions => "Promotions",
            RightTab::MergeGate => "Merge Gate",
        },
    };
    let line = Line::from(vec![
        Span::styled("  Focus:", Style::default().fg(MUTED)),
        Span::styled(
            format!(" {current}"),
            Style::default().fg(TEXT_BODY).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ·  Tab focus", Style::default().fg(MUTED)),
        Span::styled("  ·  Ctrl+R tabs", Style::default().fg(MUTED)),
        Span::styled("  ·  / help", Style::default().fg(MUTED)),
        Span::styled("  ·  /keys", Style::default().fg(MUTED)),
        Span::styled("  ·  /tab", Style::default().fg(MUTED)),
    ]);
    frame.render_widget(
        Paragraph::new(line).style(Style::default().bg(BG_DARK)),
        area,
    );
}
