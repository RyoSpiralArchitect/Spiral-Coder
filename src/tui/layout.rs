//! Shared geometry for drawing and pointer hit-testing.
use ratatui::layout::{Constraint, Direction, Layout, Rect};

use super::app::{App, Focus, RightTab};

pub(super) struct ScreenLayout {
    pub header: Rect,
    pub coder: Rect,
    pub right_tabs: Rect,
    pub right_content: Rect,
    pub input: Rect,
    pub footer: Rect,
}

impl ScreenLayout {
    pub fn new(area: Rect, app: &App) -> Self {
        let pane = match app.focus {
            Focus::Coder => Some(&app.coder),
            Focus::Right => match app.right_tab {
                RightTab::Observer => Some(&app.observer),
                RightTab::Chat => Some(&app.chat),
                _ => None,
            },
        };
        let picker_open = pane.is_some_and(|pane| {
            matches!(
                pane.textarea.lines().join("\n").trim(),
                "/provider" | "/model"
            )
        });
        let input_height = if picker_open {
            (area.height / 3).clamp(4, 9)
        } else {
            4
        };
        let rows = Layout::vertical([
            Constraint::Length(2),
            Constraint::Min(1),
            Constraint::Length(input_height),
            Constraint::Length(1),
        ])
        .split(area);
        let left = if app.focus == Focus::Right { 40 } else { 55 };
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(left),
                Constraint::Percentage(100 - left),
            ])
            .split(rows[1]);
        let right = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(columns[1]);
        Self {
            header: rows[0],
            coder: columns[0],
            right_tabs: right[0],
            right_content: right[1],
            input: rows[2],
            footer: rows[3],
        }
    }
}

pub(super) struct TabLabel {
    pub tab: RightTab,
    pub text: String,
    pub area: Rect,
}

/// Labels and click targets use the same widths, including count badges.
pub(super) fn right_tab_labels(area: Rect, app: &App) -> Vec<TabLabel> {
    let review_count =
        app.harness_promotions.summary.needs_review + app.harness_promotions.summary.approved;
    let merge_count = app.merge_gate.summary.needs_review
        + app.merge_gate.summary.rollback_available
        + app.merge_gate.summary.blocked;
    let badge = |label: &str, count: usize| {
        if count > 0 {
            format!("[{label} {count}]")
        } else {
            format!("[{label}]")
        }
    };
    let full = [
        "[Observer]".into(),
        "[Chat]".into(),
        "[Tasks]".into(),
        badge("Review", review_count),
        badge("Merge", merge_count),
    ];
    let compact = ["Obs", "Chat", "Tasks", "Review", "Merge"].map(str::to_string);
    let width = |labels: &[String; 5]| labels.iter().map(|label| label.len()).sum::<usize>() + 4;
    let labels = if width(&full) <= area.width as usize {
        full
    } else if width(&compact) <= area.width as usize {
        compact
    } else {
        ["O", "C", "T", "R", "M"].map(str::to_string)
    };
    let tabs = [
        RightTab::Observer,
        RightTab::Chat,
        RightTab::Tasks,
        RightTab::Promotions,
        RightTab::MergeGate,
    ];
    let mut x = area.x;
    tabs.into_iter()
        .zip(labels)
        .map(|(tab, text)| {
            let width = (text.len() as u16).min(area.right().saturating_sub(x));
            let label_area = Rect::new(x, area.y, width, area.height);
            x = x.saturating_add(width).saturating_add(1).min(area.right());
            TabLabel {
                tab,
                text,
                area: label_area,
            }
        })
        .collect()
}

pub(super) fn contains(area: Rect, x: u16, y: u16) -> bool {
    x >= area.x && x < area.right() && y >= area.y && y < area.bottom()
}
