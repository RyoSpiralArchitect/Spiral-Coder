use tokio::task::JoinHandle;
use tui_textarea::TextArea;

use crate::config::RunConfig;
use crate::harness_gate::HarnessPromotionBoard;
use crate::merge_gate::MergeGateBoard;
use crate::streaming::{GovernorState, RealizeState};

use super::agent::RealizePreset;
use super::intent::IntentAnchor;
use super::suggestion::ObserverSuggestionEnvelope;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Coder,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RightTab {
    Observer,
    Chat,
    Tasks,
    Promotions,
    MergeGate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskTarget {
    Coder,
    Observer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskPhase {
    Core,
    Feature,
    Polish,
    Any,
}

#[derive(Debug, Clone)]
pub struct Task {
    pub id: String,
    pub target: TaskTarget,
    pub title: String,
    pub body: String,
    pub phase: TaskPhase,
    pub priority: u8,
    pub done: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub role: Role,
    pub content: String,
    /// Original Observer text before display formatting. Audit-only, never
    /// included in provider history or treated as execution evidence.
    pub raw_content: Option<String>,
    /// False while streaming, true once the full message is received.
    pub complete: bool,
}

impl Message {
    pub fn new_streaming(role: Role) -> Self {
        Self {
            role,
            content: String::new(),
            raw_content: None,
            complete: false,
        }
    }
    pub fn new_complete(role: Role, content: String) -> Self {
        Self {
            role,
            content,
            raw_content: None,
            complete: true,
        }
    }
}

pub struct Pane {
    pub messages: Vec<Message>,
    pub textarea: TextArea<'static>,
    /// Lines from the bottom.  0 = pinned to bottom (auto-scroll).
    /// N = N lines above the bottom (user has scrolled up).
    pub scroll: usize,
    pub streaming: bool,
    /// Optional local filter for history readability (slash command: /find).
    pub find_query: String,
    /// Hide the first-run welcome text once the pane has been used or clicked.
    pub welcome_dismissed: bool,
    /// Shared cursor for lightweight slash pickers.
    pub picker_index: usize,
}

impl Pane {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            textarea: TextArea::default(),
            scroll: 0,
            streaming: false,
            find_query: String::new(),
            welcome_dismissed: false,
            picker_index: 0,
        }
    }

    /// Append a delta to the current in-flight streaming message.
    pub fn push_delta(&mut self, delta: &str) {
        if let Some(last) = self.messages.last_mut() {
            if !last.complete {
                last.content.push_str(delta);
                return;
            }
        }
        // No open streaming message — start one.
        let mut msg = Message::new_streaming(Role::Assistant);
        msg.content.push_str(delta);
        self.messages.push(msg);
    }

    pub fn finish_stream(&mut self) {
        if let Some(last) = self.messages.last_mut() {
            last.complete = true;
        }
        self.streaming = false;
    }

    pub fn push_user(&mut self, text: String) {
        self.welcome_dismissed = true;
        self.messages.push(Message::new_complete(Role::User, text));
    }

    pub fn push_tool(&mut self, text: String) {
        self.messages.push(Message::new_complete(Role::Tool, text));
    }

    /// Collect chat history for the API (User + Assistant only, complete only).
    pub fn chat_history(&self) -> Vec<crate::types::ChatMessage> {
        self.messages
            .iter()
            .filter(|m| matches!(m.role, Role::User | Role::Assistant) && m.complete)
            .map(|m| crate::types::ChatMessage {
                role: match m.role {
                    Role::User => "user".to_string(),
                    Role::Assistant => "assistant".to_string(),
                    Role::Tool => unreachable!(),
                },
                content: m.content.clone(),
            })
            .collect()
    }
}

pub struct App {
    pub coder: Pane,
    pub observer: Pane,
    pub chat: Pane,
    pub focus: Focus,
    pub right_tab: RightTab,
    /// UI / response language hint (ja/en/fr).
    pub lang: String,
    pub auto_observe: bool,
    pub last_auto_obs_idx: Option<usize>,
    pub coder_cfg: RunConfig,
    pub observer_cfg: RunConfig,
    pub chat_cfg: RunConfig,
    pub tool_root: Option<String>,
    pub prefs_root: Option<String>,
    /// Stack label detected at first Coder send (e.g. "Rust · React · git:main").
    pub project_stack_label: Option<String>,
    /// Git commit hash created at the start of the last Coder session (for /rollback).
    pub last_git_checkpoint: Option<String>,
    /// Auto-detected or configured test command from project scan.
    pub project_test_cmd: Option<String>,
    /// When true, Observer review is automatically forwarded to Coder for fixing.
    pub auto_fix_mode: bool,
    /// Pending auto-fix text: set by handle_observer_token, consumed on next Tick.
    pub pending_auto_fix: Option<String>,
    /// Pending advisory-only Observer suggestion for the next Coder continuation turn.
    pub pending_observer_hint: Option<String>,
    /// Pending Observer contract for the next Coder continuation turn.
    pub pending_observer_contract: Option<ObserverSuggestionEnvelope>,
    pub coder_max_iters: Option<usize>,
    pub quit: bool,

    /// 100ms tick counter — drives the streaming spinner animation.
    pub tick_count: u64,

    /// How many agentic tool-call iterations have been completed so far
    /// for the current Coder task (reset to 0 on each new send).
    pub coder_iter: u32,
    /// Latest governor status snapshot from the agentic loop.
    pub coder_governor: Option<GovernorState>,
    /// Session-scoped realize-on-demand preset for the Coder.
    pub coder_realize_preset: RealizePreset,
    /// Latest realize-on-demand status snapshot for the Coder.
    pub coder_realize_state: Option<RealizeState>,
    /// Latest normalized user intent anchor for the Coder.
    pub coder_intent_anchor: Option<IntentAnchor>,
    /// Latest structured next-action suggestion emitted by the Observer.
    pub last_observer_suggestion: Option<ObserverSuggestionEnvelope>,
    /// Active Observer contract currently being answered by the Coder turn.
    pub active_observer_contract: Option<ObserverSuggestionEnvelope>,
    /// First tool action observed while an Observer contract is active.
    pub active_observer_first_tool: Option<(String, String)>,
    /// Latest Observer contract outcome recorded from a Coder turn.
    pub last_observer_contract_outcome: Option<String>,

    /// Running task handles used for Ctrl+K cancellation.
    pub(crate) coder_chunk_reserve: usize,
    pub(crate) coder_chunk_requests: Option<usize>,
    pub(crate) work_budget: crate::work_budget::WorkBudget,
    pub(crate) coder_completed: Option<bool>,
    pub(crate) active_observer_start: std::collections::BTreeSet<String>,
    pub(crate) active_assist_id: Option<u64>,
    pub coder_session: Option<crate::agent_session::AgentSession>,
    pub coder_generation: u64,
    pub observer_generation: u64,
    pub chat_generation: u64,
    pub coder_task: Option<JoinHandle<()>>,
    pub observer_task: Option<JoinHandle<()>>,
    pub chat_task: Option<JoinHandle<()>>,

    /// When true, incoming stream tokens for that pane are silently discarded
    /// (set after Ctrl+K, cleared on the next send).
    pub ignore_coder_tokens: bool,
    pub ignore_observer_tokens: bool,
    pub ignore_chat_tokens: bool,

    /// One-shot anti-loop retry budget for the Observer pane.
    /// Reset to 1 on each new Observer send (manual or auto-observe).
    pub observer_loop_retry_budget: u8,
    /// When set, the next Tick will re-run the Observer with a diff-only instruction
    /// and overwrite the last assistant message (value = similarity 0..1).
    pub observer_loop_pending: Option<f64>,

    /// One-shot language rewrite retry budget for the Observer pane.
    /// If the Observer ignores the requested language (ja/fr), we rewrite once.
    pub observer_lang_retry_budget: u8,
    /// When set, the next Tick will rewrite the last assistant message into this language.
    pub observer_lang_pending: Option<String>,

    /// True while the current Observer response is a `/meta-diagnose` run.
    /// Used to suppress auto-fix / loop-rewrite behaviors on diagnostic JSON.
    pub observer_meta_mode: bool,
    /// True while the current Observer response is a next-action assist run.
    /// Used to suppress auto-fix so suggestions are not fed back as critiques.
    pub observer_next_action_mode: bool,
    /// Transient source-bound request and one explicit action-contract repair.
    pub(crate) observer_assist_request:
        Option<super::events::observer_request::ObserverAssistRequest>,
    /// Dedup key for auto-triggered next-action assists.
    pub last_auto_next_action_sig: Option<String>,

    /// Background task planning state (Chat -> TaskRouter -> Tasks tab).
    pub planning_tasks: bool,
    pub tasks: Vec<Task>,
    pub tasks_cursor: usize,
    pub harness_promotions: HarnessPromotionBoard,
    pub harness_promotions_cursor: usize,
    pub harness_promotions_status: Option<String>,
    pub merge_gate: MergeGateBoard,
    pub merge_gate_cursor: usize,
    pub merge_gate_status: Option<String>,
}

impl App {
    pub fn new(
        coder_cfg: RunConfig,
        observer_cfg: RunConfig,
        chat_cfg: RunConfig,
        tool_root: Option<String>,
        prefs_root: Option<String>,
        auto_observe: bool,
        lang: String,
        coder_max_iters: Option<usize>,
    ) -> Self {
        let l = lang.trim().to_ascii_lowercase();
        let lang = if l == "en" || l == "fr" || l == "ja" {
            l
        } else {
            "en".to_string()
        };
        Self {
            coder: Pane::new(),
            observer: Pane::new(),
            chat: Pane::new(),
            focus: Focus::Coder,
            right_tab: RightTab::Chat,
            lang,
            auto_observe,
            last_auto_obs_idx: None,
            coder_cfg,
            observer_cfg,
            chat_cfg,
            tool_root,
            prefs_root,
            project_stack_label: None,
            last_git_checkpoint: None,
            project_test_cmd: None,
            auto_fix_mode: false,
            pending_auto_fix: None,
            pending_observer_hint: None,
            pending_observer_contract: None,
            coder_max_iters,
            quit: false,
            tick_count: 0,
            coder_iter: 0,
            coder_governor: None,
            coder_realize_preset: RealizePreset::tui_default(),
            coder_realize_state: None,
            coder_intent_anchor: None,
            last_observer_suggestion: None,
            active_observer_contract: None,
            active_observer_first_tool: None,
            last_observer_contract_outcome: None,
            coder_chunk_reserve: 3,
            coder_chunk_requests: None,
            work_budget: crate::work_budget::WorkBudget::default(),
            coder_completed: None,
            active_observer_start: Default::default(),
            active_assist_id: None,
            coder_session: None,
            coder_generation: 0,
            observer_generation: 0,
            chat_generation: 0,
            coder_task: None,
            observer_task: None,
            chat_task: None,
            ignore_coder_tokens: false,
            ignore_observer_tokens: false,
            ignore_chat_tokens: false,
            observer_loop_retry_budget: 0,
            observer_loop_pending: None,
            observer_lang_retry_budget: 0,
            observer_lang_pending: None,
            observer_meta_mode: false,
            observer_next_action_mode: false,
            observer_assist_request: None,
            last_auto_next_action_sig: None,
            planning_tasks: false,
            tasks: Vec::new(),
            tasks_cursor: 0,
            harness_promotions: HarnessPromotionBoard::default(),
            harness_promotions_cursor: 0,
            harness_promotions_status: None,
            merge_gate: MergeGateBoard::default(),
            merge_gate_cursor: 0,
            merge_gate_status: None,
        }
    }

    pub fn focused_pane_mut(&mut self) -> &mut Pane {
        match self.focus {
            Focus::Coder => &mut self.coder,
            Focus::Right => match self.right_tab {
                RightTab::Observer => &mut self.observer,
                RightTab::Chat => &mut self.chat,
                // Tasks tab is read-only; fall back to Observer for generic actions (copy, clear, etc.).
                RightTab::Tasks | RightTab::Promotions | RightTab::MergeGate => &mut self.observer,
            },
        }
    }

    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Coder => Focus::Right,
            Focus::Right => Focus::Coder,
        };
    }

    pub fn cycle_right_tab(&mut self) {
        self.right_tab = match self.right_tab {
            RightTab::Observer => RightTab::Chat,
            RightTab::Chat => RightTab::Tasks,
            RightTab::Tasks => RightTab::Promotions,
            RightTab::Promotions => RightTab::MergeGate,
            RightTab::MergeGate => RightTab::Observer,
        };
    }

    /// Check whether the auto-observe timer should fire.
    ///
    /// Returns `(message_index, content)` if a new completed Coder message
    /// hasn't been observed yet.
    pub fn auto_observe_trigger(&self) -> Option<(usize, String)> {
        if !self.auto_observe {
            return None;
        }
        if self.observer.streaming || self.coder.streaming {
            return None;
        }

        let (idx, msg) = self
            .coder
            .messages
            .iter()
            .enumerate()
            .rev()
            .find(|(_, m)| {
                matches!(m.role, Role::Assistant) && m.complete && m.content.trim().len() > 40
            })?;

        if Some(idx) == self.last_auto_obs_idx {
            return None;
        }
        Some((idx, msg.content.clone()))
    }
}
