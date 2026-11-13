//! Inline SVG icons. `currentColor` is replaced by the widget's style colour.

use iced::widget::svg::Handle;

const LOGO: &[u8] = include_bytes!("../assets/icon/patina.svg");

const PLAY: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M8 5.5v13l11-6.5z" fill="currentColor"/></svg>"#;
const PAUSE: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="6.5" y="5" width="3.6" height="14" rx="1" fill="currentColor"/><rect x="13.9" y="5" width="3.6" height="14" rx="1" fill="currentColor"/></svg>"#;
const PREVIOUS: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M6 5v14" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/><path d="M19 5.5v13L9 12z" fill="currentColor"/></svg>"#;
const NEXT: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M18 5v14" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/><path d="M5 5.5v13L15 12z" fill="currentColor"/></svg>"#;

const VOLUME: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M4 9.5h3.5L12 5.5v13l-4.5-4H4z" fill="currentColor"/><path d="M15.5 9a4.2 4.2 0 0 1 0 6M18 6.5a7.8 7.8 0 0 1 0 11" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/></svg>"#;

pub fn logo() -> Handle {
    Handle::from_memory(LOGO)
}

pub fn play() -> Handle {
    Handle::from_memory(PLAY)
}

pub fn pause() -> Handle {
    Handle::from_memory(PAUSE)
}

pub fn previous() -> Handle {
    Handle::from_memory(PREVIOUS)
}

pub fn next() -> Handle {
    Handle::from_memory(NEXT)
}

pub fn volume() -> Handle {
    Handle::from_memory(VOLUME)
}
