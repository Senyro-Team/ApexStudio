use apex_application::ApplicationServices;
use apex_core::{Error, Project};
use clap::{Parser, Subcommand};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};
use std::{
    fmt,
    io::{self, IsTerminal},
    path::PathBuf,
    process::ExitCode,
};

const APP_IDENTIFIER: &str = "studio.apexdesigns.apexstudio";

#[derive(Debug, Parser)]
#[command(
    name = "apex-studio-cli",
    version,
    about = "Apex Studio terminal workspace"
)]
struct Args {
    /// Use a specific Apex Studio workspace directory.
    #[arg(long, global = true)]
    workspace: Option<PathBuf>,

    /// Print command results as JSON.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Browse and inspect projects.
    Projects {
        #[command(subcommand)]
        action: ProjectCommand,
    },
    /// Show the workspace name and storage summary.
    Workspace,
}

#[derive(Debug, Subcommand)]
enum ProjectCommand {
    /// List projects, optionally filtering by a case-insensitive substring.
    List {
        #[arg(long, short)]
        search: Option<String>,
    },
    /// Show a project's details by its ID.
    Show { id: String },
}

fn main() -> ExitCode {
    let args = Args::parse();
    let json = args.json;
    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if json {
                eprintln!(
                    "{}",
                    serde_json::json!({
                        "ok": false,
                        "error": { "code": error.code, "message": error.message }
                    })
                );
            } else {
                eprintln!("Error: {}", error.message);
            }
            ExitCode::FAILURE
        }
    }
}

#[derive(Debug)]
struct CliError {
    code: String,
    message: String,
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl From<Error> for CliError {
    fn from(error: Error) -> Self {
        Self {
            code: error.code().into(),
            message: error.to_string(),
        }
    }
}

fn run(args: Args) -> Result<(), CliError> {
    let root = args
        .workspace
        .or_else(default_workspace_path)
        .ok_or_else(|| {
            CliError {
                code: "workspace_path_unavailable".into(),
                message: "Could not find this user's local application data directory. Use --workspace <PATH>.".into(),
            }
        })?;
    let app = ApplicationServices::open(&root).map_err(CliError::from)?;

    match args.command {
        Some(Command::Projects { action }) => match action {
            ProjectCommand::List { search } => {
                let mut projects = app.projects().map_err(CliError::from)?;
                if let Some(search) = search.filter(|query| !query.trim().is_empty()) {
                    let query = search.trim().to_lowercase();
                    projects.retain(|project| project_matches(project, &query));
                }
                print_projects(&projects, args.json);
            }
            ProjectCommand::Show { id } => {
                let project = app.project(&id).map_err(CliError::from)?;
                print_project(&project, args.json);
            }
        },
        Some(Command::Workspace) => {
            let info = app.workspace_info().map_err(CliError::from)?;
            if args.json {
                println!(
                    "{}",
                    serde_json::to_string(&info).map_err(|error| CliError {
                        code: "serialization_error".into(),
                        message: error.to_string(),
                    })?
                );
            } else {
                println!("{}", info.name);
                println!("Path: {}", info.root);
                println!("Database schema: {}", info.schema_version);
                println!("Managed files: {}", info.managed_file_count);
            }
        }
        None => {
            if args.json {
                return Err(CliError {
                    code: "invalid_mode".into(),
                    message: "--json requires a non-interactive command.".into(),
                });
            }
            if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
                return Err(CliError {
                    code: "interactive_terminal_required".into(),
                    message: "Run this command in a terminal, or use a command such as `projects list --json`.".into(),
                });
            }
            run_tui(app).map_err(|error| CliError {
                code: "terminal_error".into(),
                message: error.to_string(),
            })?;
        }
    }
    Ok(())
}

fn default_workspace_path() -> Option<PathBuf> {
    dirs::data_local_dir().map(|data| data.join(APP_IDENTIFIER).join("workspace"))
}

fn project_matches(project: &Project, query: &str) -> bool {
    project.fields.name.to_lowercase().contains(query)
        || project.fields.description.to_lowercase().contains(query)
        || project
            .fields
            .tags
            .iter()
            .any(|tag| tag.to_lowercase().contains(query))
        || project
            .fields
            .vehicles
            .iter()
            .any(|vehicle| vehicle.to_lowercase().contains(query))
}

fn print_projects(projects: &[Project], json: bool) {
    if json {
        match serde_json::to_string(projects) {
            Ok(value) => println!("{value}"),
            Err(error) => eprintln!("Could not encode project list as JSON: {error}"),
        }
        return;
    }
    if projects.is_empty() {
        println!("No projects found.");
        return;
    }
    for project in projects {
        let state = if project.archived {
            "archived"
        } else {
            "active"
        };
        println!(
            "{}\t{}\t{} assets",
            project.id, project.fields.name, project.asset_count
        );
        println!("  {state} · updated {}", project.updated_at);
    }
}

fn print_project(project: &Project, json: bool) {
    if json {
        match serde_json::to_string(project) {
            Ok(value) => println!("{value}"),
            Err(error) => eprintln!("Could not encode project as JSON: {error}"),
        }
        return;
    }
    println!("{}", project.fields.name);
    println!("ID: {}", project.id);
    println!("Created: {}", project.created_at);
    println!("Updated: {}", project.updated_at);
    println!(
        "Status: {}",
        if project.archived {
            "archived"
        } else {
            "active"
        }
    );
    println!("Assets: {}", project.asset_count);
    if !project.fields.description.is_empty() {
        println!("Description: {}", project.fields.description);
    }
    if !project.fields.tags.is_empty() {
        println!("Tags: {}", project.fields.tags.join(", "));
    }
    if !project.fields.vehicles.is_empty() {
        println!("Vehicles: {}", project.fields.vehicles.join(", "));
    }
    if !project.fields.reference_links.is_empty() {
        println!("Reference links:");
        for link in &project.fields.reference_links {
            println!("  {link}");
        }
    }
    if let Some(deadline) = &project.fields.deadline {
        println!("Deadline: {deadline}");
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Screen {
    Home,
    Projects,
    Detail,
}

struct TuiState {
    screen: Screen,
    projects: Vec<Project>,
    selected: usize,
    search: String,
    searching: bool,
    selected_project: Option<String>,
    no_color: bool,
}

impl TuiState {
    fn new(projects: Vec<Project>) -> Self {
        Self {
            screen: Screen::Home,
            projects,
            selected: 0,
            search: String::new(),
            searching: false,
            selected_project: None,
            no_color: std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty()),
        }
    }

    fn visible_projects(&self) -> Vec<&Project> {
        let query = self.search.to_lowercase();
        self.projects
            .iter()
            .filter(|project| query.is_empty() || project_matches(project, &query))
            .collect()
    }
}

fn run_tui(app: ApplicationServices) -> io::Result<()> {
    let projects = app.projects().map_err(io::Error::other)?;
    let mut state = TuiState::new(projects);
    ratatui::run(|terminal| loop {
        terminal.draw(|frame| draw(frame, &state))?;
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                    break Ok(());
                }
                match state.screen {
                    Screen::Home => match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break Ok(()),
                        KeyCode::Up | KeyCode::Char('k') => state.selected = 0,
                        KeyCode::Down | KeyCode::Char('j') => state.selected = 1,
                        KeyCode::Enter if state.selected == 0 => {
                            state.screen = Screen::Projects;
                            state.selected = 0;
                            refresh_projects(&mut state, &app).map_err(io::Error::other)?;
                        }
                        KeyCode::Enter => break Ok(()),
                        _ => {}
                    },
                    Screen::Projects if !state.searching && key.code == KeyCode::Char('r') => {
                        refresh_projects(&mut state, &app).map_err(io::Error::other)?;
                    }
                    Screen::Projects => handle_project_key(&mut state, key.code),
                    Screen::Detail => match key.code {
                        KeyCode::Esc | KeyCode::Backspace => state.screen = Screen::Projects,
                        KeyCode::Char('q') => break Ok(()),
                        _ => {}
                    },
                }
            }
            Event::Resize(_, _) => {}
            _ => {}
        }
    })
}

fn refresh_projects(state: &mut TuiState, app: &ApplicationServices) -> Result<(), Error> {
    let selected_id = state
        .visible_projects()
        .get(state.selected)
        .map(|project| project.id.clone());
    state.projects = app.projects()?;
    let visible = state.visible_projects();
    state.selected = selected_id
        .and_then(|id| visible.iter().position(|project| project.id == id))
        .unwrap_or_else(|| state.selected.min(visible.len().saturating_sub(1)));
    Ok(())
}

fn handle_project_key(state: &mut TuiState, key: KeyCode) {
    if state.searching {
        match key {
            KeyCode::Esc | KeyCode::Enter => state.searching = false,
            KeyCode::Backspace => {
                state.search.pop();
                state.selected = 0;
            }
            KeyCode::Char(value) if !value.is_control() => {
                state.search.push(value);
                state.selected = 0;
            }
            _ => {}
        }
        return;
    }
    match key {
        KeyCode::Esc | KeyCode::Backspace => {
            state.screen = Screen::Home;
            state.search.clear();
            state.selected = 0;
        }
        KeyCode::Char('q') => state.screen = Screen::Home,
        KeyCode::Char('/') => state.searching = true,
        KeyCode::Up | KeyCode::Char('k') => {
            state.selected = state.selected.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let max = state.visible_projects().len().saturating_sub(1);
            state.selected = (state.selected + 1).min(max);
        }
        KeyCode::Enter => {
            state.selected_project = state
                .visible_projects()
                .get(state.selected)
                .map(|project| project.id.clone());
            if state.selected_project.is_some() {
                state.screen = Screen::Detail;
            }
        }
        _ => {}
    }
}

fn draw(frame: &mut Frame<'_>, state: &TuiState) {
    let area = frame.area();
    let regions = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(2),
        ])
        .split(area);
    let accent = if state.no_color {
        Color::Reset
    } else {
        Color::Rgb(112, 104, 255)
    };
    let muted = if state.no_color {
        Color::Reset
    } else {
        Color::DarkGray
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " APEX ",
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled("STUDIO", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled("  ·  Workspace CLI", Style::default().fg(muted)),
        ]))
        .block(Block::default().borders(Borders::ALL)),
        regions[0],
    );
    match state.screen {
        Screen::Home => draw_home(frame, state, regions[1], accent, muted),
        Screen::Projects => draw_projects(frame, state, regions[1], accent, muted),
        Screen::Detail => draw_detail(frame, state, regions[1], accent, muted),
    }
    let footer = match state.screen {
        Screen::Home => " ↑/↓ Select   Enter Open   q Quit ",
        Screen::Projects if state.searching => " Type to filter   Enter/Esc Finish search ",
        Screen::Projects => {
            " ↑/↓ Select   Enter Details   / Search   r Refresh   Esc Back   q Back "
        }
        Screen::Detail => " Esc Back   q Quit ",
    };
    frame.render_widget(
        Paragraph::new(footer)
            .style(Style::default().fg(muted))
            .alignment(ratatui::layout::Alignment::Center),
        regions[2],
    );
}

fn draw_home(
    frame: &mut Frame<'_>,
    state: &TuiState,
    area: ratatui::layout::Rect,
    accent: Color,
    muted: Color,
) {
    let sections = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);
    let items = vec![ListItem::new("Projects"), ListItem::new("Quit")];
    let mut list_state = ListState::default().with_selected(Some(state.selected.min(1)));
    frame.render_stateful_widget(
        List::new(items)
            .block(Block::default().title("Menu").borders(Borders::ALL))
            .highlight_symbol("▶ ")
            .highlight_style(Style::default().fg(accent).add_modifier(Modifier::BOLD)),
        sections[0],
        &mut list_state,
    );
    frame.render_widget(
        Paragraph::new("Browse and search your projects.")
            .block(Block::default().title("Apex Studio").borders(Borders::ALL))
            .style(Style::default().fg(muted))
            .wrap(Wrap { trim: true }),
        sections[1],
    );
}

fn draw_projects(
    frame: &mut Frame<'_>,
    state: &TuiState,
    area: ratatui::layout::Rect,
    accent: Color,
    muted: Color,
) {
    let (list_area, detail_area) = if area.width < 88 {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
            .split(area);
        (rows[0], rows[1])
    } else {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(area);
        (columns[0], columns[1])
    };
    let visible = state.visible_projects();
    let query_label = if state.searching {
        "Search (editing)"
    } else {
        "Search"
    };
    let search = if state.search.is_empty() {
        "Type / to search"
    } else {
        state.search.as_str()
    };
    let title = format!("Projects · {}", visible.len());
    let items = visible
        .iter()
        .map(|project| {
            let status = if project.archived { " [archived]" } else { "" };
            ListItem::new(format!("{}{}", project.fields.name, status))
        })
        .collect::<Vec<_>>();
    let mut list_state = ListState::default()
        .with_selected((!visible.is_empty()).then_some(state.selected.min(visible.len() - 1)));
    frame.render_stateful_widget(
        List::new(items)
            .block(Block::default().title(title).borders(Borders::ALL))
            .highlight_symbol("▶ ")
            .highlight_style(Style::default().fg(accent).add_modifier(Modifier::BOLD)),
        list_area,
        &mut list_state,
    );
    let detail = visible.get(state.selected.min(visible.len().saturating_sub(1)));
    let detail_text = if let Some(project) = detail {
        format!(
            "{}\n\n{}\n\n{} assets\nCreated {}\nUpdated {}\n\nTags: {}\nVehicles: {}",
            project.fields.name,
            if project.fields.description.is_empty() {
                "No description"
            } else {
                &project.fields.description
            },
            project.asset_count,
            project.created_at,
            project.updated_at,
            if project.fields.tags.is_empty() {
                "—".into()
            } else {
                project.fields.tags.join(", ")
            },
            if project.fields.vehicles.is_empty() {
                "—".into()
            } else {
                project.fields.vehicles.join(", ")
            },
        )
    } else if state.projects.is_empty() {
        "No projects yet.".to_owned()
    } else {
        "No projects match this search.".to_owned()
    };
    let detail_lines = vec![
        Line::styled(query_label, Style::default().fg(muted)),
        Line::raw(search),
        Line::raw(""),
        Line::raw(detail_text),
    ];
    frame.render_widget(
        Paragraph::new(detail_lines)
            .block(
                Block::default()
                    .title("Project details")
                    .borders(Borders::ALL),
            )
            .wrap(Wrap { trim: true }),
        detail_area,
    );
}

fn draw_detail(
    frame: &mut Frame<'_>,
    state: &TuiState,
    area: ratatui::layout::Rect,
    accent: Color,
    muted: Color,
) {
    let project = state
        .selected_project
        .as_deref()
        .and_then(|id| state.projects.iter().find(|project| project.id == id));
    let contents = if let Some(project) = project {
        vec![
            Line::styled(
                &project.fields.name,
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            ),
            Line::raw(format!("ID: {}", project.id)),
            Line::raw(format!(
                "Status: {}",
                if project.archived {
                    "Archived"
                } else {
                    "Active"
                }
            )),
            Line::raw(format!("Created: {}", project.created_at)),
            Line::raw(format!("Updated: {}", project.updated_at)),
            Line::raw(format!("Assets: {}", project.asset_count)),
            Line::raw(format!(
                "Deadline: {}",
                project.fields.deadline.as_deref().unwrap_or("—")
            )),
            Line::raw(""),
            Line::raw(if project.fields.description.is_empty() {
                "No description"
            } else {
                &project.fields.description
            }),
            Line::raw(""),
            Line::raw(format!(
                "Tags: {}",
                if project.fields.tags.is_empty() {
                    "—".into()
                } else {
                    project.fields.tags.join(", ")
                }
            )),
            Line::raw(format!(
                "Vehicles: {}",
                if project.fields.vehicles.is_empty() {
                    "—".into()
                } else {
                    project.fields.vehicles.join(", ")
                }
            )),
            Line::raw(format!(
                "Reference links: {}",
                project.fields.reference_links.len()
            )),
        ]
    } else {
        vec![Line::raw("Project is no longer available.")]
    };
    frame.render_widget(
        Paragraph::new(contents)
            .block(Block::default().title("Project").borders(Borders::ALL))
            .style(Style::default().fg(muted))
            .wrap(Wrap { trim: true }),
        area,
    );
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use apex_core::ProjectInput;
    use ratatui::backend::TestBackend;

    #[test]
    fn project_filter_matches_name_description_tags_and_vehicle() {
        let project = Project {
            id: "p1".into(),
            fields: ProjectInput {
                name: "Rodenauer Land".into(),
                description: "EMS fleet".into(),
                tags: vec!["rescue".into()],
                vehicles: vec!["Falcon Interceptor Utility".into()],
                ..Default::default()
            },
            created_at: "2026-11-08".into(),
            updated_at: "2026-11-08".into(),
            archived: false,
            asset_count: 0,
        };
        for query in ["rodenauer", "EMS", "RESCUE", "falcon"] {
            assert!(project_matches(&project, &query.to_lowercase()));
        }
        assert!(!project_matches(&project, "fire"));
    }

    #[test]
    fn project_screen_filters_typing_and_opens_selected_project() {
        let project = Project {
            id: "p1".into(),
            fields: ProjectInput {
                name: "EMS Fleet".into(),
                ..Default::default()
            },
            created_at: "2026-11-08".into(),
            updated_at: "2026-11-08".into(),
            archived: false,
            asset_count: 0,
        };
        let mut state = TuiState::new(vec![project]);
        state.screen = Screen::Projects;
        handle_project_key(&mut state, KeyCode::Char('/'));
        handle_project_key(&mut state, KeyCode::Char('e'));
        handle_project_key(&mut state, KeyCode::Char('m'));
        handle_project_key(&mut state, KeyCode::Char('s'));
        assert_eq!(state.visible_projects().len(), 1);
        handle_project_key(&mut state, KeyCode::Enter);
        handle_project_key(&mut state, KeyCode::Enter);
        assert_eq!(state.screen, Screen::Detail);
        assert_eq!(state.selected_project.as_deref(), Some("p1"));
    }

    #[test]
    fn tui_draws_in_narrow_and_standard_terminal_sizes() {
        for (width, height) in [(40, 12), (80, 24), (120, 36)] {
            let backend = TestBackend::new(width, height);
            let mut terminal = ratatui::Terminal::new(backend).expect("test terminal");
            let state = TuiState::new(Vec::new());
            terminal.draw(|frame| draw(frame, &state)).expect("render");
            assert_eq!(terminal.backend().buffer().area.width, width);
            assert_eq!(terminal.backend().buffer().area.height, height);
        }
    }
}
