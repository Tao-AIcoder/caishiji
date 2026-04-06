//! Main REPL event loop.
//! Mirrors Claude Code's screens/REPL.tsx.

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::mpsc;

use crate::{
    api,
    context::build_system_prompt,
    memory::memory_dir,
    messages::{ContentBlock, Message},
    permissions::PermissionChecker,
    query::{run_query, QueryCallbacks, QueryParams},
    state::AppState,
    tools::{default_registry, ToolContext},
    tui::{
        input::{InputAction, InputState},
        renderer,
    },
};

/// Events sent from the async query task to the UI thread.
enum QueryEvent {
    TextChunk(String),
    ToolStart { name: String },
    ToolDone { is_error: bool },
    Done(Vec<Message>, crate::messages::Usage),
    Error(String),
}

/// Launch the interactive REPL. Blocks until the user quits.
pub async fn run_repl(mut app_state: AppState) -> Result<()> {
    // Set up terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = repl_loop(&mut terminal, &mut app_state).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

async fn repl_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app_state: &mut AppState,
) -> Result<()> {
    let mut input = InputState::new();
    let mut scroll_offset: u16 = 0;
    let mut streaming_buffer = String::new();

    // Channel from query task → UI
    let (tx, mut rx) = mpsc::channel::<QueryEvent>(256);

    // Flag to cancel in-flight query
    let cancel_flag = Arc::new(AtomicBool::new(false));

    // Show welcome message
    app_state.push_message(Message::system(
        "采石矶 ready. Type a message and press Enter.",
    ));

    loop {
        // ── Drain query events ───────────────────────────────────────────
        while let Ok(event) = rx.try_recv() {
            match event {
                QueryEvent::TextChunk(chunk) => {
                    streaming_buffer.push_str(&chunk);
                    // Update the last assistant message in place
                    update_streaming_message(app_state, &streaming_buffer);
                }
                QueryEvent::ToolStart { name } => {
                    app_state.push_message(Message::system(format!("⚙ Running {name}…")));
                }
                QueryEvent::ToolDone { is_error } => {
                    if is_error {
                        // Error will be visible in tool_result block
                    }
                }
                QueryEvent::Done(new_messages, usage) => {
                    // Remove streaming placeholder, add real messages
                    remove_streaming_placeholder(app_state);
                    for msg in new_messages {
                        app_state.push_message(msg.clone());
                        if let Message::Assistant { usage: u, .. } = &msg {
                            app_state.add_usage(u);
                        }
                    }
                    app_state.add_usage(&usage);
                    app_state.is_loading = false;
                    app_state.last_error = None;
                    streaming_buffer.clear();
                }
                QueryEvent::Error(err) => {
                    remove_streaming_placeholder(app_state);
                    app_state.last_error = Some(err.clone());
                    app_state.push_message(Message::system(format!("✗ {err}")));
                    app_state.is_loading = false;
                    streaming_buffer.clear();
                }
            }
        }

        // ── Render ───────────────────────────────────────────────────────
        terminal.draw(|f| renderer::render(f, app_state, &input, scroll_offset))?;

        // ── Poll keyboard (non-blocking, 16ms tick) ───────────────────────
        if event::poll(Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                match input.handle_key(key) {
                    InputAction::Quit => break,

                    InputAction::Interrupt => {
                        if app_state.is_loading {
                            cancel_flag.store(true, Ordering::Relaxed);
                            app_state.is_loading = false;
                            streaming_buffer.clear();
                            remove_streaming_placeholder(app_state);
                            app_state.push_message(Message::system("Interrupted."));
                        }
                    }

                    InputAction::Clear => {
                        scroll_offset = 0;
                    }

                    InputAction::Submit(text) => {
                        if app_state.is_loading {
                            continue; // Ignore submit while processing
                        }

                        app_state.last_error = None;
                        app_state.is_loading = true;
                        app_state.push_message(Message::user_text(&text));
                        // Push a streaming placeholder
                        app_state.push_message(Message::assistant(
                            vec![ContentBlock::Text { text: String::new() }],
                            Default::default(),
                        ));

                        // Spawn async query task
                        let history = app_state.api_messages();
                        let system_prompt = build_system_prompt(
                            &app_state.settings,
                            &app_state.working_dir,
                            &memory_dir(),
                        );
                        let settings = app_state.settings.clone();
                        let working_dir = app_state.working_dir.clone();
                        let tx2 = tx.clone();
                        let cancel2 = Arc::clone(&cancel_flag);
                        cancel_flag.store(false, Ordering::Relaxed);

                        tokio::spawn(async move {
                            run_query_task(
                                text,
                                history,
                                system_prompt,
                                settings,
                                working_dir,
                                tx2,
                                cancel2,
                            )
                            .await;
                        });
                    }

                    InputAction::Continue | InputAction::PasteFromClipboard => {}
                }
            }
        }
    }

    Ok(())
}

async fn run_query_task(
    user_text: String,
    history: Vec<crate::messages::ApiMessage>,
    system_prompt: String,
    settings: crate::config::Settings,
    working_dir: std::path::PathBuf,
    tx: mpsc::Sender<QueryEvent>,
    _cancel: Arc<AtomicBool>,
) {
    let provider = match api::from_settings(&settings) {
        Ok(p) => p,
        Err(e) => {
            let _ = tx.send(QueryEvent::Error(e.to_string())).await;
            return;
        }
    };

    let tool_registry = default_registry();
    let permissions = Arc::new(PermissionChecker::new(settings.permission_mode.clone()));
    let tool_ctx = ToolContext {
        working_dir: working_dir.clone(),
        permissions,
        shell: settings.shell.clone(),
    };

    let tx_text = tx.clone();
    let tx_tool_start = tx.clone();
    let tx_tool_done = tx.clone();

    let callbacks = QueryCallbacks {
        on_text: Some(Box::new(move |chunk: &str| {
            let _ = tx_text.blocking_send(QueryEvent::TextChunk(chunk.to_string()));
        })),
        on_tool_start: Some(Box::new(move |name: &str, _id: &str| {
            let _ = tx_tool_start.blocking_send(QueryEvent::ToolStart {
                name: name.to_string(),
            });
        })),
        on_tool_done: Some(Box::new(move |_id: &str, is_error: bool| {
            let _ = tx_tool_done.blocking_send(QueryEvent::ToolDone { is_error });
        })),
    };

    // Remove the streaming placeholder message from history (last user message)
    // History already has the user message appended by the REPL, so we pass
    // everything up to (but not including) the new user message as history.
    let params = QueryParams {
        history: &history,
        new_user_content: vec![ContentBlock::Text { text: user_text }],
        system_prompt,
        model: settings.provider.model.clone(),
        max_tokens: settings.provider.max_tokens,
        provider: provider.as_ref(),
        tool_registry: &tool_registry,
        tool_ctx: &tool_ctx,
        callbacks,
        max_iterations: 30,
    };

    match run_query(params).await {
        Ok(result) => {
            let _ = tx.send(QueryEvent::Done(result.new_messages, result.usage)).await;
        }
        Err(e) => {
            let _ = tx.send(QueryEvent::Error(e.to_string())).await;
        }
    }
}

// ─── Streaming message helpers ────────────────────────────────────────────────

fn update_streaming_message(state: &mut AppState, text: &str) {
    // Find last assistant message and update its text
    for msg in state.messages.iter_mut().rev() {
        if let Message::Assistant { content, .. } = msg {
            if let Some(ContentBlock::Text { text: t }) = content.first_mut() {
                *t = text.to_string();
                return;
            }
        }
    }
}

fn remove_streaming_placeholder(state: &mut AppState) {
    // Remove the last empty assistant message (streaming placeholder)
    if let Some(last) = state.messages.last() {
        if let Message::Assistant { content, .. } = last {
            let is_placeholder = content
                .iter()
                .all(|b| matches!(b, ContentBlock::Text { text } if text.is_empty()));
            if is_placeholder {
                state.messages.pop();
            }
        }
    }
}
