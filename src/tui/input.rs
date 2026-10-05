//! Input operations shared by terminal events and deterministic interaction tests.
use ratatui::layout::Rect;

use super::app::{App, Focus, RightTab, Role};

#[derive(Clone, Copy)]
pub(super) enum HistoryPane {
    Coder,
    Observer,
    Chat,
}

pub(super) enum Scroll {
    Up(usize),
    Down(usize),
    Top,
    Bottom,
}

pub(super) fn scroll_history(app: &mut App, pane: HistoryPane, action: Scroll, area: Rect) {
    let max_scroll = super::ui::history_scroll_limit(app, pane, area);
    let pane = match pane {
        HistoryPane::Coder => &mut app.coder,
        HistoryPane::Observer => &mut app.observer,
        HistoryPane::Chat => &mut app.chat,
    };
    let current = pane.scroll.min(max_scroll);
    pane.scroll = match action {
        Scroll::Up(lines) => current.saturating_add(lines).min(max_scroll),
        Scroll::Down(lines) => current.saturating_sub(lines),
        Scroll::Top => max_scroll,
        Scroll::Bottom => 0,
    };
}

pub(super) fn scroll_focused_history(app: &mut App, action: Scroll) {
    let pane = match app.focus {
        Focus::Coder => HistoryPane::Coder,
        Focus::Right => match app.right_tab {
            RightTab::Observer => HistoryPane::Observer,
            RightTab::Chat => HistoryPane::Chat,
            _ => return,
        },
    };
    let (width, height) = crossterm::terminal::size().unwrap_or((80, 24));
    scroll_history(app, pane, action, Rect::new(0, 0, width, height));
}

pub(super) fn paste(app: &mut App, text: &str) {
    let pane = match app.focus {
        Focus::Coder => &mut app.coder,
        Focus::Right => match app.right_tab {
            RightTab::Observer => &mut app.observer,
            RightTab::Chat => &mut app.chat,
            _ => return,
        },
    };
    // A pasted newline is text, never an Enter key or an implicit send.
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    pane.textarea.insert_str(text);
    pane.welcome_dismissed = true;
}

pub(super) fn manual_review_prompt(app: &App) -> Option<String> {
    let latest = app.coder.messages.iter().rev().find(|message| {
        message.role == Role::Assistant && message.complete && !message.content.trim().is_empty()
    })?;
    let content = latest.content.chars().take(4000).collect::<String>();
    let instruction = match app.lang.as_str() {
        "ja" => "最新のCoder出力を批評し、不具合・不足した検証・次の一手を挙げてください。",
        "fr" => "Analyse la dernière sortie du Coder : défauts, vérifications manquantes et prochaine étape.",
        _ => "Review the latest Coder output for defects, missing verification, and the next useful step.",
    };
    Some(format!(
        "[MANUAL-OBSERVE] {instruction}\n\nLatest Coder output:\n{content}"
    ))
}
