//! Small drawing helpers shared by the window and the dialogs.
use crate::settings::Look;
use crate::theme::{semibold, Palette};
use eframe::egui::{self, Color32, CornerRadius, FontId, Rect, RichText, Sense, Stroke, Vec2};

/// A rounded card (modern) or a plain framed box (classic).
pub fn card<R>(ui: &mut egui::Ui, p: &Palette, look: Look, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let radius = if look == Look::Classic { 3 } else { 16 };
    egui::Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(radius))
        .inner_margin(egui::Margin::symmetric(16, 14))
        .show(ui, add)
        .inner
}

pub fn caption(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text).font(FontId::new(12.0, semibold())).color(p.muted));
}

pub fn heading(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text).font(FontId::new(20.0, crate::theme::bold())).color(p.text));
}

pub fn muted(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text).size(12.5).color(p.muted));
}

/// Status light: green when running, orange while starting or stopping.
pub fn dot(ui: &mut egui::Ui, p: &Palette, on: bool, busy: bool) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
    let color = if busy { p.accent } else if on { p.green } else { p.faint };
    if on || busy {
        let mut halo = color;
        halo = halo.gamma_multiply(0.35);
        ui.painter().circle_filled(rect.center(), 7.0, halo);
    }
    ui.painter().circle_filled(rect.center(), 3.5, color);
}

/// Rounded label for PIDs and ports; green while the module runs.
pub fn chip(ui: &mut egui::Ui, p: &Palette, look: Look, text: &str, on: bool, width: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 24.0), Sense::hover());
    let (fill, stroke, fg) = if on {
        (p.green_soft, p.green_soft, p.green)
    } else {
        (p.card_alt, p.border, p.muted)
    };
    let radius = if look == Look::Classic { 2 } else { 9 };
    ui.painter().rect(
        rect,
        CornerRadius::same(radius),
        fill,
        Stroke::new(1.0, stroke),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        FontId::new(12.0, semibold()),
        fg,
    );
}

/// Button filled with a colour (Start, Stop, the accent buttons in dialogs).
pub fn filled_button(
    ui: &mut egui::Ui,
    p: &Palette,
    look: Look,
    text: &str,
    fill: Color32,
    min_width: f32,
) -> egui::Response {
    let radius = if look == Look::Classic { 3 } else { 10 };
    ui.add(
        egui::Button::new(RichText::new(text).font(FontId::new(13.5, semibold())).color(p.on_color))
            .fill(fill)
            .stroke(Stroke::NONE)
            .corner_radius(CornerRadius::same(radius))
            .min_size(Vec2::new(min_width, 30.0)),
    )
}

pub fn plain_button(ui: &mut egui::Ui, text: &str, min_width: f32) -> egui::Response {
    ui.add(egui::Button::new(text).min_size(Vec2::new(min_width, 30.0)))
}

/// The LAMPPost logo, drawn directly so the panel needs no image loader.
pub fn logo(ui: &mut egui::Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let painter = ui.painter();
    let orange = Color32::from_rgb(0xf0, 0x7a, 0x22);
    painter.rect_filled(rect, CornerRadius::same((size * 0.26) as u8), orange);
    let white = Color32::WHITE;
    let unit = size / 256.0;
    let at = |x: f32, y: f32| rect.min + Vec2::new(x * unit, y * unit);
    // lamp shade
    painter.add(egui::Shape::convex_polygon(
        vec![at(101.0, 56.0), at(155.0, 56.0), at(178.0, 114.0), at(78.0, 114.0)],
        white,
        Stroke::NONE,
    ));
    // post and base
    painter.rect_filled(Rect::from_min_max(at(117.0, 114.0), at(139.0, 194.0)), CornerRadius::ZERO, white);
    painter.rect_filled(
        Rect::from_min_max(at(84.0, 192.0), at(172.0, 216.0)),
        CornerRadius::same((12.0 * unit) as u8),
        white,
    );
}



/// egui draws checkboxes with the general widget radius, which makes them look
/// like radio buttons in the modern look. Square them off for a group.
pub fn square_ticks(ui: &mut egui::Ui) {
    let radius = CornerRadius::same(4);
    let box_fill = ui.visuals().extreme_bg_color;
    // an outline in the weak text colour stays visible on striped rows too
    let outline = Stroke::new(1.0, ui.visuals().weak_text_color());
    let widgets = &mut ui.visuals_mut().widgets;
    for widget in [&mut widgets.inactive, &mut widgets.hovered, &mut widgets.active] {
        widget.corner_radius = radius;
    }
    widgets.inactive.bg_fill = box_fill;
    widgets.inactive.weak_bg_fill = box_fill;
    widgets.inactive.bg_stroke = outline;
}

/// One table cell of a fixed width, so columns line up whatever the content.
pub fn cell<R>(ui: &mut egui::Ui, width: f32, right: bool, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 26.0), Sense::hover());
    let layout = if right {
        egui::Layout::right_to_left(egui::Align::Center)
    } else {
        egui::Layout::left_to_right(egui::Align::Center)
    };
    let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(layout));
    add(&mut inner)
}

/// A small rounded label sized to its text (scope, module, conflict).
pub fn tag(ui: &mut egui::Ui, text: &str, fg: Color32, bg: Color32) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(text.to_string(), FontId::new(11.5, semibold()), fg);
    let size = galley.size() + Vec2::new(16.0, 6.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(9), bg);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, fg);
    response
}

/// Section title with a line under it, as in the tables.
pub fn section(ui: &mut egui::Ui, p: &Palette, title: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(title).font(FontId::new(11.0, semibold())).color(p.muted));
    let (line, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(line, 0.0, p.border);
}

/// A settings row: title (and a hint) on the left, the control on the right.
/// Odd rows get the same stripe as the tables.
pub fn setting_row(
    ui: &mut egui::Ui,
    p: &Palette,
    index: usize,
    title: &str,
    hint: &str,
    control: impl FnOnce(&mut egui::Ui),
) {
    let height = if hint.is_empty() { 34.0 } else { 44.0 };
    let (row, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    if index % 2 == 1 {
        ui.painter().rect_filled(row, CornerRadius::same(6), p.card_alt);
    }
    let inner = row.shrink2(Vec2::new(10.0, 0.0));
    // centre the text block vertically: measure it, then place it
    let title_height = ui.fonts_mut(|f| f.row_height(&FontId::proportional(14.0)));
    let hint_height = if hint.is_empty() { 0.0 } else { ui.fonts_mut(|f| f.row_height(&FontId::proportional(11.5))) + 1.0 };
    let block = title_height + hint_height;
    let text = Rect::from_min_size(
        egui::pos2(inner.left(), inner.center().y - block / 2.0),
        Vec2::new(inner.width(), block),
    );
    let mut left = ui.new_child(
        egui::UiBuilder::new().max_rect(text).layout(egui::Layout::top_down(egui::Align::Min)),
    );
    left.spacing_mut().item_spacing.y = 1.0;
    if !title.is_empty() {
        left.label(RichText::new(title).color(p.text));
    }
    if !hint.is_empty() {
        left.label(RichText::new(hint).size(11.5).color(p.muted));
    }
    let mut right = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    control(&mut right);
}

/// Segmented control: one of a few options, the chosen one highlighted.
/// Returns true when the choice changed.
pub fn segmented<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    p: &Palette,
    value: &mut T,
    options: &[(T, &str)],
    enabled: bool,
) -> bool {
    let font = FontId::new(12.5, semibold());
    let pad = Vec2::new(12.0, 5.0);
    let galleys: Vec<_> = options
        .iter()
        .map(|(_, label)| ui.painter().layout_no_wrap(label.to_string(), font.clone(), p.text))
        .collect();
    let width: f32 = galleys.iter().map(|g| g.size().x + pad.x * 2.0).sum::<f32>() + 4.0;
    let height = galleys.iter().map(|g| g.size().y).fold(0.0, f32::max) + pad.y * 2.0 + 4.0;
    let (outer, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    ui.painter().rect(outer, CornerRadius::same(10), p.card_alt, Stroke::new(1.0, p.border), egui::StrokeKind::Inside);

    let mut changed = false;
    let mut x = outer.left() + 2.0;
    for ((option, label), galley) in options.iter().zip(galleys) {
        let w = galley.size().x + pad.x * 2.0;
        let rect = Rect::from_min_size(egui::pos2(x, outer.top() + 2.0), Vec2::new(w, height - 4.0));
        x += w;
        let response = ui.interact(rect, ui.id().with(label), if enabled { Sense::click() } else { Sense::hover() });
        let selected = *value == *option;
        if selected {
            ui.painter().rect_filled(rect, CornerRadius::same(8), p.card);
            ui.painter().rect_stroke(rect, CornerRadius::same(8), Stroke::new(1.0, p.border), egui::StrokeKind::Inside);
        } else if response.hovered() && enabled {
            ui.painter().rect_filled(rect, CornerRadius::same(8), p.hover_fill());
        }
        let colour = if !enabled { p.faint } else if selected { p.accent } else { p.muted };
        let text = ui.painter().layout_no_wrap(label.to_string(), font.clone(), colour);
        ui.painter().galley(rect.center() - text.size() / 2.0, text, colour);
        if response.clicked() && !selected {
            *value = *option;
            changed = true;
        }
    }
    changed
}
