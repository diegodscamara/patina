//! Spotify access: sign-in, playback and the Web API.
//!
//! Playback goes through librespot, which streams straight from Spotify using
//! the signed-in Premium account. Streaming needs a token issued to one of
//! Spotify's own client IDs, so sign-in uses librespot's OAuth flow with
//! librespot's default client ID. After the first sign-in, librespot stores
//! reusable credentials in the app's cache folder and later launches connect
//! without a browser.
//!
//! Library data (liked songs, playlists) comes from the Web API, using tokens
//! from the same session.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use librespot::core::SpotifyUri;
use librespot::core::authentication::Credentials;
use librespot::core::cache::Cache;
use librespot::core::config::SessionConfig;
use librespot::core::session::Session;
use librespot::core::spotify_id::SpotifyId;
use librespot::oauth::OAuthClientBuilder;
use librespot::playback::audio_backend;
use librespot::playback::config::{AudioFormat, Bitrate, PlayerConfig};
use librespot::playback::mixer::NoOpVolume;
use librespot::playback::player::Player;
use rspotify::clients::OAuthClient;
use rspotify::{AuthCodeSpotify, Token};

/// Port for the OAuth redirect listener. Must match librespot's registered
/// redirect pattern (`http://127.0.0.1:<port>/login`).
const OAUTH_PORT: u16 = 8898;

/// Scopes requested at sign-in.
const OAUTH_SCOPES: &[&str] = &[
    "streaming",
    "user-read-email",
    "user-read-private",
    "user-library-read",
    "user-library-modify",
    "playlist-read-private",
    "playlist-read-collaborative",
    "user-read-playback-state",
    "user-modify-playback-state",
    "user-read-currently-playing",
    "user-read-recently-played",
];

/// Scopes for Web API tokens pulled from the session.
const WEB_API_SCOPES: &str =
    "user-library-read,playlist-read-private,playlist-read-collaborative,user-read-private";

/// A track as the UI shows it.
#[derive(Debug, Clone)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub duration_ms: u32,
}

/// A playlist row for the sidebar.
#[derive(Debug, Clone)]
#[allow(dead_code)] // `id` is used once playlists open in the main view
pub struct Playlist {
    pub id: String,
    pub name: String,
}

/// A connected Spotify account with a player attached.
pub struct Spotify {
    session: Session,
    player: Arc<Player>,
}

impl std::fmt::Debug for Spotify {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Spotify")
            .field("username", &self.session.username())
            .finish()
    }
}

fn cache_dir() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("dev", "diegodscamara", "Patina")
        .ok_or_else(|| anyhow!("no home directory"))?;
    Ok(dirs.cache_dir().to_path_buf())
}

fn cache() -> Result<Cache> {
    let dir = cache_dir()?;
    Cache::new(
        Some(dir.clone()),
        Some(dir.clone()),
        Some(dir.join("audio")),
        Some(2 << 30),
    )
    .context("opening cache")
}

/// True when a previous sign-in left reusable credentials on disk.
pub fn has_saved_credentials() -> bool {
    cache().ok().and_then(|c| c.credentials()).is_some()
}

impl Spotify {
    /// Connects with saved credentials, or opens the browser to sign in when
    /// there are none.
    pub async fn connect(allow_browser: bool) -> Result<Self> {
        let cache = cache()?;
        let config = SessionConfig::default();

        let credentials = match cache.credentials() {
            Some(saved) => saved,
            None if allow_browser => {
                let client = OAuthClientBuilder::new(
                    &config.client_id,
                    &format!("http://127.0.0.1:{OAUTH_PORT}/login"),
                    OAUTH_SCOPES.to_vec(),
                )
                .open_in_browser()
                .with_custom_message("Signed in. You can close this tab and go back to Patina.")
                .build()?;
                let token = client.get_access_token_async().await?;
                Credentials::with_access_token(token.access_token)
            }
            None => return Err(anyhow!("not signed in")),
        };

        let session = Session::new(config, Some(cache));
        session.connect(credentials, true).await?;

        let player_config = PlayerConfig {
            bitrate: Bitrate::Bitrate320,
            gapless: true,
            normalisation: true,
            ..PlayerConfig::default()
        };
        let backend = audio_backend::find(None).ok_or_else(|| anyhow!("no audio backend"))?;
        let player = Player::new(
            player_config,
            session.clone(),
            Box::new(NoOpVolume),
            move || backend(None, AudioFormat::default()),
        );

        Ok(Self { session, player })
    }

    pub fn username(&self) -> String {
        self.session.username()
    }

    async fn web_api(&self) -> Result<AuthCodeSpotify> {
        let token = self
            .session
            .token_provider()
            .get_token(WEB_API_SCOPES)
            .await?;
        Ok(AuthCodeSpotify::from_token(Token {
            access_token: token.access_token,
            ..Token::default()
        }))
    }

    /// The first page of the user's liked songs.
    pub async fn liked_songs(&self) -> Result<Vec<Track>> {
        let api = self.web_api().await?;
        let page = api
            .current_user_saved_tracks_manual(None, Some(50), Some(0))
            .await?;
        Ok(page
            .items
            .into_iter()
            .filter_map(|saved| {
                let track = saved.track;
                let id = track.id?.to_string();
                Some(Track {
                    id,
                    title: track.name,
                    artist: track
                        .artists
                        .iter()
                        .map(|a| a.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    duration_ms: track.duration.num_milliseconds().max(0) as u32,
                })
            })
            .collect())
    }

    /// The user's playlists, for the sidebar.
    pub async fn playlists(&self) -> Result<Vec<Playlist>> {
        let api = self.web_api().await?;
        let page = api.current_user_playlists_manual(Some(50), Some(0)).await?;
        Ok(page
            .items
            .into_iter()
            .map(|p| Playlist {
                id: p.id.to_string(),
                name: p.name,
            })
            .collect())
    }

    /// Starts a track. `uri` is a `spotify:track:...` URI.
    pub fn play_track(&self, uri: &str) -> Result<()> {
        let base62 = uri.rsplit(':').next().unwrap_or(uri);
        let id = SpotifyId::from_base62(base62).map_err(|e| anyhow!("bad track id: {e}"))?;
        self.player.load(SpotifyUri::Track { id }, true, 0);
        Ok(())
    }

    pub fn pause(&self) {
        self.player.pause();
    }

    pub fn resume(&self) {
        self.player.play();
    }
}
