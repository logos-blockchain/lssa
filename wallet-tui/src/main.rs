use std::io;
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState},
    Terminal,
};

#[derive(Debug, Clone)]
struct Account {
    name: String,
    balance: u64,
}

enum AppState {
    AccountsList,
    AccountActions(usize),
}

struct App {
    accounts: Vec<Account>,
    selected: usize,
    state: AppState,
    logs: Vec<String>,
    action_state: ListState,
}

impl App {
    fn new() -> Self {
        let mut state = ListState::default();
        state.select(Some(0));
        Self {
            accounts: vec![],
            selected: 0,
            state: AppState::AccountsList,
            logs: vec!["Welcome to Wallet TUI!".to_string()],
            action_state: state,
        }
    }

    fn create_account(&mut self, name: &str) {
        self.accounts.push(Account {
            name: name.to_string(),
            balance: 100,
        });
        self.logs.push(format!("Created account '{}'", name));
    }

    fn send_tokens(&mut self, from: usize, to: usize, amount: u64) {
        if from == to {
            self.logs.push("Can't send tokens to the same account.".into());
            return;
        }

        let (first, second) = if from < to {
            let (left, right) = self.accounts.split_at_mut(to);
            (&mut left[from], &mut right[0])
        } else {
            let (left, right) = self.accounts.split_at_mut(from);
            (&mut right[0], &mut left[to])
        };

        if first.balance >= amount {
            first.balance -= amount;
            second.balance += amount;
            self.logs.push(format!(
                "Sent {} tokens from {} to {}",
                amount, first.name, second.name
            ));
        } else {
            self.logs.push("Insufficient balance.".into());
        }
    }
}

fn main() -> Result<(), io::Error> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut app = App::new();

    // Start with a couple of accounts
    app.create_account("Alice");
    app.create_account("Bob");

    let res = run_app(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err);
    }

    Ok(())
}

fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> io::Result<()> {
    loop {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints(
                    [Constraint::Percentage(70), Constraint::Percentage(30)].as_ref(),
                )
                .split(f.size());

            match app.state {
                AppState::AccountsList => {
                    let items: Vec<ListItem> = app
                        .accounts
                        .iter()
                        .enumerate()
                        .map(|(i, acc)| {
                            let prefix = if i == app.selected { "> " } else { "  " };
                            ListItem::new(format!(
                                "{}{} (balance: {})",
                                prefix, acc.name, acc.balance
                            ))
                        })
                        .collect();

                    let accounts = List::new(items)
                        .block(Block::default().title("Accounts").borders(Borders::ALL));
                    f.render_widget(accounts, chunks[0]);
                }
                AppState::AccountActions(idx) => {
                    let acc = &app.accounts[idx];
                    let actions = vec![ListItem::new("Send Tokens"), ListItem::new("Back")];

                    let list = List::new(actions)
                        .block(
                            Block::default()
                                .title(format!("Actions for {}", acc.name))
                                .borders(Borders::ALL),
                        )
                        .highlight_style(
                            Style::default()
                                .bg(Color::Blue)
                                .fg(Color::White)
                                .add_modifier(Modifier::BOLD),
                        )
                        .highlight_symbol("➤ ");

                    f.render_stateful_widget(list, chunks[0], &mut app.action_state);
                }
            }

            let log_lines: Vec<ListItem> = app
                .logs
                .iter()
                .rev()
                .take(5)
                .map(|l| ListItem::new(l.clone()))
                .collect();

            let logs = List::new(log_lines)
                .block(Block::default().title("Logs").borders(Borders::ALL));
            f.render_widget(logs, chunks[1]);
        })?;

        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                match app.state {
                    AppState::AccountsList => match key.code {
                        KeyCode::Char('q') => return Ok(()),
                        KeyCode::Down => {
                            if app.selected + 1 < app.accounts.len() {
                                app.selected += 1;
                            }
                        }
                        KeyCode::Up => {
                            if app.selected > 0 {
                                app.selected -= 1;
                            }
                        }
                        KeyCode::Enter => {
                            app.state = AppState::AccountActions(app.selected);
                            app.action_state.select(Some(0)); // reset selection
                        }
                        KeyCode::Char('n') => {
                            let name = format!("Account{}", app.accounts.len() + 1);
                            app.create_account(&name);
                        }
                        _ => {}
                    },
                    AppState::AccountActions(idx) => match key.code {
                        KeyCode::Esc => app.state = AppState::AccountsList,
                        KeyCode::Down => {
                            let i = match app.action_state.selected() {
                                Some(i) if i < 1 => i + 1,
                                _ => 1,
                            };
                            app.action_state.select(Some(i));
                        }
                        KeyCode::Up => {
                            let i = match app.action_state.selected() {
                                Some(i) if i > 0 => i - 1,
                                _ => 0,
                            };
                            app.action_state.select(Some(i));
                        }
                        KeyCode::Enter => {
                            match app.action_state.selected() {
                                Some(0) => {
                                    if app.accounts.len() > 1 {
                                        let target = (idx + 1) % app.accounts.len();
                                        app.send_tokens(idx, target, 10);
                                    } else {
                                        app.logs.push(
                                            "No other account to send tokens to.".into(),
                                        );
                                    }
                                }
                                Some(1) => {
                                    // Back
                                }
                                _ => {}
                            }
                            app.state = AppState::AccountsList;
                        }
                        _ => {}
                    },
                }
            }
        }
    }
}

