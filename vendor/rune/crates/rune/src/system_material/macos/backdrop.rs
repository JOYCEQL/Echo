//! Native window coating, following Yab's WindowBackdrop/BackdropCoatingView.
//! Separate sibling NSViews keep the HUD, veil and gradient in AppKit's layer
//! tree. No color pixels are painted into Rune's Metal window surface.
use super::super::BackdropFinish;
use super::{rect, AnyClass, AnyObject, Retained, CGPoint, msg_send};
use crate::WindowBackdrop;

fn view(class: &std::ffi::CStr) -> Retained<AnyObject> {
    // SAFETY: called only by the window's main-thread material transaction.
    unsafe {
        let class = AnyClass::get(class).expect("required native backdrop class");
        let alloc: *mut AnyObject = msg_send![class, alloc];
        let value: *mut AnyObject = msg_send![alloc, initWithFrame: rect(0., 0., 0., 0.)];
        let value = Retained::from_raw(value).expect("native backdrop view");
        let _: () = msg_send![&*value, setWantsLayer: true];
        let _: () = msg_send![&*value, setAutoresizingMask: 18usize];
        value
    }
}
fn color(rgb: [f32; 3], alpha: f64) -> Retained<AnyObject> {
    // SAFETY: NSColor factory returns an autoreleased color, retained here.
    unsafe {
        let class = AnyClass::get(c"NSColor").expect("NSColor");
        let value: *mut AnyObject = msg_send![class, colorWithSRGBRed: rgb[0] as f64,
            green: rgb[1] as f64, blue: rgb[2] as f64, alpha: alpha];
        Retained::retain(value).expect("sRGB color")
    }
}
fn layer(view: &AnyObject) -> &AnyObject {
    // SAFETY: each owned view is layer-backed; its layer lives with the view.
    unsafe { let ptr: *mut AnyObject = msg_send![view, layer]; ptr.as_ref().expect("backing layer") }
}
fn background(layer: &AnyObject, color: &AnyObject) {
    // SAFETY: CALayer retains the CGColor returned by this live NSColor.
    unsafe {
        let cg: *const AnyObject = msg_send![color, CGColor];
        let _: () = msg_send![layer, setBackgroundColor: cg];
    }
}

pub(super) struct Backdrop {
    pub view: Retained<AnyObject>,
    frost: Retained<AnyObject>,
    veil: Retained<AnyObject>,
    coating: Retained<AnyObject>,
    gradient: Retained<AnyObject>,
    liquid: Option<Retained<AnyObject>>,
    last: Option<(WindowBackdrop, bool, bool)>,
    size: [f64; 2],
}
impl Backdrop {
    pub fn new(host: &AnyObject) -> Self {
        let view = view(c"NSView");
        let frost = self::view(c"NSVisualEffectView");
        let veil = self::view(c"NSView");
        let coating = self::view(c"NSView");
        // SAFETY: AppKit/Core Animation calls in the owning window's main thread.
        unsafe {
            let _: () = msg_send![&*frost, setMaterial: 13isize]; // NSVisualEffectMaterialHUDWindow
            let _: () = msg_send![&*frost, setBlendingMode: 0isize]; // behindWindow
            let _: () = msg_send![&*frost, setState: 1isize]; // active
            for child in [&*frost, &*veil, &*coating] { let _: () = msg_send![&*view, addSubview: child]; }
            let _: () = msg_send![layer(&view), setMasksToBounds: true];
            let class = AnyClass::get(c"CAGradientLayer").expect("CAGradientLayer");
            let gradient: *mut AnyObject = msg_send![class, new];
            let gradient = Retained::from_raw(gradient).expect("native gradient");
            // The coating is an unflipped NSView, as in Yab.
            let _: () = msg_send![&*gradient, setStartPoint: CGPoint { x: 0., y: 1. }];
            let _: () = msg_send![&*gradient, setEndPoint: CGPoint { x: 1., y: 0. }];
            let _: () = msg_send![layer(&coating), addSublayer: &*gradient];
            let _: () = msg_send![&*view, setAlphaValue: 0.0f64];
            let _: () = msg_send![host, addSubview: &*view];
            Self { view, frost, veil, coating, gradient, liquid: None, last: None, size: [0.; 2] }
        }
    }
    pub fn update(&mut self, style: WindowBackdrop, width: f64, height: f64) {
        // SAFETY: material presentation owns this transaction on the main thread.
        unsafe {
            let workspace: *mut AnyObject = msg_send![AnyClass::get(c"NSWorkspace").expect("NSWorkspace"), sharedWorkspace];
            let reduced: bool = msg_send![workspace, accessibilityDisplayShouldReduceTransparency];
            let contrast: bool = msg_send![workspace, accessibilityDisplayShouldIncreaseContrast];
            if self.size != [width, height] {
                let bounds = rect(0., 0., width, height);
                for child in [&*self.frost, &*self.veil, &*self.coating] { let _: () = msg_send![child, setFrame: bounds]; }
                if let Some(glass) = &self.liquid { let _: () = msg_send![&**glass, setFrame: bounds]; }
                let _: () = msg_send![&*self.gradient, setFrame: bounds];
                self.size = [width, height];
            }
            if self.last == Some((style, reduced, contrast)) { return; }
            let colors = style.colors();
            let sidebar = style.finish == BackdropFinish::Sidebar;
            let alpha = if reduced || ((sidebar || style.is_glass()) && contrast) { 1. } else { style.opacity() };
            let sidebar_glass = sidebar && alpha < 1. && AnyClass::get(c"NSGlassEffectView").is_some();
            let glass = (style.is_glass() || sidebar_glass) && !reduced && !contrast;
            let appearance_name = objc2_foundation_video::NSString::from_str(if style.dark { "NSAppearanceNameDarkAqua" } else { "NSAppearanceNameAqua" });
            let appearance: *mut AnyObject = msg_send![AnyClass::get(c"NSAppearance").expect("NSAppearance"), appearanceNamed: &*appearance_name];
            assert!(!appearance.is_null(), "native backdrop appearance");
            let _: () = msg_send![&*self.frost, setAppearance: appearance];
            if glass && self.liquid.is_none() {
                let liquid = self::view(c"NSGlassEffectView");
                let _: () = msg_send![&*liquid, setFrame: rect(0., 0., width, height)];
                let _: () = msg_send![&*self.view, addSubview: &*liquid, positioned: 1isize, relativeTo: &*self.frost];
                self.liquid = Some(liquid);
            } else if !glass && let Some(liquid) = self.liquid.take() {
                let _: () = msg_send![&*liquid, removeFromSuperview];
            }
            if let Some(liquid) = &self.liquid {
                let _: () = msg_send![&**liquid, setCornerRadius: if sidebar { 0.0f64 } else { 18.0f64 }];
                let _: () = msg_send![&**liquid, setStyle: if style.dark && !sidebar { 1isize } else { 0isize }];
                let _: () = msg_send![&**liquid, setAppearance: appearance];
            }
            let frosted = matches!(style.finish, BackdropFinish::Frost | BackdropFinish::Glass) && !glass && alpha < 1. && !reduced;
            let _: () = msg_send![&*self.frost, setHidden: !frosted];
            let show_veil = frosted || (style.is_glass() && glass && style.dark);
            let _: () = msg_send![&*self.veil, setHidden: !show_veil];
            let veil_alpha = if show_veil && glass { glass_veil(&colors, alpha) } else { 0.6 };
            background(layer(&self.veil), &color(if style.dark { [0.; 3] } else { [1.; 3] }, veil_alpha));
            let array: *mut AnyObject = msg_send![AnyClass::get(c"NSMutableArray").expect("NSMutableArray"), new];
            let array = Retained::from_raw(array).expect("gradient colors");
            for rgb in &colors {
                let value = color(*rgb, 1.);
                let cg: *const AnyObject = msg_send![&*value, CGColor];
                let _: () = msg_send![&*array, addObject: cg];
            }
            let _: () = msg_send![&*self.gradient, setColors: &*array];
            let _: () = msg_send![&*self.gradient, setHidden: colors.len() == 1];
            if colors.len() == 1 { background(layer(&self.coating), &color(colors[0], 1.)); }
            else { let _: () = msg_send![layer(&self.coating), setBackgroundColor: std::ptr::null::<AnyObject>()]; }
            let _: () = msg_send![layer(&self.coating), setOpacity: alpha];
            self.last = Some((style, reduced, contrast));
        }
    }
}

// Same readability rule as Yab WindowGlassPolicy: protect muted white text on
// clear glass even with a white desktop behind it; preserve every gradient end.
fn glass_veil(colors: &[[f32; 3]], alpha: f32) -> f64 {
    let words = crate::color::srgb_decode(0.96);
    let mut veil = 0.58f32;
    while veil < 1. {
        if colors.iter().all(|c| {
            let rgb = c.map(|x| crate::color::srgb_decode(x * alpha + (1. - veil) * (1. - alpha)));
            let l = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
            (words + 0.05) / (l + 0.05) >= 4.5
        }) { break; }
        veil = (veil + 0.02).min(1.);
    }
    veil as f64
}
