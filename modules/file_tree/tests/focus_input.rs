// Reproduction headless du bug « champ à lettre unique » : on pilote egui sans
// écran via `Context::run`, en injectant des `Event::Text` sur des frames
// successives, puis on vérifie que le contenu accumule bien les caractères.
//
// Objectif : décider par démonstration si `request_focus()` appelé À CHAQUE
// FRAME est la cause, en le comparant à un focus demandé UNE SEULE FOIS.

fn type_over_frames(request_focus_every_frame: bool) -> String {
    let ctx = egui::Context::default();
    let mut text = String::new();
    let mut focused_once = false;

    // Frame 0 : établir le focus, aucune saisie encore.
    // Frames 1..=3 : injecter 'a', 'b', 'c' un par frame.
    let chars = ["", "a", "b", "c"];
    for c in chars {
        let mut input = egui::RawInput::default();
        if !c.is_empty() {
            input.events.push(egui::Event::Text(c.to_string()));
        }
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let edit = ui.text_edit_singleline(&mut text);
                if request_focus_every_frame {
                    edit.request_focus();
                } else if !focused_once {
                    edit.request_focus();
                    focused_once = true;
                }
            });
        });
    }
    text
}

#[test]
fn saisie_multi_caracteres_focus_une_fois() {
    assert_eq!(type_over_frames(false), "abc");
}

#[test]
fn saisie_multi_caracteres_focus_chaque_frame() {
    assert_eq!(type_over_frames(true), "abc");
}

// Structure EXACTE du dialogue réel : une `egui::Window` flottante imbriquée
// dans le `CentralPanel` du core, un seul champ, `request_focus()` par frame.
fn type_dialog_window() -> String {
    let ctx = egui::Context::default();
    let mut name = String::new();
    let chars = ["", "a", "b", "c"];
    for c in chars {
        let mut input = egui::RawInput::default();
        if !c.is_empty() {
            input.events.push(egui::Event::Text(c.to_string()));
        }
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.label("hub");
                egui::Window::new("Nouveau projet")
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ui.ctx(), |ui| {
                        let edit = ui.text_edit_singleline(&mut name);
                        edit.request_focus();
                    });
            });
        });
    }
    name
}

#[test]
fn dialogue_window_saisie() {
    assert_eq!(type_dialog_window(), "abc");
}

// Modélise l'architecture réelle : un viewport ENFANT (éditeur/cockpit) ouvert
// via `show_viewport_immediate` PENDANT qu'on tape dans le dialogue du viewport
// principal. L'enfant contient lui aussi un champ qui réclame le focus.
fn type_with_child_viewport(child_requests_focus: bool) -> String {
    let ctx = egui::Context::default();
    let mut name = String::new();
    let mut child_text = String::new();
    let chars = ["", "a", "b", "c"];
    for c in chars {
        let mut input = egui::RawInput::default();
        if !c.is_empty() {
            input.events.push(egui::Event::Text(c.to_string()));
        }
        let _ = ctx.run(input, |ctx| {
            // Le viewport enfant (fenêtre OS séparée dans l'app réelle).
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("child"),
                egui::ViewportBuilder::default(),
                |ctx, _class| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let e = ui.text_edit_singleline(&mut child_text);
                        if child_requests_focus {
                            e.request_focus();
                        }
                    });
                },
            );
            // Le dialogue dans le viewport principal.
            egui::CentralPanel::default().show(ctx, |ui| {
                egui::Window::new("Nouveau projet").show(ui.ctx(), |ui| {
                    ui.text_edit_singleline(&mut name).request_focus();
                });
            });
        });
    }
    name
}

#[test]
fn dialogue_avec_viewport_enfant_passif() {
    // Enfant présent mais qui ne réclame pas le focus : la saisie doit marcher.
    assert_eq!(type_with_child_viewport(false), "abc");
}

#[test]
fn dialogue_avec_viewport_enfant_qui_reclame_focus() {
    // Enfant qui réclame le focus chaque frame : reproduit le vol de focus
    // inter-viewport → la saisie du dialogue principal est cassée.
    let got = type_with_child_viewport(true);
    assert_ne!(
        got, "abc",
        "le vol de focus inter-viewport doit casser la saisie"
    );
}

// Correctif : chaque champ ne réclame le focus qu'UNE FOIS (première apparition),
// via un drapeau persistant — comme l'état d'un module dans l'app réelle.
fn type_with_child_viewport_fixed() -> String {
    let ctx = egui::Context::default();
    let mut name = String::new();
    let mut child_text = String::new();
    let mut name_focus_asked = false;
    let mut child_focus_asked = false;
    let chars = ["", "a", "b", "c"];
    for c in chars {
        let mut input = egui::RawInput::default();
        if !c.is_empty() {
            input.events.push(egui::Event::Text(c.to_string()));
        }
        let _ = ctx.run(input, |ctx| {
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("child"),
                egui::ViewportBuilder::default(),
                |ctx, _class| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let e = ui.text_edit_singleline(&mut child_text);
                        if !child_focus_asked {
                            e.request_focus();
                            child_focus_asked = true;
                        }
                    });
                },
            );
            egui::CentralPanel::default().show(ctx, |ui| {
                egui::Window::new("Nouveau projet").show(ui.ctx(), |ui| {
                    let e = ui.text_edit_singleline(&mut name);
                    if !name_focus_asked {
                        e.request_focus();
                        name_focus_asked = true;
                    }
                });
            });
        });
    }
    name
}

#[test]
fn dialogue_avec_viewport_enfant_focus_une_fois_ok() {
    // Avec « focus une seule fois » des deux côtés, plus de vol : le dialogue
    // (dessiné en dernier, donc gagnant du focus initial) reçoit la saisie.
    assert_eq!(type_with_child_viewport_fixed(), "abc");
}
