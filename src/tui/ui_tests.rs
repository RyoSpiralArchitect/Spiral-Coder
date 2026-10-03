use super::*;
use crate::modes::Mode;
use ratatui::{backend::TestBackend, Terminal};

fn app() -> App {
    let cfg = RunConfig {
        provider: ProviderKind::Mistral,
        model: "mistral-small-2603".into(),
        chat_model: "mistral-small-2603".into(),
        code_model: "mistral-small-2603".into(),
        api_key: None,
        base_url: "http://localhost:1".into(),
        mode: Mode::Vibe,
        persona: "default".into(),
        temperature: 0.0,
        max_tokens: 1024,
        timeout_seconds: 1,
        hf_device: "cpu".into(),
        hf_local_only: true,
    };
    App::new(
        cfg.clone(),
        cfg.clone(),
        cfg,
        None,
        None,
        false,
        "en".into(),
        None,
    )
}

fn screen(app: &App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| render(frame, app)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn wrapped_message_bottom_shows_latest_content() {
    let mut app = app();
    app.coder
        .push_user(format!("{}TAIL_VISIBLE", "日本語 long text ".repeat(80)));
    let rendered = screen(&app, 80, 24);
    assert!(rendered.contains("TAIL_VISIBLE"), "{rendered}");
}

#[test]
fn picker_keeps_selected_provider_visible() {
    for height in [12, 24] {
        for focus in [Focus::Coder, Focus::Right] {
            for (command, kind) in [
                ("/provider", ActivePicker::Provider),
                ("/model", ActivePicker::Model),
            ] {
                let mut app = app();
                app.focus = focus;
                let items = picker_items(&app, kind);
                let selected = items.len() - 1;
                let pane = app.focused_pane_mut();
                pane.textarea.insert_str(command);
                pane.picker_index = selected;
                let rendered = screen(&app, 80, height);
                assert!(
                    rendered.contains(&format!("› {}", items[selected])),
                    "{rendered}"
                );
            }
        }
    }
}

#[test]
fn render_handles_small_terminal_and_resize() {
    let mut app = app();
    for focus in [Focus::Coder, Focus::Right] {
        app.focus = focus;
        for (width, height) in [(1, 1), (20, 8), (80, 24), (120, 40)] {
            for picker in ["", "/model"] {
                app.coder.textarea = tui_textarea::TextArea::default();
                app.chat.textarea = tui_textarea::TextArea::default();
                app.coder.textarea.insert_str(picker);
                app.chat.textarea.insert_str(picker);
                let _ = screen(&app, width, height);
            }
        }
    }
}
