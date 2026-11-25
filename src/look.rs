//! Echo material studio. Color math and the dot field derive from Yab under Apache-2.0.
//! See THIRD_PARTY.md and licenses/yab-Apache-2.0.txt. Layout and events use Rune.
use crate::{
    icons::{CHECK, CHEVRON_RIGHT, MINUS, PLUS, ico},
    material::{LookMode, Tint},
    model::State,
    ui::{c, label, theme_ink, theme_muted, editor_colors},
};
use rune::{Cursor, El, Key, Phase, col, el, row};
pub struct Editor {
    pub open: bool,
    pub anchor: [f32; 4],
    pub page: usize,
    pub chosen: usize,
    pub opened_transparency: u8,
    grab: Option<Grab>,
    pub editing_color: Option<usize>,
    pub color_input: rune::TextEdit,
    pub color_error: bool,
    pub customizing: bool,
}
impl Default for Editor {
    fn default() -> Self { Self { open:false, anchor:[0.;4], page:0, chosen:0,
        opened_transparency:0, grab:None, editing_color:None,
        color_input:rune::TextEdit::new(""), color_error:false, customizing:false } }
}
struct Grab {
    colors: Vec<Tint>,
    index: usize,
    offset: [f32; 2],
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Transparency,
    Grain,
}
const FIELD: f32 = 260.;
const DOT: f32 = 36.;
const PRESETS: [[u32; 9]; 5] = [
    [
        0xf2e9e3, 0xec8fbb, 0xa0689a, 0xe54c6e, 0xf37c5a, 0xf3d85e, 0x44fe96, 0x74bfd6, 0x666587,
    ],
    [
        0xf2e9e3, 0xec8fbb, 0xa0689a, 0xf37c5a, 0xf3ed6e, 0xb7ff79, 0x72f2e6, 0x938edc, 0x666587,
    ],
    [
        0xfefbfa, 0xffe9f3, 0xf1d1ec, 0xffcbd1, 0xffe0d3, 0xfff9e5, 0xdefdeb, 0xdcf3fc, 0xc5c6e5,
    ],
    [
        0x4e3a5a, 0x693558, 0x8f4042, 0xb16d40, 0xd7b35d, 0xcdcca4, 0x7aa982, 0x226149, 0x26456b,
    ],
    [
        0xffffff, 0xe5e5e5, 0xcccccc, 0xb2b2b2, 0x808080, 0x666666, 0x333333, 0x1a1a1a, 0x000000,
    ],
];
const LINKS: [[u32; 2]; 9] = [
    [0xf1e2e7, 0xeff2e5],
    [0xe68ce7, 0xe9aa9c],
    [0x9f6a80, 0x886e9d],
    [0xf23b97, 0xedf349],
    [0xf6bb6d, 0xfaa68e],
    [0xddeb78, 0xe3bd78],
    [0x69e7b9, 0x8c95e0],
    [0x8dabda, 0x8dbcd0],
    [0x7d5f88, 0x628087],
];
fn quiet(s: &State, key: &str, name: &str, icon: &'static rune::Icon) -> El {
    el().key(key)
        .size(26., 26.)
        .radius(6.)
        .center()
        .role("button")
        .label(name)
        .help(name)
        .focusable(true)
        .cursor(Cursor::Pointer)
        .hover(c(theme_ink(s.theme())).alpha(0.07))
        .child(ico(icon, 14.).color(c(theme_muted(s.theme()))))
}
fn field(s: &State, width: f32, height: f32) -> El {
    let travel_w = width - DOT;
    let travel_h = height - DOT;
    let primary = s.sidebar_material.colors[s.look.chosen.min(s.sidebar_material.colors.len()-1)];
    let p = primary.point();
    let lens = [18. + p[0] * travel_w, 18. + p[1] * travel_h];
    let mut field = el()
        .key("look-color-field")
        .size(width, height)
        .radius(12.)
        .clip_rounded(12.)
        .fill(c(s.theme().choose(0xf5f4f4, 0x25272b)));
    // Every dot carries the color at that position; the ground itself stays neutral.
    let columns = (width / 12.).floor() as usize;
    let rows = (height / 12.).floor() as usize;
    for iy in 0..rows {
        for ix in 0..columns {
            let x = 9. + ix as f32 * (width-18.) / (columns-1) as f32;
            let y = 9. + iy as f32 * (height-18.) / (rows-1) as f32;
            let xy = [
                ((x - 18.) / travel_w).clamp(0., 1.),
                ((y - 18.) / travel_h).clamp(0., 1.),
            ];
            let mut color = Tint {
                lightness: primary.lightness.clamp(55, 75),
                ..primary
            }
            .at(xy[0], xy[1]);
            // At black/white, dots show the available (zero) chroma as in Yab.
            if primary.lightness == 0 || primary.lightness == 100 {
                color.saturation = 0;
            }
            let radius = 1.6
                + 2.2 * (-((x - lens[0]).powi(2) + (y - lens[1]).powi(2)) / (2. * 24. * 24.)).exp();
            field = field.child(
                el().absolute(x - radius, y - radius)
                    .size(radius * 2., radius * 2.)
                    .radius(radius)
                    .fill(color.color())
                    .inert(),
            );
        }
    }
    let mut area = el()
        .key("look-field-drag")
        .absolute(0., 0.)
        .size(width, height)
        .cursor(Cursor::Pointer)
        .on_drag(move |s: &mut State, d, tx| {
            if d.phase == Phase::Down {
                let mut index = s.look.chosen.min(s.sidebar_material.colors.len()-1);
                let mut offset = [0., 0.];
                let mut nearest = 22_f32;
                for (i, tint) in s.sidebar_material.colors.iter().enumerate() {
                    let p = tint.point();
                    let center = [18. + p[0] * travel_w, 18. + p[1] * travel_h];
                    let distance = (d.x - center[0]).hypot(d.y - center[1]);
                    if distance < nearest {
                        nearest = distance;
                        index = i;
                        offset = [d.x - center[0], d.y - center[1]];
                    }
                }
                s.look.chosen = index;
                s.look.customizing = true;
                s.look.editing_color = None; s.look.color_error = false;
                s.look.grab = Some(Grab {
                    colors: s.sidebar_material.colors.clone(),
                    index,
                    offset,
                });
                tx.focus(format!("look-color-{index}"), false);
            }
            if let Some(grab) = &s.look.grab {
                if d.phase == Phase::Up && d.cancelled() {
                    s.sidebar_material.colors = grab.colors.clone();
                } else {
                    let old = grab.colors[grab.index];
                    let next = old.at(
                        (d.x - grab.offset[0] - 18.) / travel_w,
                        (d.y - grab.offset[1] - 18.) / travel_h,
                    );
                    s.sidebar_material.colors = grab.colors.clone();
                    s.sidebar_material.colors[grab.index] = next;
                    if grab.index == 0 {
                        for i in 1..grab.colors.len() {
                            s.sidebar_material.colors[i].hue =
                                (grab.colors[i].hue as i32 + next.hue as i32 - old.hue as i32)
                                    .rem_euclid(360) as u16;
                            s.sidebar_material.colors[i].saturation =
                                (grab.colors[i].saturation as i32 + next.saturation as i32
                                    - old.saturation as i32)
                                    .clamp(0, 100) as u8;
                        }
                    }
                }
            }
            if d.phase == Phase::Up {
                s.look.grab = None;
                if !d.cancelled() {
                    s.save_sidebar_material();
                }
            }
            tx.instant();
        });
    for i in (0..s.sidebar_material.colors.len()).rev() {
        let color = s.sidebar_material.colors[i];
        let p = color.point();
        let size = if i == s.look.chosen { 36. } else { 24. };
        area = area.child(
            el().key(format!("look-color-{i}"))
                .absolute(
                    18. + p[0] * travel_w - size / 2.,
                    18. + p[1] * travel_h - size / 2.,
                )
                .size(size, size)
                .radius(size / 2.)
                .fill(color.color())
                .border(3., c(s.theme().choose(0xffffff, 0xe4e6ec)))
                .shadow(1., 2., c(0x000000).alpha(0.25))
                .role("slider")
                .label(if i == 0 {
                    s.tr("主色")
                } else {
                    s.tr("关联色")
                })
                .help(s.tr("拖动色点，方向键微调"))
                .range_value(color.hue as f64, 0., 360.)
                .range_step(3.)
                .focusable(true)
                .on_drag(move |s: &mut State, d, tx| {
                    if d.phase == Phase::Down {
                        s.look.chosen = i;
                        s.look.customizing = true;
                        s.look.editing_color = None; s.look.color_error = false;
                        s.look.grab = Some(Grab {
                            colors: s.sidebar_material.colors.clone(),
                            index: i,
                            offset: [0., 0.],
                        });
                        tx.focus(format!("look-color-{i}"), false);
                    }
                    if let Some(grab) = &s.look.grab {
                        let old = grab.colors[i];
                        let point = old.point();
                        if d.phase == Phase::Up && d.cancelled() {
                            s.sidebar_material.colors = grab.colors.clone();
                        } else {
                            let next =
                                old.at(point[0] + d.dx / travel_w, point[1] + d.dy / travel_h);
                            s.sidebar_material.colors = grab.colors.clone();
                            s.sidebar_material.colors[i] = next;
                            if i == 0 {
                                for j in 1..grab.colors.len() {
                                    s.sidebar_material.colors[j].hue = (grab.colors[j].hue as i32
                                        + next.hue as i32
                                        - old.hue as i32)
                                        .rem_euclid(360)
                                        as u16;
                                    s.sidebar_material.colors[j].saturation =
                                        (grab.colors[j].saturation as i32 + next.saturation as i32
                                            - old.saturation as i32)
                                            .clamp(0, 100)
                                            as u8;
                                }
                            }
                        }
                    }
                    if d.phase == Phase::Up {
                        s.look.grab = None;
                        if !d.cancelled() {
                            s.save_sidebar_material();
                        }
                    }
                    tx.instant();
                })
                .on_step(move |s: &mut State, delta, tx| {
                    shift_color(s, i, delta, 0);
                    tx.instant();
                })
                .on_key(move |s: &mut State, e, tx| {
                    let (h, c) = match e.key {
                        Key::Left => (-3, 0),
                        Key::Right => (3, 0),
                        Key::Up => (0, 2),
                        Key::Down => (0, -2),
                        _ => return false,
                    };
                    shift_color(s, i, h, c);
                    tx.instant();
                    true
                }),
        );
    }
    field = field.child(area);
    field
}
fn color_actions(s: &State) -> El {
    row().gap(4.)
            .child(
                quiet(s, "look-remove", s.tr("移除颜色"), &MINUS)
                    .disabled(s.sidebar_material.colors.len() == 1)
                    .on_press_tx(|s: &mut State, tx| {
                        if s.sidebar_material.colors.len() > 1 {
                            let i = if s.look.chosen > 0 {
                                s.look.chosen
                            } else {
                                s.sidebar_material.colors.len() - 1
                            };
                            s.sidebar_material.colors.remove(i);
                            s.look.chosen = 0;
                            s.look.editing_color = None; s.look.color_error = false;
                            s.look.customizing = true;
                            s.save_sidebar_material();
                        }
                        tx.instant();
                    }),
            )
            .child(
                quiet(s, "look-add", s.tr("添加颜色"), &PLUS)
                    .disabled(s.sidebar_material.colors.len() == 3)
                    .on_press_tx(|s: &mut State, tx| {
                        if s.sidebar_material.colors.len() < 3 {
                            let main = s.sidebar_material.colors[0];
                            let count = s.sidebar_material.colors.len() + 1;
                            s.sidebar_material.colors = (0..count)
                                .map(|i| Tint {
                                    hue: (main.hue + 360 * i as u16 / count as u16) % 360,
                                    ..main
                                })
                                .collect();
                            s.look.editing_color = None; s.look.color_error = false;
                            s.look.customizing = true;
                            s.save_sidebar_material();
                        }
                        tx.instant();
                    }),
            )
}
fn shift_color(s: &mut State, index: usize, h: i32, c: i32) {
    s.look.chosen = index;
    s.look.customizing = true;
    s.look.editing_color = None; s.look.color_error = false;
    let old = s.sidebar_material.colors[index];
    let next = Tint {
        hue: (old.hue as i32 + h).rem_euclid(360) as u16,
        saturation: (old.saturation as i32 + c).clamp(0, 100) as u8,
        ..old
    };
    s.sidebar_material.colors[index] = next;
    if index == 0 {
        for color in s.sidebar_material.colors.iter_mut().skip(1) {
            color.hue = (color.hue as i32 + h).rem_euclid(360) as u16;
            color.saturation = (color.saturation as i32 + next.saturation as i32
                - old.saturation as i32)
                .clamp(0, 100) as u8;
        }
    }
    s.save_sidebar_material();
}
fn presets(s: &State) -> El {
    let mut dots = row().w(212.).gap(4.).center_y();
    for i in 0..9 {
        let color = PRESETS[s.look.page][i];
        let page = s.look.page;
        let reference = page == 2 && i == 0;
        let colors = if reference {
            crate::material::reference_colors()
        } else if page == 1 {
            vec![Tint::from_hex(color), Tint::from_hex(LINKS[i][0]), Tint::from_hex(LINKS[i][1])]
        } else { vec![Tint::from_hex(color)] };
        let preview = crate::material::Settings { colors: colors.clone(), ..s.sidebar_material.clone() };
        let stops = preview.surface_stops(s.system_dark.get());
        dots = dots.child(
            el().key(format!("look-preset-{i}"))
                .size(20., 20.)
                .radius(10.)
                .border(1.4, c(theme_ink(s.theme())).alpha(0.18))
                .fill(crate::ui::sidebar_tint(s.theme()))
                .linear_gradient(std::f32::consts::FRAC_PI_2, stops)
                .cursor(Cursor::Pointer)
                .focusable(true)
                .role("button")
                .label(format!(
                    "{} {}",
                    s.tr(["颜色", "渐变", "柔和", "低饱和", "灰阶"][page]),
                    i + 1
                ))
                .on_press_tx(move |s: &mut State, tx| {
                    s.sidebar_material.colors = colors.clone();
                    s.look.chosen = 0;
                    s.look.editing_color = None; s.look.color_error = false;
                    s.look.customizing = true;
                    s.save_sidebar_material();
                    tx.instant();
                }),
        );
    }
    row()
        .w(FIELD)
        .h(32.)
        .center_y()
        .child(
            quiet(s, "look-prev", s.tr("上一组颜色"), &CHEVRON_RIGHT)
                .w(24.)
                .rotate(std::f32::consts::PI)
                .disabled(s.look.page == 0)
                .on_press_tx(|s: &mut State, tx| {
                    s.look.page = s.look.page.saturating_sub(1);
                    tx.instant();
                }),
        )
        .child(dots)
        .child(
            quiet(s, "look-next", s.tr("下一组颜色"), &CHEVRON_RIGHT)
                .w(24.)
                .disabled(s.look.page == 4)
                .on_press_tx(|s: &mut State, tx| {
                    s.look.page = (s.look.page + 1).min(4);
                    tx.instant();
                }),
        )
}
impl Control {
    fn value(self, s: &State) -> u8 {
        match self {
            Self::Transparency => s.sidebar_material.transparency,
            Self::Grain => s.sidebar_material.grain,
        }
    }
    fn set(self, s: &mut State, value: u8) {
        match self {
            Self::Transparency => s.sidebar_material.transparency = value,
            Self::Grain => s.sidebar_material.grain = value,
        }
    }
    fn name(self, s: &State) -> &'static str {
        s.tr(match self {
            Self::Transparency => "透明度",
            Self::Grain => "颗粒强度",
        })
    }
}

// The material preview is drawn with Rune primitives, not a texture asset.
fn slider_ribbon(s: &State, control: Control, width: f32) -> El {
    let mut ribbon = el().key("material-ribbon").absolute(0.,9.).size(width,26.)
        .radius(10.).clip_rounded(10.)
        .linear_gradient(std::f32::consts::FRAC_PI_2, vec![
            (0., c(crate::ui::accent_soft(s))),
            (1., c(editor_colors(s.theme()).inset)),
        ]).inert();
    match control {
        Control::Transparency => {
            for i in 0..6 {
                ribbon = ribbon.child(el().absolute(10.+i as f32*width*0.06,6.-i as f32*0.6)
                    .size(width*0.19,24.).radius(7.)
                    .fill(c(crate::ui::accent(s)).alpha(0.07-i as f32*0.008)).inert());
            }
        }
        Control::Grain => {
            let mut dots = rune::Shape::new();
            let mut seed = 0x4543484fu32;
            let mut random = || { seed ^= seed << 13; seed ^= seed >> 17; seed ^= seed << 5; seed as f32/u32::MAX as f32 };
            for _ in 0..(width*3.5) as usize {
                let x = random()*width;
                let y = random()*26.;
                let density = x/width;
                let radius = 0.35 + random()*0.35;
                if random() < density {
                    dots = dots.line([x,y],[x+0.02,y],radius,rune::LineCap::Round,
                        c(crate::ui::accent(s)).alpha(0.12+density*0.30));
                }
            }
            ribbon = ribbon.child(rune::canvas(dots).size(width,26.).inert());
        }
    }
    ribbon
}

fn slider(s: &State, control: Control, width: f32) -> El {
    let value = control.value(s);
    let key = match control { Control::Transparency => "sidebar-transparency", Control::Grain => "sidebar-grain" };
    let travel = (width - 16.).max(1.);
    let cut = value as f32 / 100. * travel;
    el().key(key).size(width, 44.).radius(10.).role("slider")
        .label(format!("{} {}%",control.name(s),value)).range_value(value as f64,0.,100.).range_step(1.)
        .focusable(true).focus_ring(2.,2.,c(crate::ui::accent(s)))
        .help(s.tr("拖动或点击调节，方向键微调")).cursor(Cursor::Pointer)
        .on_drag(move |s: &mut State,d,tx| {
            if d.phase == Phase::Down {
                d.data = control.value(s) as f32;
                tx.focus(key,false);
            }
            if d.phase == Phase::Up && d.cancelled() {
                control.set(s,d.data as u8);
            } else {
                let original_center = 8. + d.data/100.*travel;
                let offset = if (d.start_x-original_center).abs() <= 10. {d.start_x-original_center} else {0.};
                control.set(s,(((d.x-offset-8.)/travel).clamp(0.,1.)*100.).round() as u8);
            }
            if d.phase == Phase::Up && !d.cancelled() { s.save_sidebar_material(); }
            tx.instant();
        })
        .on_step(move |s: &mut State,delta,tx| {
            control.set(s,(control.value(s) as i32+delta).clamp(0,100) as u8);
            s.save_sidebar_material(); tx.instant();
        })
        .on_step_limit(move |s: &mut State,max,tx| {
            control.set(s,if max {100} else {0});
            s.save_sidebar_material(); tx.instant();
        })
        .on_key(move |s: &mut State,e,tx| {
            // Keep Echo's Shift shortcut; ordinary arrows/Page keys are Rune's on_step.
            if !e.mods.shift {return false;}
            let v=match e.key {
                Key::Left|Key::Down=>control.value(s).saturating_sub(10),
                Key::Right|Key::Up=>(control.value(s)+10).min(100),_=>return false,
            };
            control.set(s,v); s.save_sidebar_material(); tx.instant(); true
        })
        .on_double_press_tx(move |s: &mut State,tx| {
            control.set(s,if control==Control::Transparency {s.look.opened_transparency} else {0});
            s.save_sidebar_material();tx.instant();
        })
        .child(slider_ribbon(s,control,width))
        .child(el().key("material-thumb").absolute(cut,6.).size(16.,32.).radius(8.)
            .fill(c(s.theme().choose(0xffffff,0xe6e6e4)))
            .border(0.6,c(s.theme().choose(0xdcdcdc,0xf5f5f3)))
            .shadow(2.,4.,c(0x000000).alpha(if s.theme().is_dark(){0.30}else{0.15}))
            .child(el().absolute(7.25,10.).size(1.5,12.).radius(0.75)
                .fill(c(crate::ui::accent(s))).inert()).inert())
}
fn mode_picker(s: &State) -> El {
    let mut modes=row().h(30.).padding(2.).radius(8.).gap(1.)
        .fill(c(editor_colors(s.theme()).inset)).border(0.75,c(editor_colors(s.theme()).border));
    for (mode,title,key) in [(LookMode::Light,"明亮","look-mode-light"),(LookMode::Dark,"暗色","look-mode-dark"),(LookMode::Auto,"跟随系统","look-mode-auto")] {
        let selected=s.sidebar_material.mode==mode;
        modes=modes.child(el().key(key).h(26.).px(if s.language.is_english(){8.}else{12.}).radius(6.).center()
            .role("radio").checked(selected).label(s.tr(title)).focusable(true).cursor(Cursor::Pointer)
            .focus_ring(2.,2.,c(crate::ui::accent(s)))
            .fill(if selected {c(crate::ui::accent_soft(s))}else{rune::Color::TRANSPARENT})
            .hover(c(theme_ink(s.theme())).alpha(0.06))
            .child(label(s.tr(title),12.,if selected{550.}else{450.},theme_ink(s.theme())))
            .on_press_tx(move |s:&mut State,tx|{s.look.editing_color=None;s.look.color_error=false;s.set_look_mode(mode);tx.instant();}));
    }
    modes.role("radiogroup").label(s.tr("外观"))
}
const MATERIALS: [(&str, &str, &[u32]); 7] = [
    ("porcelain", "瓷白", &crate::material::PORCELAIN_COLORS),
    ("warm-white", "暖白", &crate::material::WARM_WHITE_COLORS),
    ("paper", "纸白", &crate::material::PAPER_COLORS),
    ("dawn", "晨雾", &crate::material::DAWN_COLORS),
    ("mist", "雾蓝", &[0xdce8f3, 0xe9eef4]),
    ("sand", "杏白", &crate::material::SAND_COLORS),
    ("sage", "苔绿", &[0xe1eae1, 0xecf0e7]),
];
fn material_colors(index:usize)->Vec<Tint>{MATERIALS[index].2.iter().copied().map(Tint::from_hex).collect()}
fn selected_material(s:&State)->usize {
    if s.look.customizing {return MATERIALS.len();}
    (0..MATERIALS.len()).find(|&i|s.sidebar_material.colors==material_colors(i)).unwrap_or(MATERIALS.len())
}
fn sample(s:&State,index:usize,width:f32,selected:bool)->El {
    let (key,title)=if index==MATERIALS.len() {("custom","自定义")} else {(MATERIALS[index].0,MATERIALS[index].1)};
    let colors=if index==MATERIALS.len() {s.sidebar_material.colors.clone()}else{material_colors(index)};
    let settings=crate::material::Settings{colors,..s.sidebar_material.clone()};
    let height=(width*0.64).clamp(64.,104.);
    let mut miniature=el().key(format!("material-preview-{key}")).size(width,height).radius(12.).clip_rounded(12.)
        .fill(c(editor_colors(s.theme()).inset))
        .child(el().size(width,height).linear_gradient(crate::material::surface_angle(width, height),settings.surface_stops(s.system_dark.get())).inert())
        .child(crate::ui::material_grain(s,&format!("material-grain-{key}"),width,height).inert());
    if selected {
        miniature=miniature.child(el().absolute(width-24.,8.).size(16.,16.).radius(8.).center()
            .fill(c(crate::ui::accent(s)))
            .child(ico(&CHECK,11.).color(c(s.theme().choose(0xffffff,0x202226)))).inert());
    }
    miniature=miniature.child(el().absolute(0.,0.).size(width,height).radius(12.)
        .border(if selected{1.4}else{0.75}, if selected{c(crate::ui::accent(s))}else{c(theme_ink(s.theme())).alpha(0.10)}).inert());
    col().key(format!("material-{key}")).w(width).gap(8.).items_center().role("radio").checked(selected)
        .label(s.tr(title)).focusable(true).cursor(Cursor::Pointer)
        .focus_ring(2.,2.,c(crate::ui::accent(s)))
        .child(miniature)
        .child(label(s.tr(title),13.,if selected{550.}else{450.},theme_ink(s.theme())))
        .on_press_tx(move |s:&mut State,tx|{
            s.look.editing_color=None;s.look.color_error=false;s.look.chosen=0;
            if index<MATERIALS.len() {s.sidebar_material.colors=material_colors(index);s.look.customizing=false;s.save_sidebar_material();}
            else {s.look.customizing=true;tx.focus("look-color-0",false);}
            tx.instant();
        })
}
fn gallery(s:&State,width:f32)->El {
    let count = MATERIALS.len() + 1;
    let selected = selected_material(s);
    let columns=if width<330.{2}else if width<500.{3}else{4};
    let tile=(width-12.*(columns-1) as f32)/columns as f32;
    let mut gallery=col().key("material-gallery").w(width).gap(16.).role("radiogroup").label(s.tr("窗口质感"));
    for start in (0..count).step_by(columns) {
        let mut line=row().gap(12.);
        for i in start..(start+columns).min(count){line=line.child(sample(s,i,tile,i==selected));}
        gallery=gallery.child(line);
    }
    gallery
}
fn finish_hex(s:&mut State,tx:&mut rune::Tx) {
    let Some(index)=s.look.editing_color else{return;};
    let raw=s.look.color_input.text.trim().trim_start_matches('#');
    if raw.len()!=6 || !raw.bytes().all(|b|b.is_ascii_hexdigit()) {
        s.look.color_error=true;tx.instant();return;
    }
    let rgb=u32::from_str_radix(raw,16).expect("validated six hex digits");
    s.sidebar_material.colors[index]=Tint::from_hex(rgb);
    s.look.customizing=true;s.look.editing_color=None;s.look.color_error=false;
    s.save_sidebar_material();tx.focus(format!("look-color-{index}"),false);tx.instant();
}
fn chips(s:&State,width:f32)->El {
    let narrow=width<330.;
    let count=s.sidebar_material.colors.len();
    let chip_w=if narrow{width-72.}else{((width-72.-8.*(count-1) as f32)/count as f32).min(132.)};
    let mut colors=if narrow {col().gap(6.)} else {row().gap(8.)};
    for (i,tint) in s.sidebar_material.colors.iter().enumerate(){
        let editing=s.look.editing_color==Some(i);
        let hex=tint.color().hex()[..7].to_uppercase();
        let invalid=s.look.color_error&&editing;
        let edge=c(if invalid{s.theme().choose(0xb7443b,0xffa49b)}else{crate::ui::accent(s)});
        let mut chip=row().key(format!("look-chip-{i}")).size(chip_w,28.).px(7.).gap(7.).center_y().radius(6.)
            .fill(c(editor_colors(s.theme()).raised))
            .border(0.75,if invalid{edge}else if s.look.chosen==i{edge.alpha(0.55)}else{c(editor_colors(s.theme()).border)})
            .focus_within_ring(1.,-1.,edge);
        if chip_w>=100.{chip=chip.child(el().size(16.,16.).radius(4.).fill(tint.color()).border(0.6,c(theme_ink(s.theme())).alpha(0.1)).inert());}
        if editing {
            chip=chip.child(rune::text_input(&s.look.color_input,crate::ui::font(11.5,450.).lh(18.))
                .key(format!("look-hex-{i}")).label(s.tr("十六进制颜色")).single_line().grow(1.).shrink().min_w(0.).h(18.)
                .color(c(theme_ink(s.theme()))).no_focus_ring().invalid(invalid)
                .caret(c(crate::ui::accent(s)),c(crate::ui::accent_soft(s)))
                .on_edit(|s:&mut State|&mut s.look.color_input)
                .on_input_change(|s:&mut State,tx|{s.look.color_error=false;tx.instant();})
                .on_input_key(|s:&mut State,e,tx|{match e.key {
                    Key::Enter=>{finish_hex(s,tx);true},
                    Key::Escape=>{s.look.editing_color=None;s.look.color_error=false;tx.focus(format!("look-color-{}",s.look.chosen),false);tx.instant();true},
                    _=>false,
                }}));
        } else {
            chip=chip.role("button").label(format!("{} {}",s.tr("颜色"),hex)).help(s.tr("编辑颜色值"))
                .focusable(true).cursor(Cursor::Pointer).hover(c(editor_colors(s.theme()).inset))
                .child(label(hex,11.5,450.,theme_ink(s.theme())))
                .on_press_tx(move |s:&mut State,tx| {
                    s.look.chosen=i;s.look.editing_color=Some(i);s.look.color_error=false;
                    s.look.color_input=rune::TextEdit::new(s.sidebar_material.colors[i].color().hex()[..7].to_uppercase());
                    tx.focus(format!("look-hex-{i}"),false);tx.instant();
                });
        }
        colors=colors.child(chip);
    }
    row().w(width).gap(8.).child(colors).child(el().grow(1.)).child(color_actions(s))
}
fn controls(s:&State,width:f32)->El {
    let mut body=col().w(width).gap(18.);
    for control in [Control::Transparency,Control::Grain] {
        let title=if control==Control::Grain{s.tr("颗粒")}else{control.name(s)};
        let line=col().w_full().gap(4.)
            .child(row().w_full().center_y().child(label(title,13.,450.,theme_ink(s.theme())).grow(1.))
                .child(label(format!("{}%",control.value(s)),12.5,450.,theme_ink(s.theme())).w(36.).align(rune::Align::End)))
            .child(slider(s,control,width));
        body=body.child(line);
    }
    body
}
fn text_button(s:&State,key:&str,title:&str)->El {
    el().key(key).h(28.).px(6.).radius(5.).center().focusable(true).role("button").label(title)
        .cursor(Cursor::Pointer).hover(c(theme_ink(s.theme())).alpha(0.06))
        .focus_ring(2.,2.,c(crate::ui::accent(s)))
        .child(label(title,12.,450.,theme_muted(s.theme())))
}
pub fn studio(s:&State,width:f32)->El {
    let compact=width<650.;
    let field_w=if compact{width}else{width*0.57};
    let control_w=width-field_w-36.;
    let header=if width<440.{col().w_full().gap(12.)}else{row().w_full().center_y()};
    let header=header.child(label(s.tr("窗口质感"),16.,550.,theme_ink(s.theme())).grow(1.)).child(mode_picker(s));
    let mut color=col().w(field_w).gap(10.).child(field(s,field_w,if compact{150.}else{142.})).child(chips(s,field_w));
    if s.look.color_error {
        color=color.child(label(s.tr("请输入六位十六进制颜色"),12.,450.,s.theme().choose(0xb7443b,0xffa49b))
            .key("look-color-error").live_region(rune::LiveRegion::Assertive).wrap());
    }
    let editor=if compact {col().w_full().gap(20.).child(color).child(controls(s,width))}
        else {row().w_full().gap(36.).center_y().child(color).child(controls(s,control_w))};
    let studio=col().key("material-studio").w(width).gap(24.).child(header).child(gallery(s,width)).child(editor);
    studio.child(row().w_full().center_y()
            .child(text_button(s,"look-open",s.tr("更多配色"))
                .on_layout_tx(|s:&mut State,r,tx|{s.look.anchor=r;tx.live_only();})
                .on_press_tx(|s:&mut State,tx|{s.look.open=true;tx.focus("look-preset-0",false);tx.animated();}))
            .child(el().grow(1.))
            .child(text_button(s,"look-reset",s.tr("重置"))
                .on_press_tx(|s:&mut State,tx|{
                    let mode=s.sidebar_material.mode;
                    s.sidebar_material=crate::material::Settings{mode,..Default::default()};
                    s.look.customizing=false;s.look.editing_color=None;s.look.color_error=false;s.look.chosen=0;s.look.color_error=false;
                    s.save_sidebar_material();tx.instant();
                })))
}
pub fn close(s:&mut State,tx:&mut rune::Tx){s.look.open=false;tx.focus("look-open",false);tx.animated();}
pub fn popover(s:&State,_cx:&rune::Cx<State>,w:f32,h:f32)->El {
    let size=[300.,110.];let anchor=s.look.anchor;
    let x=anchor[0].clamp(8.,(w-size[0]-8.).max(8.));
    let y=(anchor[1]+anchor[3]+6.).min(h-size[1]-12.).max(crate::ui::CHROME_HEIGHT+8.);
    el().key("look-layer").transition(rune::Transition::identity()).absolute(0.,0.).size(w,h)
        .child(el().key("look-dismiss").absolute(0.,0.).size(w,h).on_press_tx(close))
        .child(crate::overlays::dialog_surface(s,col().key("look-popover").role("dialog").label(s.tr("更多配色"))
            .absolute(x,y).size(size[0],size[1]).padding(16.).gap(10.)).modal_tx(true,close)
            .child(row().w_full().center_y().child(label(s.tr(["颜色","渐变","柔和","低饱和","灰阶"][s.look.page]),15.,600.,theme_ink(s.theme())).grow(1.))
                .child(label(format!("{} / 5",s.look.page+1),11.,450.,theme_muted(s.theme())))
                .child(crate::overlays::close_button(s,"look-close",close)))
            .child(presets(s)))
}
