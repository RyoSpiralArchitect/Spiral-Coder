fn interaction_app() -> (App, tempfile::TempDir) {
    let root = tempfile::tempdir().unwrap();
    let app = App::new(
        test_cfg(Mode::Vibe),
        test_cfg(Mode::Observer),
        test_cfg(Mode::Chat),
        None,
        Some(root.path().display().to_string()),
        false,
        "en".into(),
        None,
    );
    (app, root)
}

fn mouse_at(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

#[test]
fn mouse_focus_uses_rendered_pane_boundary() {
    let (mut app, _root) = interaction_app();
    app.focus = Focus::Right;
    handle_mouse_at(
        mouse_at(MouseEventKind::Down(MouseButton::Left), 45, 6),
        &mut app,
        100,
        24,
    );
    assert_eq!(app.focus, Focus::Right);
}

#[test]
fn mouse_wheel_uses_rendered_pane_boundary() {
    let (mut app, _root) = interaction_app();
    app.focus = Focus::Right;
    app.chat.push_user("scrollable history\n".repeat(40));
    handle_mouse_at(mouse_at(MouseEventKind::ScrollUp, 45, 6), &mut app, 100, 24);
    assert_eq!(app.chat.scroll, 3);
    assert_eq!(app.coder.scroll, 0);
}

#[test]
fn mouse_observer_label_selects_observer() {
    use ratatui::{backend::TestBackend, Terminal};
    for width in [80, 120, 200] {
        for focus in [Focus::Coder, Focus::Right] {
            for (label, expected) in [
                ("Obs", RightTab::Observer),
                ("Chat", RightTab::Chat),
                ("Tasks", RightTab::Tasks),
                ("Review", RightTab::Promotions),
                ("Merge", RightTab::MergeGate),
            ] {
                let (mut app, _root) = interaction_app();
                app.focus = focus;
                app.harness_promotions.summary.needs_review = 12;
                app.merge_gate.summary.needs_review = 23;
                let mut terminal = Terminal::new(TestBackend::new(width, 24)).unwrap();
                terminal
                    .draw(|frame| super::super::ui::render(frame, &app))
                    .unwrap();
                let line = (0..width)
                    .map(|x| terminal.backend().buffer()[(x, 2)].symbol())
                    .collect::<String>();
                let column = (0..width)
                    .find(|start| {
                        label.chars().enumerate().all(|(offset, ch)| {
                            let x = start + offset as u16;
                            x < width
                                && terminal.backend().buffer()[(x, 2)].symbol() == ch.to_string()
                        })
                    })
                    .unwrap_or_else(|| panic!("missing {label}: {line}"));
                handle_mouse_at(
                    mouse_at(MouseEventKind::Down(MouseButton::Left), column, 2),
                    &mut app,
                    width,
                    24,
                );
                assert_eq!(
                    app.right_tab, expected,
                    "width={width} focus={focus:?} label={label}"
                );
            }
        }
    }
}

#[test]
fn mouse_composer_click_preserves_active_pane() {
    let (mut app, _root) = interaction_app();
    handle_mouse_at(
        mouse_at(MouseEventKind::Down(MouseButton::Left), 85, 20),
        &mut app,
        100,
        24,
    );
    assert_eq!(app.focus, Focus::Coder);
}

#[tokio::test]
async fn coder_missing_key_keeps_unsent_draft() {
    let (mut app, _root) = interaction_app();
    let draft = "日本語の下書き\nkeep the second line";
    app.coder.textarea.insert_str(draft);
    let (tx, _rx) = mpsc::channel(8);
    send_coder_message(&mut app, &tx).await;
    assert_eq!(app.coder.textarea.lines().join("\n"), draft);
    assert!(app
        .coder
        .messages
        .iter()
        .any(|message| message.content.contains("missing API key")));
    assert!(!app.coder.streaming);
}

async fn interaction_key(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    let (coder, _coder_rx) = mpsc::channel(8);
    let (observer, _observer_rx) = mpsc::channel(8);
    let (chat, _chat_rx) = mpsc::channel(8);
    let (internal, _internal_rx) = mpsc::channel(8);
    handle_key(
        KeyEvent::new(code, modifiers),
        app,
        &coder,
        &observer,
        &chat,
        &internal,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn manual_observer_review_uses_latest_coder_output_with_empty_draft() {
    let (mut app, _root) = interaction_app();
    app.observer_cfg.api_key = Some("test-placeholder".into());
    app.observer_cfg.base_url = "http://127.0.0.1:1".into();
    app.coder
        .messages
        .push(msg(Role::Assistant, "Latest coder result to review"));
    interaction_key(&mut app, KeyCode::Char('o'), KeyModifiers::CONTROL).await;
    if let Some(task) = app.observer_task.take() {
        task.abort();
    }
    assert!(app
        .observer
        .messages
        .iter()
        .any(|message| message.role == Role::User
            && message.content.contains("Latest coder result to review")));
    assert_eq!(app.right_tab, RightTab::Observer);
}

#[tokio::test]
async fn observer_busy_keeps_next_draft() {
    let (mut app, _root) = interaction_app();
    app.observer_cfg.api_key = Some("test-placeholder".into());
    app.observer.streaming = true;
    app.observer
        .textarea
        .insert_str("次の依頼\nkeep this draft");
    let (tx, _rx) = mpsc::channel(8);
    send_observer_message(&mut app, &tx, None).await;
    assert_eq!(
        app.observer.textarea.lines().join("\n"),
        "次の依頼\nkeep this draft"
    );
    assert!(app.observer.messages.is_empty());
}

#[tokio::test]
async fn home_then_page_down_moves_rendered_history() {
    use ratatui::{backend::TestBackend, Terminal};
    let (mut app, _root) = interaction_app();
    app.coder
        .push_user((0..80).map(|n| format!("row {n:02}\n")).collect());
    let snapshot = |app: &App| {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| super::super::ui::render(frame, app))
            .unwrap();
        (4..18)
            .flat_map(|y| (1..40).map(move |x| (x, y)))
            .map(|position| terminal.backend().buffer()[position].symbol())
            .collect::<String>()
    };
    interaction_key(&mut app, KeyCode::Home, KeyModifiers::NONE).await;
    let top = snapshot(&app);
    interaction_key(&mut app, KeyCode::PageDown, KeyModifiers::NONE).await;
    assert_ne!(top, snapshot(&app));
}

#[test]
fn bracketed_paste_is_a_supported_terminal_event() {
    assert!(matches!(
        terminal_event(Event::Paste("日本語\nsecond line".into())),
        Some(AppEvent::Paste(_))
    ));
}

#[test]
fn multiline_paste_edits_only_the_focused_composer_without_sending() {
    for (focus, tab) in [
        (Focus::Coder, RightTab::Chat),
        (Focus::Right, RightTab::Observer),
        (Focus::Right, RightTab::Chat),
    ] {
        let (mut app, _root) = interaction_app();
        app.focus = focus;
        app.right_tab = tab;
        let Some(AppEvent::Paste(text)) =
            terminal_event(Event::Paste("日本語\r\n/rollback\nlast line\r".into()))
        else {
            panic!("paste must be preserved")
        };
        input::paste(&mut app, &text);
        assert_eq!(
            app.focused_pane_mut().textarea.lines().join("\n"),
            "日本語\n/rollback\nlast line\n"
        );
        assert!(
            app.coder.messages.is_empty()
                && app.observer.messages.is_empty()
                && app.chat.messages.is_empty()
        );
        assert!(!app.coder.streaming && !app.observer.streaming && !app.chat.streaming);
    }
    let (mut app, _root) = interaction_app();
    app.focus = Focus::Right;
    app.right_tab = RightTab::Tasks;
    input::paste(&mut app, "read-only pane");
    assert_eq!(app.observer.textarea.lines(), &[String::new()]);
}

#[tokio::test]
async fn missing_keys_allow_local_help_but_block_all_pane_sends() {
    for provider in [ProviderKind::Mistral, ProviderKind::Anthropic] {
        let (mut app, _root) = interaction_app();
        app.coder_cfg.provider = ProviderKind::Mistral;
        app.observer_cfg.provider = provider.clone();
        app.chat_cfg.provider = provider;
        let (tx, mut rx) = mpsc::channel(16);
        let (internal, mut internal_rx) = mpsc::channel(16);
        for pane in [PaneId::Coder, PaneId::Observer, PaneId::Chat] {
            assert!(handle_slash_command("/keys", &mut app, pane));
            assert!(pane_mut(&mut app, pane)
                .messages
                .last()
                .unwrap()
                .content
                .contains("missing"));
            pane_mut(&mut app, pane)
                .textarea
                .insert_str("unsent draft 日本語");
        }
        send_coder_message(&mut app, &tx).await;
        send_observer_message(&mut app, &tx, None).await;
        send_chat_message(&mut app, &tx, &internal).await;
        send_observer_message(&mut app, &tx, Some("manual review".into())).await;
        send_meta_diagnose(&mut app, &tx, "last-fail").await;
        send_next_action_assist(&mut app, &tx, "last-fail", "failure").await;
        for pane in [PaneId::Coder, PaneId::Observer, PaneId::Chat] {
            assert_eq!(pane_input_text(&app, pane), "unsent draft 日本語");
            assert!(!pane_mut(&mut app, pane).streaming);
        }
        assert!(app.coder_task.is_none() && app.observer_task.is_none() && app.chat_task.is_none());
        assert!(rx.try_recv().is_err() && internal_rx.try_recv().is_err());
    }
}

#[test]
fn base_url_switch_drops_another_endpoint_explicit_key() {
    let (mut app, _root) = interaction_app();
    app.observer_cfg.api_key = Some("original-explicit-placeholder".into());
    assert!(handle_slash_command(
        "/base_url https://api.anthropic.com/v1",
        &mut app,
        PaneId::Observer
    ));
    assert_ne!(
        app.observer_cfg.api_key.as_deref(),
        Some("original-explicit-placeholder")
    );
    assert_eq!(app.observer_cfg.base_url, "https://api.anthropic.com/v1");
}
