//! Patina: a small, native Spotify player for macOS.

mod icons;
mod spotify;
mod theme;
mod updater;

use std::sync::Arc;

use iced::widget::{Space, button, column, container, row, scrollable, svg, text};
use iced::{Alignment, Background, Border, Element, Length, Size, Task, Theme};

use spotify::{Playlist, Spotify, Track};

pub fn main() -> iced::Result {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "patina=info,librespot=warn".into()),
        )
        .init();

    iced::application(Patina::boot, Patina::update, Patina::view)
        .title("Patina")
        .theme(|_: &Patina| theme::theme())
        .window_size(Size::new(1180.0, 760.0))
        .run()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Library,
    Settings,
}

#[derive(Debug)]
enum Account {
    SignedOut,
    Connecting,
    Connected(Arc<Spotify>),
    Failed(String),
}

#[derive(Debug, Clone)]
enum Message {
    SignIn,
    Connected(Result<Arc<Spotify>, String>),
    LikedLoaded(Result<Vec<Track>, String>),
    PlaylistsLoaded(Result<Vec<Playlist>, String>),
    Navigate(Page),
    PlayIndex(usize),
    TogglePlay,
    Next,
    Previous,
    CheckForUpdates,
}

struct Patina {
    account: Account,
    page: Page,
    liked: Vec<Track>,
    playlists: Vec<Playlist>,
    current: Option<usize>,
    playing: bool,
    error: Option<String>,
}

impl Patina {
    fn boot() -> (Self, Task<Message>) {
        updater::start();

        let mut state = Self {
            account: Account::SignedOut,
            page: Page::Library,
            liked: Vec::new(),
            playlists: Vec::new(),
            current: None,
            playing: false,
            error: None,
        };

        // Reconnect silently when a previous sign-in left credentials behind.
        let task = if spotify::has_saved_credentials() {
            state.account = Account::Connecting;
            connect(false)
        } else {
            Task::none()
        };
        (state, task)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SignIn => {
                self.account = Account::Connecting;
                connect(true)
            }
            Message::Connected(Ok(client)) => {
                self.account = Account::Connected(client.clone());
                let liked = client.clone();
                Task::batch([
                    Task::perform(
                        async move { liked.liked_songs().await.map_err(|e| e.to_string()) },
                        Message::LikedLoaded,
                    ),
                    Task::perform(
                        async move { client.playlists().await.map_err(|e| e.to_string()) },
                        Message::PlaylistsLoaded,
                    ),
                ])
            }
            Message::Connected(Err(e)) => {
                self.account = Account::Failed(e);
                Task::none()
            }
            Message::LikedLoaded(result) => {
                match result {
                    Ok(tracks) => self.liked = tracks,
                    Err(e) => self.error = Some(e),
                }
                Task::none()
            }
            Message::PlaylistsLoaded(result) => {
                match result {
                    Ok(lists) => self.playlists = lists,
                    Err(e) => self.error = Some(e),
                }
                Task::none()
            }
            Message::Navigate(page) => {
                self.page = page;
                Task::none()
            }
            Message::PlayIndex(index) => {
                self.play(index);
                Task::none()
            }
            Message::TogglePlay => {
                if let Account::Connected(client) = &self.account {
                    if self.playing {
                        client.pause();
                        self.playing = false;
                    } else if self.current.is_some() {
                        client.resume();
                        self.playing = true;
                    } else if !self.liked.is_empty() {
                        self.play(0);
                    }
                }
                Task::none()
            }
            Message::Next => {
                if let Some(i) = self.current
                    && i + 1 < self.liked.len()
                {
                    self.play(i + 1);
                }
                Task::none()
            }
            Message::Previous => {
                if let Some(i) = self.current {
                    self.play(i.saturating_sub(1));
                }
                Task::none()
            }
            Message::CheckForUpdates => {
                updater::check_now();
                Task::none()
            }
        }
    }

    fn play(&mut self, index: usize) {
        let (Account::Connected(client), Some(track)) = (&self.account, self.liked.get(index))
        else {
            return;
        };
        match client.play_track(&track.id) {
            Ok(()) => {
                self.current = Some(index);
                self.playing = true;
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let content: Element<'_, Message> = match (&self.account, self.page) {
            (_, Page::Settings) => self.settings(),
            (Account::Connected(_), Page::Library) => self.library(),
            (account, Page::Library) => sign_in(account),
        };

        let body = row![
            self.sidebar(),
            container(content)
                .padding([24, 32])
                .width(Length::Fill)
                .height(Length::Fill),
        ]
        .height(Length::Fill);

        column![body, self.player_bar()].into()
    }

    fn sidebar(&self) -> Element<'_, Message> {
        let nav = |label: &'static str, page: Page| {
            let selected = self.page == page;
            button(text(label).size(14))
                .width(Length::Fill)
                .padding([8, 10])
                .style(move |_: &Theme, status| nav_style(selected, status))
                .on_press(Message::Navigate(page))
        };

        let mut lists = column![].spacing(2);
        for playlist in &self.playlists {
            lists = lists
                .push(container(text(&playlist.name).size(13).color(theme::ASH)).padding([5, 10]));
        }

        let sidebar = column![
            row![
                svg(icons::logo()).width(28).height(28),
                text("Patina").size(20).color(theme::BONE),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            Space::new().height(12),
            nav("Library", Page::Library),
            Space::new().height(16),
            text("PLAYLISTS").size(11).color(theme::ASH),
            scrollable(lists).height(Length::Fill),
            nav("Settings", Page::Settings),
        ]
        .spacing(4)
        .padding([20, 12]);

        container(sidebar)
            .width(240)
            .height(Length::Fill)
            .style(|_| surface(theme::SURFACE))
            .into()
    }

    fn library(&self) -> Element<'_, Message> {
        let mut rows = column![].spacing(2);
        for (i, track) in self.liked.iter().enumerate() {
            let current = self.current == Some(i);
            let color = if current { theme::OLIVE } else { theme::BONE };
            rows = rows.push(
                button(
                    row![
                        text(format!("{}", i + 1))
                            .size(13)
                            .color(theme::ASH)
                            .width(36),
                        column![
                            text(&track.title).size(14).color(color),
                            text(&track.artist).size(12).color(theme::ASH),
                        ]
                        .width(Length::Fill),
                        text(format_duration(track.duration_ms))
                            .size(13)
                            .color(theme::ASH),
                    ]
                    .align_y(Alignment::Center),
                )
                .width(Length::Fill)
                .padding([6, 12])
                .style(move |_: &Theme, status| nav_style(current, status))
                .on_press(Message::PlayIndex(i)),
            );
        }

        let mut page = column![text("Liked Songs").size(30).color(theme::BONE)].spacing(16);
        if let Some(error) = &self.error {
            page = page.push(text(error).size(13).color(theme::ASH));
        }
        page.push(scrollable(rows).height(Length::Fill)).into()
    }

    fn settings(&self) -> Element<'_, Message> {
        let account = match &self.account {
            Account::Connected(client) => format!("Signed in as {}", client.username()),
            _ => "Not signed in".to_string(),
        };
        let updates = if updater::available() {
            "Updates install automatically from GitHub Releases."
        } else {
            "Updates are off when running outside the installed app."
        };

        column![
            text("Settings").size(30).color(theme::BONE),
            text(account).size(14),
            Space::new().height(8),
            text(format!("Version {}", env!("CARGO_PKG_VERSION"))).size(14),
            text(updates).size(13).color(theme::ASH),
            button(text("Check for updates").size(13))
                .padding([6, 12])
                .on_press_maybe(updater::available().then_some(Message::CheckForUpdates)),
        ]
        .spacing(10)
        .into()
    }

    fn player_bar(&self) -> Element<'_, Message> {
        let now = self.current.and_then(|i| self.liked.get(i));
        let info = column![
            text(now.map_or("Nothing playing", |t| t.title.as_str())).size(14),
            text(now.map_or("", |t| t.artist.as_str()))
                .size(12)
                .color(theme::ASH),
        ]
        .width(300);

        let control = |icon: svg::Handle, message: Message, size: u32| {
            button(svg(icon).width(size).height(size).style(|_, _| svg::Style {
                color: Some(theme::BONE),
            }))
            .padding(6)
            .style(|_: &Theme, _| button::Style::default())
            .on_press(message)
        };

        let play_pause = button(
            svg(if self.playing {
                icons::pause()
            } else {
                icons::play()
            })
            .width(18)
            .height(18)
            .style(|_, _| svg::Style {
                color: Some(theme::GRAPHITE),
            }),
        )
        .padding(10)
        .style(|_: &Theme, _| button::Style {
            background: Some(Background::Color(theme::BONE)),
            border: Border::default().rounded(19),
            ..button::Style::default()
        })
        .on_press(Message::TogglePlay);

        let controls = row![
            control(icons::previous(), Message::Previous, 20),
            play_pause,
            control(icons::next(), Message::Next, 20),
        ]
        .spacing(18)
        .align_y(Alignment::Center);

        let bar = row![
            info,
            container(controls).center_x(Length::Fill),
            Space::new().width(300),
        ]
        .align_y(Alignment::Center)
        .padding([0, 20]);

        container(bar)
            .height(80)
            .center_y(80)
            .width(Length::Fill)
            .style(|_| surface(theme::SURFACE))
            .into()
    }
}

fn sign_in(account: &Account) -> Element<'_, Message> {
    let (status, can_sign_in) = match account {
        Account::Connecting => ("Connecting to Spotify…".to_string(), false),
        Account::Failed(e) => (format!("Couldn't connect: {e}"), true),
        _ => (
            "Sign in with your Spotify Premium account to start listening.".to_string(),
            true,
        ),
    };

    let button = button(text("Sign in with Spotify").size(14).color(theme::GRAPHITE))
        .padding([10, 20])
        .style(|_: &Theme, _| button::Style {
            background: Some(Background::Color(theme::OLIVE)),
            border: Border::default().rounded(20),
            ..button::Style::default()
        })
        .on_press_maybe(can_sign_in.then_some(Message::SignIn));

    container(
        column![
            svg(icons::logo()).width(96).height(96),
            text("Patina").size(32),
            text(status).size(14).color(theme::ASH),
            button,
        ]
        .spacing(16)
        .align_x(Alignment::Center),
    )
    .center(Length::Fill)
    .into()
}

fn connect(allow_browser: bool) -> Task<Message> {
    Task::perform(
        async move {
            Spotify::connect(allow_browser)
                .await
                .map(Arc::new)
                .map_err(|e| e.to_string())
        },
        Message::Connected,
    )
}

fn surface(color: iced::Color) -> container::Style {
    container::Style {
        background: Some(Background::Color(color)),
        ..container::Style::default()
    }
}

fn nav_style(selected: bool, status: button::Status) -> button::Style {
    let background = match (selected, status) {
        (true, _) => Some(Background::Color(theme::RAISED)),
        (false, button::Status::Hovered) => Some(Background::Color(theme::BORDER)),
        _ => None,
    };
    button::Style {
        background,
        text_color: if selected { theme::BONE } else { theme::ASH },
        border: Border::default().rounded(8),
        ..button::Style::default()
    }
}

fn format_duration(ms: u32) -> String {
    let seconds = ms / 1000;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}
