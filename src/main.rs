//! Patina: a small, native Spotify player for macOS.

mod icons;
mod spotify;
mod theme;
mod updater;

use std::collections::HashMap;
use std::sync::Arc;

use iced::widget::{
    Space, button, column, container, image, row, scrollable, slider, svg, text, text_input,
};
use iced::{Alignment, Background, Border, Element, Length, Size, Task, Theme};

use spotify::{PlaybackEvent, Playlist, Spotify, Track};

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
    Search,
    NowPlaying,
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
    QueryChanged(String),
    Search,
    SearchLoaded(Result<Vec<Track>, String>),
    PlayLiked(usize),
    PlayResult(usize),
    TogglePlay,
    Next,
    Previous,
    Playback(PlaybackEvent),
    Seeking(u32),
    SeekDone,
    ArtLoaded(String, Result<Vec<u8>, String>),
    CheckForUpdates,
}

struct Patina {
    account: Account,
    page: Page,
    liked: Vec<Track>,
    playlists: Vec<Playlist>,
    query: String,
    results: Vec<Track>,
    searching: bool,
    /// What's playing now and what comes next: the list the track was started from.
    queue: Vec<Track>,
    current: Option<usize>,
    playing: bool,
    position_ms: u32,
    /// Where the user is dragging the progress slider, before release.
    seek_to: Option<u32>,
    art: HashMap<String, image::Handle>,
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
            query: String::new(),
            results: Vec::new(),
            searching: false,
            queue: Vec::new(),
            current: None,
            playing: false,
            position_ms: 0,
            seek_to: None,
            art: HashMap::new(),
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

    fn client(&self) -> Option<Arc<Spotify>> {
        match &self.account {
            Account::Connected(client) => Some(client.clone()),
            _ => None,
        }
    }

    fn now_playing(&self) -> Option<&Track> {
        self.current.and_then(|i| self.queue.get(i))
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
                let lists = client.clone();
                Task::batch([
                    Task::perform(
                        async move { liked.liked_songs().await.map_err(|e| e.to_string()) },
                        Message::LikedLoaded,
                    ),
                    Task::perform(
                        async move { lists.playlists().await.map_err(|e| e.to_string()) },
                        Message::PlaylistsLoaded,
                    ),
                    Task::run(client.events(), Message::Playback),
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
            Message::QueryChanged(query) => {
                self.query = query;
                Task::none()
            }
            Message::Search => {
                let (Some(client), query) = (self.client(), self.query.trim().to_string()) else {
                    return Task::none();
                };
                if query.is_empty() {
                    return Task::none();
                }
                self.searching = true;
                Task::perform(
                    async move { client.search(&query).await.map_err(|e| e.to_string()) },
                    Message::SearchLoaded,
                )
            }
            Message::SearchLoaded(result) => {
                self.searching = false;
                match result {
                    Ok(tracks) => self.results = tracks,
                    Err(e) => self.error = Some(e),
                }
                Task::none()
            }
            Message::PlayLiked(index) => {
                self.queue = self.liked.clone();
                self.play(index)
            }
            Message::PlayResult(index) => {
                self.queue = self.results.clone();
                self.play(index)
            }
            Message::TogglePlay => {
                let Some(client) = self.client() else {
                    return Task::none();
                };
                if self.playing {
                    client.pause();
                    self.playing = false;
                } else if self.current.is_some() {
                    client.resume();
                    self.playing = true;
                } else if !self.liked.is_empty() {
                    self.queue = self.liked.clone();
                    return self.play(0);
                }
                Task::none()
            }
            Message::Next => self.advance(),
            Message::Previous => match self.current {
                // Like most players: restart the track unless we're at its very start.
                Some(_) if self.position_ms > 3000 => {
                    if let Some(client) = self.client() {
                        client.seek(0);
                    }
                    self.position_ms = 0;
                    Task::none()
                }
                Some(i) => self.play(i.saturating_sub(1)),
                None => Task::none(),
            },
            Message::Playback(event) => match event {
                PlaybackEvent::Playing { position_ms } => {
                    self.playing = true;
                    self.position_ms = position_ms;
                    Task::none()
                }
                PlaybackEvent::Paused { position_ms } => {
                    self.playing = false;
                    self.position_ms = position_ms;
                    Task::none()
                }
                PlaybackEvent::Position { position_ms } => {
                    self.position_ms = position_ms;
                    Task::none()
                }
                PlaybackEvent::EndOfTrack | PlaybackEvent::Unavailable => self.advance(),
            },
            Message::Seeking(ms) => {
                self.seek_to = Some(ms);
                Task::none()
            }
            Message::SeekDone => {
                if let (Some(ms), Some(client)) = (self.seek_to.take(), self.client()) {
                    client.seek(ms);
                    self.position_ms = ms;
                }
                Task::none()
            }
            Message::ArtLoaded(url, result) => {
                match result {
                    Ok(bytes) => {
                        self.art.insert(url, image::Handle::from_bytes(bytes));
                    }
                    Err(e) => tracing::warn!("album art failed: {e}"),
                }
                Task::none()
            }
            Message::CheckForUpdates => {
                updater::check_now();
                Task::none()
            }
        }
    }

    /// Plays `queue[index]` and fetches its cover if we don't have it yet.
    fn play(&mut self, index: usize) -> Task<Message> {
        let (Some(client), Some(track)) = (self.client(), self.queue.get(index).cloned()) else {
            return Task::none();
        };
        match client.play_track(&track.id) {
            Ok(()) => {
                self.current = Some(index);
                self.playing = true;
                self.position_ms = 0;
                self.seek_to = None;
            }
            Err(e) => {
                self.error = Some(e.to_string());
                return Task::none();
            }
        }

        match track.art_url {
            Some(url) if !self.art.contains_key(&url) => Task::perform(
                {
                    let url = url.clone();
                    async move { Spotify::fetch_image(&url).await.map_err(|e| e.to_string()) }
                },
                move |result| Message::ArtLoaded(url.clone(), result),
            ),
            _ => Task::none(),
        }
    }

    /// Moves to the next track in the queue, or stops at the end.
    fn advance(&mut self) -> Task<Message> {
        match self.current {
            Some(i) if i + 1 < self.queue.len() => self.play(i + 1),
            _ => {
                self.playing = false;
                Task::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let content: Element<'_, Message> = match (&self.account, self.page) {
            (_, Page::Settings) => self.settings(),
            (Account::Connected(_), Page::Library) => self.library(),
            (Account::Connected(_), Page::Search) => self.search(),
            (Account::Connected(_), Page::NowPlaying) => self.now_playing_page(),
            (account, _) => sign_in(account),
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
            nav("Search", Page::Search),
            nav("Now playing", Page::NowPlaying),
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

    /// A list of tracks; `play` builds the message for a clicked row.
    fn track_list<'a>(
        &'a self,
        tracks: &'a [Track],
        play: impl Fn(usize) -> Message,
    ) -> Element<'a, Message> {
        let playing_id = self.now_playing().map(|t| t.id.as_str());
        let mut rows = column![].spacing(2);
        for (i, track) in tracks.iter().enumerate() {
            let current = playing_id == Some(track.id.as_str());
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
                        .width(Length::FillPortion(3)),
                        text(&track.album)
                            .size(13)
                            .color(theme::ASH)
                            .width(Length::FillPortion(2)),
                        text(format_duration(track.duration_ms))
                            .size(13)
                            .color(theme::ASH),
                    ]
                    .spacing(12)
                    .align_y(Alignment::Center),
                )
                .width(Length::Fill)
                .padding([6, 12])
                .style(move |_: &Theme, status| nav_style(current, status))
                .on_press(play(i)),
            );
        }
        scrollable(rows).height(Length::Fill).into()
    }

    fn library(&self) -> Element<'_, Message> {
        let mut page = column![text("Liked Songs").size(30).color(theme::BONE)].spacing(16);
        if let Some(error) = &self.error {
            page = page.push(text(error).size(13).color(theme::ASH));
        }
        page.push(self.track_list(&self.liked, Message::PlayLiked))
            .into()
    }

    fn search(&self) -> Element<'_, Message> {
        let input = text_input("Songs, artists, albums", &self.query)
            .on_input(Message::QueryChanged)
            .on_submit(Message::Search)
            .padding(12)
            .size(16);

        let status = if self.searching {
            text("Searching…").size(13).color(theme::ASH)
        } else if self.results.is_empty() && !self.query.is_empty() {
            text("Press Return to search.").size(13).color(theme::ASH)
        } else {
            text("")
        };

        column![
            text("Search").size(30).color(theme::BONE),
            container(input).max_width(560),
            status,
            self.track_list(&self.results, Message::PlayResult),
        ]
        .spacing(16)
        .into()
    }

    fn now_playing_page(&self) -> Element<'_, Message> {
        let Some(track) = self.now_playing() else {
            return container(text("Nothing playing yet.").size(16).color(theme::ASH))
                .center(Length::Fill)
                .into();
        };

        let cover: Element<'_, Message> = match track.art_url.as_ref().and_then(|u| self.art.get(u))
        {
            Some(handle) => image(handle.clone()).width(400).height(400).into(),
            None => container(Space::new())
                .width(400)
                .height(400)
                .style(|_| surface(theme::RAISED))
                .into(),
        };

        let mut up_next = column![text("UP NEXT").size(11).color(theme::ASH)].spacing(8);
        let start = self.current.map_or(0, |i| i + 1);
        for next in self.queue.iter().skip(start).take(8) {
            up_next = up_next.push(column![
                text(&next.title).size(14),
                text(&next.artist).size(12).color(theme::ASH),
            ]);
        }

        row![
            column![
                cover,
                text(&track.title).size(28).color(theme::BONE),
                text(format!("{} · {}", track.artist, track.album))
                    .size(15)
                    .color(theme::ASH),
            ]
            .spacing(14)
            .width(Length::Fill)
            .align_x(Alignment::Center),
            container(scrollable(up_next))
                .width(320)
                .height(Length::Fill)
                .padding(20)
                .style(|_| container::Style {
                    background: Some(Background::Color(theme::SURFACE)),
                    border: Border::default().rounded(16),
                    ..container::Style::default()
                }),
        ]
        .spacing(40)
        .into()
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
        let now = self.now_playing();

        let thumb: Element<'_, Message> = match now
            .and_then(|t| t.art_url.as_ref())
            .and_then(|u| self.art.get(u))
        {
            Some(handle) => image(handle.clone()).width(52).height(52).into(),
            None => container(Space::new())
                .width(52)
                .height(52)
                .style(|_| surface(theme::RAISED))
                .into(),
        };

        let info = button(
            row![
                thumb,
                column![
                    text(now.map_or("Nothing playing", |t| t.title.as_str())).size(14),
                    text(now.map_or("", |t| t.artist.as_str()))
                        .size(12)
                        .color(theme::ASH),
                ],
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        )
        .padding(0)
        .style(|_: &Theme, _| button::Style {
            text_color: theme::BONE,
            ..button::Style::default()
        })
        .on_press(Message::Navigate(Page::NowPlaying))
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

        let duration = now.map_or(0, |t| t.duration_ms);
        let position = self.seek_to.unwrap_or(self.position_ms).min(duration);
        let progress = row![
            text(format_duration(position)).size(11).color(theme::ASH),
            slider(0..=duration.max(1), position, Message::Seeking)
                .on_release(Message::SeekDone)
                .step(1000u32)
                .style(|theme: &Theme, status| {
                    let mut style = slider::default(theme, status);
                    style.rail.backgrounds = (
                        Background::Color(theme::OLIVE),
                        Background::Color(theme::RAISED),
                    );
                    style.handle.background = Background::Color(theme::BONE);
                    style
                }),
            text(format_duration(duration)).size(11).color(theme::ASH),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .padding([0, 40]);

        let center = column![controls, progress]
            .spacing(6)
            .align_x(Alignment::Center)
            .width(Length::Fill);

        let bar = row![info, center, Space::new().width(300)]
            .align_y(Alignment::Center)
            .padding([0, 20]);

        container(bar)
            .height(88)
            .center_y(88)
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

#[cfg(test)]
mod tests {
    use super::format_duration;

    #[test]
    fn formats_durations() {
        assert_eq!(format_duration(0), "0:00");
        assert_eq!(format_duration(59_999), "0:59");
        assert_eq!(format_duration(318_000), "5:18");
    }
}
