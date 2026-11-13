//! Media keys and the Now Playing widget in Control Center (macOS).
//!
//! `start()` registers handlers with `MPRemoteCommandCenter`, so the play/pause,
//! next and previous keys (and AirPods, the Touch Bar and Control Center) reach
//! Patina. Commands come back through `commands()`, a stream the app subscribes
//! to. `publish()` tells `MPNowPlayingInfoCenter` what's playing, which macOS
//! shows in Control Center and uses to route the media keys to this app.
//!
//! Other platforms get no-op stand-ins.

use iced::futures::Stream;

/// A command from the media keys or Control Center.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub enum Command {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
    /// Seek to this position, in milliseconds.
    Seek(u32),
}

/// What Control Center shows.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub struct NowPlaying<'a> {
    pub title: &'a str,
    pub artist: &'a str,
    pub album: &'a str,
    pub duration_ms: u32,
    pub position_ms: u32,
    pub playing: bool,
}

#[cfg(target_os = "macos")]
mod imp {
    use std::ptr::NonNull;
    use std::sync::{Mutex, OnceLock};

    use block2::RcBlock;
    use iced::futures::channel::mpsc::{self, UnboundedReceiver, UnboundedSender};
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2_foundation::{NSDictionary, NSNumber, NSString};
    use objc2_media_player::{
        MPChangePlaybackPositionCommandEvent, MPMediaItemPropertyAlbumTitle,
        MPMediaItemPropertyArtist, MPMediaItemPropertyPlaybackDuration, MPMediaItemPropertyTitle,
        MPNowPlayingInfoCenter, MPNowPlayingInfoPropertyElapsedPlaybackTime,
        MPNowPlayingInfoPropertyPlaybackRate, MPNowPlayingPlaybackState, MPRemoteCommand,
        MPRemoteCommandCenter, MPRemoteCommandEvent, MPRemoteCommandHandlerStatus,
    };

    use super::{Command, NowPlaying};

    static STARTED: OnceLock<()> = OnceLock::new();
    static RECEIVER: Mutex<Option<UnboundedReceiver<Command>>> = Mutex::new(None);

    type Translate = fn(NonNull<MPRemoteCommandEvent>) -> Option<Command>;

    /// Adds a handler that turns `command`'s events into `Command`s on `tx`.
    fn listen(command: &MPRemoteCommand, tx: &UnboundedSender<Command>, translate: Translate) {
        let tx = tx.clone();
        let handler =
            RcBlock::new(
                move |event: NonNull<MPRemoteCommandEvent>| match translate(event) {
                    Some(command) if tx.unbounded_send(command).is_ok() => {
                        MPRemoteCommandHandlerStatus::Success
                    }
                    _ => MPRemoteCommandHandlerStatus::CommandFailed,
                },
            );
        unsafe {
            command.setEnabled(true);
            // The command keeps the handler for the life of the app; we never
            // remove it, so the returned target token isn't needed.
            let _target = command.addTargetWithHandler(&handler);
        }
    }

    pub fn start() {
        STARTED.get_or_init(|| {
            let (tx, rx) = mpsc::unbounded();
            if let Ok(mut slot) = RECEIVER.lock() {
                *slot = Some(rx);
            }
            let center = unsafe { MPRemoteCommandCenter::sharedCommandCenter() };
            let (play, pause, toggle, next, previous, position) = unsafe {
                (
                    center.playCommand(),
                    center.pauseCommand(),
                    center.togglePlayPauseCommand(),
                    center.nextTrackCommand(),
                    center.previousTrackCommand(),
                    center.changePlaybackPositionCommand(),
                )
            };
            listen(&play, &tx, |_| Some(Command::Play));
            listen(&pause, &tx, |_| Some(Command::Pause));
            listen(&toggle, &tx, |_| Some(Command::Toggle));
            listen(&next, &tx, |_| Some(Command::Next));
            listen(&previous, &tx, |_| Some(Command::Previous));
            listen(&position, &tx, |event| {
                // This command only ever sends position events.
                let event = event.cast::<MPChangePlaybackPositionCommandEvent>();
                let seconds = unsafe { event.as_ref().positionTime() };
                Some(Command::Seek((seconds.max(0.0) * 1000.0) as u32))
            });
        });
    }

    /// The receiving end, handed out once.
    pub fn take_receiver() -> Option<UnboundedReceiver<Command>> {
        RECEIVER.lock().ok().and_then(|mut slot| slot.take())
    }

    fn object<T: objc2::Message>(value: Retained<T>) -> Retained<AnyObject> {
        // Every Objective-C object is an `AnyObject`.
        unsafe { Retained::cast_unchecked(value) }
    }

    pub fn publish(now: Option<NowPlaying<'_>>) {
        let center = unsafe { MPNowPlayingInfoCenter::defaultCenter() };
        let Some(now) = now else {
            unsafe {
                center.setNowPlayingInfo(None);
                center.setPlaybackState(MPNowPlayingPlaybackState::Stopped);
            }
            return;
        };

        let keys: [&NSString; 6] = unsafe {
            [
                MPMediaItemPropertyTitle,
                MPMediaItemPropertyArtist,
                MPMediaItemPropertyAlbumTitle,
                MPMediaItemPropertyPlaybackDuration,
                MPNowPlayingInfoPropertyElapsedPlaybackTime,
                MPNowPlayingInfoPropertyPlaybackRate,
            ]
        };
        let values = [
            object(NSString::from_str(now.title)),
            object(NSString::from_str(now.artist)),
            object(NSString::from_str(now.album)),
            object(NSNumber::new_f64(f64::from(now.duration_ms) / 1000.0)),
            object(NSNumber::new_f64(f64::from(now.position_ms) / 1000.0)),
            object(NSNumber::new_f64(if now.playing { 1.0 } else { 0.0 })),
        ];
        let info = NSDictionary::from_retained_objects(&keys, &values);
        let state = if now.playing {
            MPNowPlayingPlaybackState::Playing
        } else {
            MPNowPlayingPlaybackState::Paused
        };
        unsafe {
            center.setNowPlayingInfo(Some(&info));
            center.setPlaybackState(state);
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::{Command, NowPlaying};

    pub fn start() {}

    pub fn take_receiver() -> Option<iced::futures::channel::mpsc::UnboundedReceiver<Command>> {
        None
    }

    pub fn publish(_now: Option<NowPlaying<'_>>) {}
}

pub use imp::{publish, start};

/// Commands from the media keys, for `Subscription::run`.
pub fn commands() -> impl Stream<Item = Command> {
    use iced::futures::StreamExt;
    match imp::take_receiver() {
        Some(rx) => rx.left_stream(),
        None => iced::futures::stream::pending().right_stream(),
    }
}
