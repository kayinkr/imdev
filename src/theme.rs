// Baseline CSS logical pixels plus one typographic point (4/3 units).
#[derive(Clone, Copy, Debug)]
pub enum Role { Body, Button, Small, Mono, Url, Sidebar, Brand, Headline, Caption, Section }
use eframe::egui::{Color32, Stroke};
pub const BG:Color32=Color32::from_rgb(20,23,28);
pub const SIDEBAR:Color32=Color32::from_rgb(24,28,34);
pub const SURFACE:Color32=Color32::from_rgb(29,34,41);
pub const FIELD:Color32=Color32::from_rgb(22,26,32);
pub const BORDER:Color32=Color32::from_rgb(57,65,77);
pub const TEXT:Color32=Color32::from_rgb(235,239,244);
pub const MUTED:Color32=Color32::from_rgb(166,177,192);
pub const ACCENT:Color32=Color32::from_rgb(244,153,79);
pub const ACCENT_INK:Color32=Color32::from_rgb(29,25,21);
pub const SUCCESS:Color32=Color32::from_rgb(143,209,177);
pub const ERROR:Color32=Color32::from_rgb(245,158,153);
/// Allocate the shared row height before laying out labels and controls.
pub fn control_row<R>(ui:&mut eframe::egui::Ui,add:impl FnOnce(&mut eframe::egui::Ui)->R)->eframe::egui::InnerResponse<R> {
    ui.allocate_ui_with_layout(eframe::egui::vec2(ui.available_width(),30.0),eframe::egui::Layout::left_to_right(eframe::egui::Align::Center),add)
}

/// Quiet inline action: text-sized visual with a separate 30-point hit target.
pub fn row_action(ui:&mut eframe::egui::Ui,label:&str,role:Role,enabled:bool)->eframe::egui::Response {
    use eframe::egui::{Button,RichText,vec2,Rect};
    let font_id=font(role);
    let response=ui.add_enabled(enabled,Button::new(RichText::new(label).font(font_id.clone())).frame(false).min_size(vec2(0.0,30.0)));
    let line_height=ui.fonts_mut(|fonts|fonts.row_height(&font_id));
    let visible=Rect::from_center_size(response.rect.center(),vec2(response.rect.width(),line_height));
    if response.hovered() || response.has_focus() {
        ui.painter().line_segment([visible.left_bottom(),visible.right_bottom()],Stroke::new(1.0,ACCENT));
    }
    #[cfg(test)]
    ui.ctx().data_mut(|d|d.insert_temp(eframe::egui::Id::new(("compact_row_geometry",label)),(visible,response.rect)));
    let _=visible;
    response
}

/// Keep standard DragValue editing/drag behavior, but decouple its 22pt frame from its 30pt hit area.
pub fn timeout_control(ui:&mut eframe::egui::Ui,value:&mut u64,enabled:bool)->eframe::egui::Response {
    use eframe::egui::{Shape,TextStyle,Rect,vec2,StrokeKind};
    let background=ui.painter().add(Shape::Noop);
    let response=ui.scope(|ui| {
        let style=ui.style_mut();
        let text_style=TextStyle::Name("compact_row_caption".into());
        style.text_styles.insert(text_style.clone(),font(Role::Caption));
        style.drag_value_text_style=text_style;
        style.spacing.button_padding=vec2(8.0,2.0);
        style.spacing.interact_size.y=30.0;
        style.visuals.button_frame=false;
        style.visuals.text_edit_bg_color=Some(Color32::TRANSPARENT);
        style.visuals.selection.stroke=Stroke::NONE;
        for widget in [&mut style.visuals.widgets.inactive,&mut style.visuals.widgets.hovered,&mut style.visuals.widgets.active] {widget.bg_stroke=Stroke::NONE;}
        ui.add_enabled(enabled,eframe::egui::DragValue::new(value).range(100..=120000).speed(100))
    }).inner;
    let visible=Rect::from_center_size(response.rect.center(),vec2(response.rect.width(),22.0));
    let border=if response.has_focus(){ACCENT}else{BORDER};
    ui.painter().set(background,Shape::Vec(vec![Shape::rect_filled(visible,4.0,FIELD),Shape::rect_stroke(visible,4.0,Stroke::new(1.0,border),StrokeKind::Inside)]));
    #[cfg(test)]
    ui.ctx().data_mut(|d|d.insert_temp(eframe::egui::Id::new(("compact_row_geometry","Timeout input")),(visible,response.rect)));
    response
}

pub fn card()->eframe::egui::Frame {eframe::egui::Frame::new().fill(SURFACE).stroke(Stroke::new(1.0,BORDER)).corner_radius(6).inner_margin(14)}
impl Role {
    pub fn source_px(self) -> f32 {
        match self { Self::Body | Self::Button => 13.0, Self::Small => 9.0,
            Self::Mono => 11.0, Self::Url | Self::Sidebar => 12.0,
            Self::Brand => 25.0, Self::Headline => 27.0,
            Self::Caption => 11.0, Self::Section => 12.0 }
    }
    pub fn size(self) -> f32 { self.source_px() + 4.0 / 3.0 }
}
pub fn apply(ctx: &eframe::egui::Context) {
    use eframe::egui::{Color32, FontFamily, FontId, TextStyle, Visuals, ThemePreference};
    ctx.set_theme(ThemePreference::Dark);
    let mut visuals = Visuals::dark();
    visuals.panel_fill = BG;
    visuals.window_fill = SURFACE;
    visuals.extreme_bg_color = FIELD;
    visuals.faint_bg_color = FIELD;
    visuals.override_text_color = Some(TEXT);
    visuals.selection.bg_fill = Color32::from_rgb(66,76,92);
    visuals.selection.stroke = Stroke::new(1.0,TEXT);
    visuals.window_stroke = Stroke::new(1.0,BORDER);
    for widget in [&mut visuals.widgets.inactive,&mut visuals.widgets.hovered,&mut visuals.widgets.active] {
        widget.bg_fill=Color32::from_rgb(39,46,56);
        widget.weak_bg_fill=widget.bg_fill;
        widget.bg_stroke=Stroke::new(1.0,BORDER);
        widget.fg_stroke=Stroke::new(1.0,TEXT);
        widget.corner_radius=eframe::egui::CornerRadius::same(4);
    }
    visuals.widgets.hovered.bg_fill=Color32::from_rgb(51,60,73);
    visuals.widgets.active.bg_fill=Color32::from_rgb(62,72,87);
    visuals.widgets.noninteractive.fg_stroke=Stroke::new(1.0,MUTED);
    visuals.widgets.noninteractive.bg_stroke=Stroke::new(1.0,BORDER);
    ctx.set_visuals(visuals);
    ctx.style_mut(|style| {
        for (text, role) in [(TextStyle::Body,Role::Body), (TextStyle::Button,Role::Button), (TextStyle::Small,Role::Small), (TextStyle::Monospace,Role::Mono), (TextStyle::Heading,Role::Headline)] {
            style.text_styles.insert(text, FontId::new(role.size(), if matches!(role, Role::Mono) { FontFamily::Monospace } else { FontFamily::Proportional }));
        }
        style.spacing.item_spacing = eframe::egui::vec2(8.0, 7.0);
        style.spacing.button_padding = eframe::egui::vec2(10.0, 7.0);
    });
}

pub fn font(role: Role) -> eframe::egui::FontId {
    eframe::egui::FontId::new(role.size(), if matches!(role, Role::Mono | Role::Url) { eframe::egui::FontFamily::Monospace } else { eframe::egui::FontFamily::Proportional })
}

/// Use installed platform Korean fonts; never install system packages.
/// Returns false if no CJK font was available, allowing a visible warning.
pub fn load_korean_font(ctx: &eframe::egui::Context) -> bool {
    let paths = ["/System/Library/Fonts/AppleSDGothicNeo.ttc", "C:/Windows/Fonts/malgun.ttf", "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", "/usr/share/fonts/truetype/nanum/NanumGothic.ttf"];
    let Some(bytes) = paths.iter().find_map(|p| std::fs::read(p).ok()).or_else(|| Some(include_bytes!("../assets/NotoSansKR.ttf").to_vec())) else { return false; };
    let mut fonts = eframe::egui::FontDefinitions::default();
    fonts.font_data.insert("korean".into(), std::sync::Arc::new(eframe::egui::FontData::from_owned(bytes)));
    for family in [eframe::egui::FontFamily::Proportional, eframe::egui::FontFamily::Monospace] {
        fonts.families.entry(family).or_default().push("korean".into());
    }
    ctx.set_fonts(fonts);
    true
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dark_theme_is_forced_and_default_text_styles_are_adjusted() {
        let ctx = eframe::egui::Context::default();
        ctx.set_visuals(eframe::egui::Visuals::light());
        apply(&ctx);
        assert!(ctx.style().visuals.dark_mode);
        for (style, role) in [(eframe::egui::TextStyle::Body, Role::Body), (eframe::egui::TextStyle::Button, Role::Button), (eframe::egui::TextStyle::Small, Role::Small), (eframe::egui::TextStyle::Monospace, Role::Mono), (eframe::egui::TextStyle::Heading, Role::Headline)] {
            assert_eq!(ctx.style().text_styles[&style].size, role.size());
        }
        assert_eq!(ctx.zoom_factor(), 1.0);
    }
    #[test]
    fn normal_text_tokens_meet_wcag_aa_on_every_surface() {
        fn luminance(c:Color32)->f32 {
            let linear=|v:u8| {let s=v as f32/255.0;if s<=0.04045 {s/12.92} else {((s+0.055)/1.055).powf(2.4)}};
            0.2126*linear(c.r())+0.7152*linear(c.g())+0.0722*linear(c.b())
        }
        for ink in [TEXT,MUTED,SUCCESS,ERROR] {for surface in [BG,SIDEBAR,SURFACE,FIELD,Color32::from_rgb(51,60,73)] {
            let a=luminance(ink);let b=luminance(surface);let ratio=(a.max(b)+0.05)/(a.min(b)+0.05);
            assert!(ratio>=4.5,"{ink:?} on {surface:?}: {ratio}");
        }}
        assert!((luminance(ACCENT)+0.05)/(luminance(ACCENT_INK)+0.05)>=4.5);
    }
    #[test]
    fn each_role_adds_exactly_one_typographic_point() {
        for role in [Role::Body, Role::Button, Role::Small, Role::Mono, Role::Url, Role::Sidebar, Role::Brand, Role::Headline, Role::Caption, Role::Section] {
            assert!((role.size() - role.source_px() - 4.0 / 3.0).abs() < 0.00001, "{role:?}");
        }
    }
}
