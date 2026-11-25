//! Keep AppKit menus, file panels and native materials in the same appearance as Echo.
use crate::material::LookMode;
use objc2::{class, msg_send, runtime::AnyObject};
use objc2_foundation::NSString;

pub fn apply(mode: LookMode) {
    assert!(objc2::MainThreadMarker::new().is_some(), "appearance changes require the main thread");
    // AppKit retains the appearance. Passing nil explicitly restores system following.
    unsafe {
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        let appearance: *mut AnyObject = match mode {
            LookMode::Auto => std::ptr::null_mut(),
            LookMode::Light | LookMode::Dark => {
                let name = NSString::from_str(if mode == LookMode::Dark {
                    "NSAppearanceNameDarkAqua"
                } else { "NSAppearanceNameAqua" });
                let value: *mut AnyObject = msg_send![class!(NSAppearance), appearanceNamed: &*name];
                assert!(!value.is_null(), "AppKit did not provide the requested appearance");
                value
            }
        };
        let _: () = msg_send![app, setAppearance: appearance];
    }
}
