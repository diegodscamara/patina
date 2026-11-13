//! Automatic updates through Sparkle 2.
//!
//! The release script embeds `Sparkle.framework` in `Contents/Frameworks` and
//! writes the feed URL (`SUFeedURL`) and public EdDSA key (`SUPublicEDKey`)
//! into `Info.plist`. At launch we load the framework and create an
//! `SPUStandardUpdaterController`, which checks the feed on Sparkle's schedule
//! and installs new versions in the background.
//!
//! When the app runs outside a bundle (`cargo run`) there is no framework to
//! load, so updates are simply off.

#[cfg(target_os = "macos")]
mod imp {
    use std::sync::OnceLock;

    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject, Bool};
    use objc2_foundation::{NSBundle, NSString};

    struct Controller(*mut AnyObject);
    // The controller is created once on the main thread and only messaged
    // from the main thread afterwards.
    unsafe impl Send for Controller {}
    unsafe impl Sync for Controller {}

    static CONTROLLER: OnceLock<Option<Controller>> = OnceLock::new();

    fn create() -> Option<Controller> {
        let frameworks = NSBundle::mainBundle().privateFrameworksPath()?;
        let path =
            frameworks.stringByAppendingPathComponent(&NSString::from_str("Sparkle.framework"));
        let bundle = NSBundle::bundleWithPath(&path)?;
        if !unsafe { bundle.load() } {
            tracing::warn!("Sparkle.framework is present but failed to load");
            return None;
        }
        let class = AnyClass::get(c"SPUStandardUpdaterController")?;
        let null: *mut AnyObject = std::ptr::null_mut();
        unsafe {
            let allocated: *mut AnyObject = msg_send![class, alloc];
            let controller: *mut AnyObject = msg_send![
                allocated,
                initWithStartingUpdater: Bool::YES,
                updaterDelegate: null,
                userDriverDelegate: null
            ];
            if controller.is_null() {
                return None;
            }
            tracing::info!("Sparkle updater started");
            Some(Controller(controller))
        }
    }

    pub fn start() {
        CONTROLLER.get_or_init(create);
    }

    pub fn check_now() {
        if let Some(Some(Controller(controller))) = CONTROLLER.get() {
            let null: *mut AnyObject = std::ptr::null_mut();
            unsafe {
                let _: () = msg_send![*controller, checkForUpdates: null];
            }
        }
    }

    pub fn available() -> bool {
        matches!(CONTROLLER.get(), Some(Some(_)))
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    pub fn start() {}
    pub fn check_now() {}
    pub fn available() -> bool {
        false
    }
}

/// Starts Sparkle if the app is running from a bundle that ships it.
pub fn start() {
    imp::start();
}

/// Shows Sparkle's "Check for Updates" flow.
pub fn check_now() {
    imp::check_now();
}

/// Whether the updater is running.
pub fn available() -> bool {
    imp::available()
}
