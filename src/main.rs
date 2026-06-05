use clap::Parser;
use std::io::{self, BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;
use ratatui::{
    backend::CrosstermBackend,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap, Gauge},
    layout::{Layout, Constraint, Direction},
    style::{Style, Modifier, Color},
    Terminal,
};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

#[derive(Parser)]
#[command(name = "jp-stock-cli")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(clap::Subcommand, Clone, Debug, PartialEq)]
enum Commands {
    /// Sync historical prices from Yahoo Finance (Incremental)
    SyncYahoo,
    /// Run AI Scout (Analysis & Notification)
    AiScout,
    /// Run Full Automated Trading Lifecycle
    AutoTrader,
    /// View Paper Portfolio
    Portfolio,
    /// Run backtest (demo)
    Backtest,
    /// Exit TUI
    Exit,
}

impl Commands {
    fn description(&self) -> &str {
        match self {
            Commands::SyncYahoo => "【SyncYahoo】\nYahoo Financeから増分データを取得し、Parquetを更新します。",
            Commands::AiScout => "【AiScout】\nテクニカル指標で抽出した銘柄をOllamaが自動分析し、有望な銘柄をスカウトします。",
            Commands::AutoTrader => "【AutoTrader】\nデータの同期、AIスカウト、Discord通知、ペーパートレード実行までを全自動で行います。",
            Commands::Portfolio => "【Portfolio】\n保有中の仮想ポートフォリオを表示し、最新の損益状況を確認します。",
            Commands::Backtest => "【Backtest】\nアルファモデルのパフォーマンスを検証します。",
            Commands::Exit => "【Exit】\nシステムを終了します。",
        }
    }

    fn as_str(&self) -> &str {
        match self {
            Commands::SyncYahoo => "データ同期 (SyncYahoo)",
            Commands::AiScout => "AIスカウト (AiScout)",
            Commands::AutoTrader => "全自動トレーダー (AutoTrader)",
            Commands::Portfolio => "仮想ポートフォリオ (Portfolio)",
            Commands::Backtest => "バックテスト実行 (Backtest)",
            Commands::Exit => "終了 (Exit)",
        }
    }
}

// 通信メッセージ
enum Message {
    Log(String),
    Finished(String, bool),
}

// 実行状態の管理
enum AppStatus {
    Idle,
    Running {
        cmd_name: String,
        progress: u16,
        logs: Vec<String>,
    },
    Finished(String, bool),
}

struct App {
    items: Vec<Commands>,
    state: ListState,
    status: AppStatus,
    rx: Option<Receiver<Message>>,
}

impl App {
    fn new() -> App {
        let items = vec![
            Commands::AutoTrader,
            Commands::SyncYahoo,
            Commands::AiScout,
            Commands::Portfolio,
            Commands::Backtest,
            Commands::Exit,
        ];
        let mut state = ListState::default();
        state.select(Some(0));
        App {
            items,
            state,
            status: AppStatus::Idle,
            rx: None,
        }
    }

    fn next(&mut self) {
        if matches!(self.status, AppStatus::Idle) {
            let i = match self.state.selected() {
                Some(i) => if i >= self.items.len() - 1 { 0 } else { i + 1 },
                None => 0,
            };
            self.state.select(Some(i));
        }
    }

    fn previous(&mut self) {
        if matches!(self.status, AppStatus::Idle) {
            let i = match self.state.selected() {
                Some(i) => if i == 0 { self.items.len() - 1 } else { i - 1 },
                None => 0,
            };
            self.state.select(Some(i));
        }
    }

    fn execute_selected(&mut self) -> bool {
        if let Some(index) = self.state.selected() {
            let cmd = self.items[index].clone();
            if cmd == Commands::Exit {
                return true;
            }

            if matches!(self.status, AppStatus::Running { .. }) {
                return false;
            }

            self.status = AppStatus::Running {
                cmd_name: cmd.as_str().to_string(),
                progress: 5,
                logs: Vec::new(),
            };
            
            let (tx, rx) = mpsc::channel();
            self.rx = Some(rx);

            thread::spawn(move || {
                let bin_name = match cmd {
                    Commands::SyncYahoo => "sync_yahoo",
                    Commands::AiScout => "ai_scout",
                    Commands::AutoTrader => "auto_trader",
                    Commands::Portfolio => "show_portfolio",
                    Commands::Backtest => {
                        let _ = tx.send(Message::Log("🚀 バックテストデモを開始します...".to_string()));
                        thread::sleep(Duration::from_millis(800));
                        let _ = tx.send(Message::Log("📊 データをロード中...".to_string()));
                        thread::sleep(Duration::from_millis(800));
                        let _ = tx.send(Message::Log("✨ 分析完了。".to_string()));
                        let _ = tx.send(Message::Finished("✅ バックテスト完了 (デモ版)".to_string(), true));
                        return;
                    }
                    _ => return,
                };

                let child = Command::new("cargo")
                    .args(["run", "--release", "--bin", bin_name])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn();

                match child {
                    Ok(mut child) => {
                        let stdout = child.stdout.take().expect("Failed to open stdout");
                        let stderr = child.stderr.take().expect("Failed to open stderr");
                        let tx_clone = tx.clone();

                        // Stdout 読み取り用スレッド
                        thread::spawn(move || {
                            let reader = BufReader::new(stdout);
                            for line in reader.lines() {
                                if let Ok(l) = line {
                                    let _ = tx_clone.send(Message::Log(l));
                                }
                            }
                        });

                        // Stderr 読み取り用スレッド
                        let tx_clone_err = tx.clone();
                        thread::spawn(move || {
                            let reader = BufReader::new(stderr);
                            for line in reader.lines() {
                                if let Ok(l) = line {
                                    let _ = tx_clone_err.send(Message::Log(format!("ERR: {}", l)));
                                }
                            }
                        });

                        // プロセス終了待ち
                        let status = child.wait().expect("Failed to wait on child");
                        if status.success() {
                            let _ = tx.send(Message::Finished("✅ 正常に完了しました。".to_string(), true));
                        } else {
                            let _ = tx.send(Message::Finished("❌ 実行エラーが発生しました。".to_string(), false));
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Message::Finished(format!("❌ システムエラー: {}", e), false));
                    }
                }
            });
        }
        false
    }

    fn reset_status(&mut self) {
        self.status = AppStatus::Idle;
    }
}

fn run_tui() -> Result<(), io::Error> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let app = App::new();
    let res = run_app(&mut terminal, app);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err);
    }

    Ok(())
}

fn run_app<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>, mut app: App) -> io::Result<()> {
    loop {
        // 非同期メッセージのチェック
        if let Some(rx) = &app.rx {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    Message::Log(line) => {
                        if let AppStatus::Running { logs, progress, .. } = &mut app.status {
                            logs.push(line.clone());
                            // ログ内容に基づいて進捗率を少しずつ上げる
                            if *progress < 95 {
                                *progress += 2;
                            }
                            // ログが多すぎる場合は古いものを消す
                            if logs.len() > 30 {
                                logs.remove(0);
                            }
                        }
                    }
                    Message::Finished(msg, success) => {
                        app.status = AppStatus::Finished(msg, success);
                        app.rx = None;
                        break;
                    }
                }
            }
        }

        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
                .split(f.size());

            let title = Paragraph::new("📈 JP Stock System - 管理コンソール")
                .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
                .block(Block::default().borders(Borders::ALL).title("システム名称"));
            f.render_widget(title, chunks[0]);

            let body_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(35), Constraint::Percentage(65)].as_ref())
                .split(chunks[1]);

            // 左側: コマンドリスト
            let items: Vec<ListItem> = app
                .items
                .iter()
                .map(|i| ListItem::new(i.as_str()).style(Style::default().fg(Color::White)))
                .collect();

            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title("コマンド (↑/↓/Enter)"))
                .highlight_style(Style::default().bg(Color::Blue).add_modifier(Modifier::BOLD))
                .highlight_symbol("▶ ");

            f.render_stateful_widget(list, body_chunks[0], &mut app.state);

            // 右側: 詳細プレビュー / 実行結果
            match &app.status {
                AppStatus::Idle => {
                    let sel = app.state.selected().unwrap_or(0);
                    let output = Paragraph::new(app.items[sel].description())
                        .block(Block::default().borders(Borders::ALL).title("詳細プレビュー"))
                        .wrap(Wrap { trim: true });
                    f.render_widget(output, body_chunks[1]);
                }
                AppStatus::Running { cmd_name, progress, logs } => {
                    let vertical_chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([
                            Constraint::Length(3),
                            Constraint::Length(3),
                            Constraint::Min(0)
                        ].as_ref())
                        .split(body_chunks[1]);

                    let header = Paragraph::new(format!("⏳ {} を実行中...", cmd_name))
                        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
                        .block(Block::default().borders(Borders::TOP | Borders::LEFT | Borders::RIGHT));
                    f.render_widget(header, vertical_chunks[0]);

                    let gauge = Gauge::default()
                        .block(Block::default().borders(Borders::LEFT | Borders::RIGHT))
                        .gauge_style(Style::default().fg(Color::Yellow).bg(Color::Black))
                        .percent(*progress);
                    f.render_widget(gauge, vertical_chunks[1]);

                    let log_items: Vec<ListItem> = logs
                        .iter()
                        .rev()
                        .take(15)
                        .rev()
                        .map(|l| ListItem::new(l.as_str()).style(Style::default().fg(Color::Gray)))
                        .collect();

                    let log_list = List::new(log_items)
                        .block(Block::default().borders(Borders::ALL).title("実行ログ (最新)"));
                    f.render_widget(log_list, vertical_chunks[2]);
                }
                AppStatus::Finished(msg, success) => {
                    let color = if *success { Color::Green } else { Color::Red };
                    let output = Paragraph::new(format!("{}\n\n[Enterキーで戻る]", msg))
                        .style(Style::default().fg(color))
                        .block(Block::default().borders(Borders::ALL).title("実行結果"))
                        .wrap(Wrap { trim: true });
                    f.render_widget(output, body_chunks[1]);
                }
            };
        })?;

        if event::poll(std::time::Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Down => app.next(),
                    KeyCode::Up => app.previous(),
                    KeyCode::Enter => {
                        match app.status {
                            AppStatus::Idle => {
                                if app.execute_selected() {
                                    return Ok(());
                                }
                            }
                            AppStatus::Finished(_, _) => {
                                app.reset_status();
                            }
                            AppStatus::Running { .. } => {}
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

fn main() {
    let _cli = Cli::parse();
    if let Err(e) = run_tui() {
        eprintln!("Error running TUI: {}", e);
    }
}
