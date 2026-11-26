//! The rail action and the update settings reflect the same download.
use super::Status;
use crate::{
    icons::{DOWNLOAD, MORE_HORIZONTAL, ROTATE_CCW, ico},
    model::{SettingsPage, State},
    navigation::{RAIL_GLYPH_SIZE, rail_action},
    ui::{accent, c, label, nav_hover},
};
use rune::{Color, El, Shape, el};

const DIAMETER: f32 = 28.;

fn wave(percent: u8) -> El {
    let waterline = DIAMETER * (1. - f32::from(percent) / 100.);
    let amplitude = 1.8_f32.min(waterline).min(DIAMETER - waterline);
    let mut polygon = Vec::with_capacity(35);
    for step in 0..=32 {
        let x = DIAMETER * step as f32 / 16.;
        let y = waterline + amplitude * (std::f32::consts::TAU * x / DIAMETER).sin();
        polygon.push([x, y]);
    }
    polygon.extend([[2. * DIAMETER, DIAMETER], [0., DIAMETER]]);
    rune::canvas(Shape::new().fill(polygon, Color::BLACK.alpha(0.28)))
        .key("update-wave")
        .absolute(0., 0.)
        .size(2. * DIAMETER, DIAMETER)
        .inert()
        .looping(rune::LoopSpec::slide(1.4, [0., 0.], [-0.5, 0.]).easing(rune::Easing::LINEAR))
}

pub fn update_button(s: &State) -> El {
    let status = &s.updater.status;
    let tip = match status {
        Status::Available(m) => format!("{} {}", s.tr("下载更新"), m.version),
        Status::Downloading(percent) => format!("{} {percent}%", s.tr("正在下载更新")),
        Status::Verifying => s.tr("正在校验更新…").to_owned(),
        Status::Ready(p) => format!(
            "{} {} · {}",
            s.tr("重启更新"),
            p.manifest.version,
            s.tr("重启前会保存打开的笔记")
        ),
        Status::Checking => s.tr("正在检查更新…").to_owned(),
        Status::Preparing | Status::Installing => s.tr("正在准备重启…").to_owned(),
        Status::Failed(_) => s.tr("更新未完成").to_owned(),
        _ => s.tr("软件更新").to_owned(),
    };
    let accent = c(accent(s));
    let plate = if s.theme().is_dark() {
        accent.mix(Color::BLACK, 0.67)
    } else {
        accent
    };
    let mut button = rail_action("editor-updates", &tip);
    let glyph = match status {
        Status::Downloading(_) | Status::Verifying => {
            let (percent, role) = match status {
                Status::Downloading(percent) => (*percent, "progressbar"),
                _ => (100, "status"),
            };
            let mut meter = el()
                .size(DIAMETER, DIAMETER)
                .clip_rounded(DIAMETER / 2.)
                .radius(DIAMETER / 2.)
                .fill(plate)
                .center();
            if matches!(status, Status::Downloading(_)) && percent > 0 {
                meter = meter.child(wave(percent));
            }
            return button.role(role).child(meter.child(label(
                format!("{percent}%"),
                9.5,
                600.,
                0xffffff,
            )));
        }
        Status::Checking | Status::Preparing | Status::Installing => {
            button = button.role("status");
            &MORE_HORIZONTAL
        }
        _ => {
            button = button
                .role("button")
                .cursor(rune::Cursor::Pointer)
                .focusable(true)
                .hover(nav_hover(s))
                .on_press_tx(|s: &mut State, tx| match s.updater.status {
                    Status::Available(_) | Status::Ready(_) => super::act(s, tx),
                    _ => {
                        s.open_settings(tx);
                        s.settings = Some(SettingsPage::Updates);
                    }
                });
            if matches!(status, Status::Ready(_) | Status::Failed(_)) {
                &ROTATE_CCW
            } else {
                &DOWNLOAD
            }
        }
    };
    let mut disc = el()
        .key("update-disc")
        .size(24., 24.)
        .radius(12.)
        .fill(plate)
        .center();
    if matches!(status, Status::Available(_) | Status::Ready(_)) {
        disc = disc.border(1., Color::WHITE.alpha(0.40));
    }
    button.child(
        disc.child(
            ico(glyph, RAIL_GLYPH_SIZE)
                .key("editor-updates-glyph")
                .color(Color::WHITE),
        ),
    )
}

pub fn policy_notice(s: &State, width: f32, height: f32) -> Option<El> {
    s.updater.required()?;
    let policy = s.updater.policy.current.as_ref()?;
    let message = match s.language {
        crate::i18n::Language::Chinese => &policy.message_zh,
        crate::i18n::Language::English => &policy.message_en,
    };
    let ink = crate::ui::theme_ink(s.theme());
    let mut content = rune::col().gap(10.).padding(18.)
        .child(label(s.tr("需要更新 Echo"), 16., 600., ink));
    if !message.is_empty() {
        content = content.child(label(message, 13., 450., ink).wrap());
    }
    if s.updater.required_ready() {
        let panel = crate::overlays::dialog_surface(s, content.w_full().max_w(440.)
            .role("dialog").label(s.tr("需要更新 Echo"))
            .child(crate::overlays::button(s, "required-update-install", s.tr("重启更新"), true, false)
                .on_press_tx(super::act))).modal(false, |_: &mut State| {});
        return Some(el().key("required-update-dialog").absolute(0., 0.).size(width, height)
            .fill(Color::BLACK.alpha(0.35)).center().child(panel));
    }
    let detail = match &s.updater.status {
        Status::Failed(error) | Status::CheckFailed(error) => error.clone(),
        Status::Downloading(percent) => format!("{} {percent}%", s.tr("正在下载更新")),
        Status::Verifying => s.tr("正在校验更新…").to_string(),
        Status::Preparing | Status::Installing => s.tr("正在准备重启…").to_string(),
        _ => s.tr("正在检查更新…").to_string(),
    };
    content = content.child(label(detail, 12., 450., ink).wrap());
    if matches!(s.updater.status, Status::Failed(_) | Status::CheckFailed(_) | Status::Current | Status::Available(_))
        && s.updater.worker.is_none() {
        content = content.child(crate::overlays::button(s, "required-update-retry", s.tr("重新检查"), false, false)
            .on_press_tx(|s: &mut State, tx| {
                s.updater.policy.retry = std::time::Instant::now();
                tx.instant();
            }));
    }
    let width = (width - 48.).min(460.);
    Some(crate::overlays::surface(s, content.key("required-update-banner").role("alert")
        .pin(None, None, Some(24.), Some(24.)).w(width), 12.))
}
