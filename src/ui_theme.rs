use eframe::egui::{self, Color32, Rounding, Stroke, Vec2, Shadow, FontId, FontFamily};

pub struct MacTheme {
    // Colors
    pub background: Color32,
    pub surface: Color32,
    pub surface_hover: Color32,
    pub surface_active: Color32,
    pub card: Color32,
    pub card_hover: Color32,
    pub border: Color32,
    pub border_active: Color32,
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub text_muted: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub error: Color32,
    pub info: Color32,

    // Spacing
    pub spacing_small: f32,
    pub spacing_medium: f32,
    pub spacing_large: f32,
    pub padding_small: f32,
    pub padding_medium: f32,
    pub padding_large: f32,

    // Border radius
    pub radius_small: Rounding,
    pub radius_medium: Rounding,
    pub radius_large: Rounding,

    // Shadows
    pub shadow_small: Shadow,
    pub shadow_medium: Shadow,
    pub shadow_large: Shadow,

    // Typography
    pub font_small: FontId,
    pub font_medium: FontId,
    pub font_large: FontId,
    pub font_title: FontId,
}

impl Default for MacTheme {
    fn default() -> Self {
        Self {
            // macOS-inspired dark theme colors
            background: Color32::from_rgb(30, 30, 30),          // Dark background
            surface: Color32::from_rgb(45, 45, 45),            // Dark surface
            surface_hover: Color32::from_rgb(55, 55, 55),       // Hover state
            surface_active: Color32::from_rgb(65, 65, 65),      // Active state
            card: Color32::from_rgb(40, 40, 40),                // Card background
            card_hover: Color32::from_rgb(50, 50, 50),          // Card hover
            border: Color32::from_rgb(70, 70, 70),              // Border color
            border_active: Color32::from_rgb(100, 100, 100),    // Active border
            text_primary: Color32::from_rgb(255, 255, 255),     // Primary text
            text_secondary: Color32::from_rgb(200, 200, 200),   // Secondary text
            text_muted: Color32::from_rgb(140, 140, 140),       // Muted text
            accent: Color32::from_rgb(0, 122, 255),              // macOS blue
            accent_hover: Color32::from_rgb(10, 132, 255),      // Hover blue
            success: Color32::from_rgb(52, 199, 89),            // Green
            warning: Color32::from_rgb(255, 149, 0),            // Orange
            error: Color32::from_rgb(255, 59, 48),              // Red
            info: Color32::from_rgb(90, 200, 250),              // Light blue

            // Spacing - reduced by half for tighter UI
            spacing_small: 4.0,
            spacing_medium: 8.0,
            spacing_large: 12.0,
            padding_small: 6.0,
            padding_medium: 8.0,
            padding_large: 12.0,

            // Border radius
            radius_small: Rounding::same(6.0),
            radius_medium: Rounding::same(10.0),
            radius_large: Rounding::same(16.0),

            // Shadows
            shadow_small: Shadow {
                offset: Vec2::new(0.0, 1.0),
                blur: 3.0,
                spread: 0.0,
                color: Color32::from_black_alpha(25),
            },
            shadow_medium: Shadow {
                offset: Vec2::new(0.0, 2.0),
                blur: 8.0,
                spread: 0.0,
                color: Color32::from_black_alpha(40),
            },
            shadow_large: Shadow {
                offset: Vec2::new(0.0, 4.0),
                blur: 16.0,
                spread: 0.0,
                color: Color32::from_black_alpha(60),
            },

            // Typography
            font_small: FontId::new(12.0, FontFamily::Proportional),
            font_medium: FontId::new(14.0, FontFamily::Proportional),
            font_large: FontId::new(16.0, FontFamily::Proportional),
            font_title: FontId::new(20.0, FontFamily::Proportional),
        }
    }
}

impl MacTheme {
    pub fn apply_to_ctx(&self, ctx: &egui::Context) {
        let mut style = (*ctx.style()).clone();

        // Visuals
        style.visuals.panel_fill = self.background;
        style.visuals.window_fill = self.background;  // Match window fill with background
        style.visuals.window_shadow = self.shadow_medium;
        style.visuals.window_rounding = self.radius_large;
        style.visuals.window_stroke = Stroke::new(1.0, self.background);  // Match window stroke with background

        // Buttons
        style.visuals.button_frame = true;
        style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, self.text_primary);
        style.visuals.widgets.inactive.bg_fill = self.surface;
        style.visuals.widgets.inactive.rounding = self.radius_medium;
        style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, self.border);

        style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, self.text_primary);
        style.visuals.widgets.hovered.bg_fill = self.surface_hover;
        style.visuals.widgets.hovered.rounding = self.radius_medium;
        style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, self.border_active);

        style.visuals.widgets.active.fg_stroke = Stroke::new(1.0, self.text_primary);
        style.visuals.widgets.active.bg_fill = self.surface_active;
        style.visuals.widgets.active.rounding = self.radius_medium;
        style.visuals.widgets.active.bg_stroke = Stroke::new(1.0, self.border_active);

        // Text inputs
        style.visuals.text_cursor.stroke = Stroke::new(2.0, self.accent);
        style.visuals.selection.bg_fill = self.accent;
        style.visuals.selection.stroke = Stroke::new(1.0, self.accent);

        // Hyperlinks
        style.visuals.hyperlink_color = self.accent;

        // Text styles
        style.text_styles = [
            (egui::TextStyle::Heading, self.font_title.clone()),
            (egui::TextStyle::Body, self.font_medium.clone()),
            (egui::TextStyle::Monospace, FontId::new(14.0, FontFamily::Monospace)),
            (egui::TextStyle::Button, self.font_medium.clone()),
            (egui::TextStyle::Small, self.font_small.clone()),
        ].into();

        ctx.set_style(style);
    }

    pub fn card_frame(&self) -> egui::Frame {
        egui::Frame {
            inner_margin: egui::Margin::symmetric(self.padding_medium, self.padding_medium),
            // outer_margin: egui::Margin::symmetric(self.padding_medium, self.spacing_small),
            rounding: self.radius_large,
            shadow: self.shadow_medium,
            fill: self.card,
            // stroke: Stroke::new(1.0, self.border),
            ..Default::default()
        }
    }

    pub fn card_frame_borderless(&self) -> egui::Frame {
        egui::Frame {
            inner_margin: egui::Margin::symmetric(self.padding_medium, self.padding_medium),
            // outer_margin: egui::Margin::symmetric(self.padding_medium, self.spacing_small),
            rounding: self.radius_large,
            shadow: self.shadow_medium,
            // fill: self.card,
            // stroke: Stroke::new(1.0, self.border),
            ..Default::default()
        }
    }

    pub fn primary_button(&self) -> egui::Frame {
        egui::Frame {
            inner_margin: egui::Margin::symmetric(self.padding_large, self.padding_medium),
            rounding: self.radius_medium,
            shadow: self.shadow_small,
            fill: self.accent,
            stroke: Stroke::new(1.0, self.accent),
            ..Default::default()
        }
    }

    pub fn secondary_button(&self) -> egui::Frame {
        egui::Frame {
            inner_margin: egui::Margin::symmetric(self.padding_large, self.padding_medium),
            rounding: self.radius_medium,
            shadow: self.shadow_small,
            fill: self.surface,
            stroke: Stroke::new(1.0, self.border),
            ..Default::default()
        }
    }

    pub fn status_color(&self, status: &str) -> Color32 {
        match status {
            "success" | "completed" => self.success,
            "warning" | "uploading" => self.warning,
            "error" | "failed" => self.error,
            "info" | "connected" => self.info,
            _ => self.text_secondary,
        }
    }
}