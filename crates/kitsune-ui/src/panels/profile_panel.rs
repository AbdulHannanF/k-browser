use crate::theme::KitsuneTheme;
use eframe::egui;
use kitsune_agent::profile::ProfileSummary;

pub fn profile_panel(ui: &mut egui::Ui, summary: Option<&ProfileSummary>) {
    ui.heading(egui::RichText::new("Profile").color(KitsuneTheme::TEXT_PRIMARY));
    ui.separator();

    let Some(s) = summary else {
        ui.label(egui::RichText::new("No profile indexed yet.").color(KitsuneTheme::TEXT1));
        ui.label(
            egui::RichText::new("Set a folder in Settings → Profile, then click Re-index.")
                .color(KitsuneTheme::TEXT2)
                .size(11.0),
        );
        return;
    };

    egui::Grid::new("profile_grid")
        .num_columns(2)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            ui.label(egui::RichText::new("Name").color(KitsuneTheme::TEXT2));
            ui.label(egui::RichText::new(&s.full_name).color(KitsuneTheme::TEXT_PRIMARY));
            ui.end_row();

            if let Some(nat) = &s.nationality {
                ui.label(egui::RichText::new("Nationality").color(KitsuneTheme::TEXT2));
                ui.label(egui::RichText::new(nat).color(KitsuneTheme::TEXT_PRIMARY));
                ui.end_row();
            }
            if let Some(email) = &s.email {
                ui.label(egui::RichText::new("Email").color(KitsuneTheme::TEXT2));
                ui.label(egui::RichText::new(email).color(KitsuneTheme::TEXT_PRIMARY));
                ui.end_row();
            }
        });

    ui.separator();
    ui.label(egui::RichText::new("Education").color(KitsuneTheme::TEXT1).strong());
    for edu in &s.education {
        let gpa = edu.gpa.map(|g| format!(", GPA {g:.1}")).unwrap_or_default();
        ui.label(
            egui::RichText::new(format!("  {} @ {}{}", edu.degree, edu.institution, gpa))
                .color(KitsuneTheme::TEXT2)
                .size(11.0),
        );
    }

    ui.separator();
    ui.label(egui::RichText::new("Languages").color(KitsuneTheme::TEXT1).strong());
    for lang in &s.languages {
        ui.label(
            egui::RichText::new(format!("  {} ({})", lang.language, lang.level))
                .color(KitsuneTheme::TEXT2)
                .size(11.0),
        );
    }

    ui.separator();
    ui.label(egui::RichText::new("Skills").color(KitsuneTheme::TEXT1).strong());
    ui.label(
        egui::RichText::new(s.skills.join(" · "))
            .color(KitsuneTheme::TEXT2)
            .size(11.0),
    );

    if let Some(ts) = &s.generated_at {
        ui.separator();
        ui.label(
            egui::RichText::new(format!("Indexed: {}", ts.format("%Y-%m-%d %H:%M")))
                .color(KitsuneTheme::TEXT3)
                .size(10.0),
        );
    }
}
