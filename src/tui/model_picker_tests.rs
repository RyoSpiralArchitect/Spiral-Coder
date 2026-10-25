#[test]
fn model_picker_applies_and_persists_current_catalog_in_every_pane() {
    use crate::config::ProviderPreset;
    for preset in [
        ProviderPreset::OpenAi,
        ProviderPreset::Gemini,
        ProviderPreset::AnthropicCompat,
        ProviderPreset::Mistral,
    ] {
        for pane in [PaneId::Coder, PaneId::Observer, PaneId::Chat] {
            let dir = tempfile::tempdir().unwrap();
            let mut cfg = test_cfg(Mode::Vibe);
            cfg.provider = preset.provider_kind();
            cfg.base_url = preset.default_base_url().unwrap().into();
            let mut app = App::new(
                cfg.clone(),
                cfg.clone(),
                cfg,
                None,
                Some(dir.path().to_string_lossy().into_owned()),
                false,
                "en".into(),
                None,
            );
            for (index, model) in preset.representative_models().iter().enumerate() {
                set_pane_input_text(&mut app, pane, "/model");
                pane_mut(&mut app, pane).picker_index = 0;
                for _ in 0..index {
                    assert!(adjust_inline_picker(&mut app, pane, 1));
                }
                let previous = pane_cfg(&app, pane).model.clone();
                assert!(apply_inline_picker(&mut app, pane));
                if *model == "other" {
                    assert_eq!(pane_input_text(&app, pane), "/model ");
                    assert_eq!(pane_cfg(&app, pane).model, previous);
                    continue;
                }
                assert_eq!(pane_cfg(&app, pane).model, *model);
                assert_eq!(
                    pane_cfg(&app, pane).base_url,
                    preset.default_base_url().unwrap()
                );
                let saved = prefs::load_prefs(app.prefs_root.as_deref()).unwrap();
                let saved_pane = match pane {
                    PaneId::Coder => saved.coder,
                    PaneId::Observer => saved.observer,
                    PaneId::Chat => saved.chat,
                };
                assert_eq!(saved_pane.model.as_deref(), Some(*model));
                assert_eq!(
                    saved_pane.provider.as_deref(),
                    Some(preset.provider_kind().key())
                );
            }
            assert!(handle_slash_command(
                "/model custom-deployment-2026",
                &mut app,
                pane
            ));
            assert_eq!(pane_cfg(&app, pane).model, "custom-deployment-2026");
        }
    }
}
