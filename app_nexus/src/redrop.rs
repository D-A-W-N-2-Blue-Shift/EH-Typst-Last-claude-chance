// ============================================================================
// app_nexus/src/redrop.rs — Le bouton Redrop (doc §6)
//
// « Ce n'est pas un module — c'est un CoreEvent global déclenché depuis
// n'importe où » (doc §2). Vit dans le shell (app_nexus), pas dans un
// module. Atteint depuis toute fenêtre via la palette (Ctrl+Shift+P →
// « redrop »), elle-même déjà accessible partout (§7 : chaque viewport
// enfant qui veut la déclencher pousse ModuleResponse::OpenPaletteRequested
// — variant DÉJÀ existante du core, générique, aucune modification requise).
//
// Comportement exact du doc §6 :
//   1. Popup minimaliste (2 champs) : molécule + dose en mg (pré-remplie).
//   2. « Confirmer » → écrit dans med_doses avec taken_at = now().
//   3. Notification "logged" pendant 2 secondes, disparaît seule.
//   4. Retour immédiat au contexte précédent.
// Distinct du bouton "Prise" de la sous-vue Médication (health) : ici le
// timestamp n'est JAMAIS éditable, toujours now() — c'est le point (« 3
// secondes de friction »).
// ============================================================================

use std::time::{Duration, Instant};

pub enum RedropState {
    Closed,
    Open {
        medications: Vec<nexus_db::Medication>,
        selected: usize,
        dose_mg: f32,
    },
    /// Notification "logged" auto-disparaissante (doc §6 point 3).
    Confirmed {
        until: Instant,
    },
}

impl Default for RedropState {
    fn default() -> Self {
        Self::Closed
    }
}

/// Ouvre le dialogue : charge le registre de médicaments du projet
/// `root`. Erreur si la base est illisible — l'appelant décide de
/// l'affichage (statut GLaDOS), Redrop ne masque jamais un échec.
pub fn open(root: &std::path::Path) -> Result<RedropState, String> {
    let db_path = root.join(".engram").join("nexus.db");
    let conn = nexus_db::open_db(&db_path).map_err(|e| e.to_string())?;
    let medications = nexus_db::list_medications(&conn).map_err(|e| e.to_string())?;
    let dose_mg = medications
        .first()
        .map(|m| m.dose_default as f32)
        .unwrap_or(0.0);
    Ok(RedropState::Open {
        medications,
        selected: 0,
        dose_mg,
    })
}

/// Dessine le popup dans le contexte fourni (la fenêtre core : la palette
/// qui a mené ici est déjà accessible depuis partout, inutile de dupliquer
/// Redrop lui-même dans chaque viewport). Retourne `Some((med_id, dose_mg))`
/// si « Confirmer » a été cliqué — l'appelant écrit alors `med_doses` avec
/// `taken_at = now()` et fait passer l'état en `Confirmed`.
pub fn show(ctx: &egui::Context, state: &mut RedropState) -> Option<(String, f64)> {
    let mut confirmed = None;
    match state {
        RedropState::Closed => {}
        RedropState::Open {
            medications,
            selected,
            dose_mg,
        } => {
            let mut close = false;
            egui::Window::new("Redrop")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    if medications.is_empty() {
                        ui.label(
                            "Aucun médicament au registre. Ajoute-en un depuis la fenêtre Santé.",
                        );
                        if ui.button("Fermer").clicked() {
                            close = true;
                        }
                        return;
                    }
                    egui::ComboBox::from_label("Molécule")
                        .selected_text(medications[*selected].nom.as_str())
                        .show_ui(ui, |ui| {
                            for (i, m) in medications.iter().enumerate() {
                                if ui.selectable_value(selected, i, &m.nom).clicked() {
                                    *dose_mg = m.dose_default as f32;
                                }
                            }
                        });
                    ui.horizontal(|ui| {
                        ui.label("Dose (mg) :");
                        ui.add(egui::DragValue::new(dose_mg).speed(0.5).range(0.0..=2000.0));
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Confirmer").clicked() {
                            confirmed =
                                Some((medications[*selected].id.clone(), f64::from(*dose_mg)));
                        }
                        if ui.button("Annuler").clicked() {
                            close = true;
                        }
                    });
                });
            if close {
                *state = RedropState::Closed;
            }
        }
        RedropState::Confirmed { until } => {
            if Instant::now() >= *until {
                *state = RedropState::Closed;
            } else {
                egui::Window::new("redrop_confirmed")
                    .collapsible(false)
                    .resizable(false)
                    .title_bar(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.colored_label(egui::Color32::from_rgb(120, 255, 180), "✓ logged");
                    });
                ctx.request_repaint_after(Duration::from_millis(100));
            }
        }
    }
    confirmed
}

/// Durée d'affichage de la notification "logged" (doc §6 point 3 : « 2 secondes »).
pub fn confirmed_until_now() -> Instant {
    Instant::now() + Duration::from_secs(2)
}
