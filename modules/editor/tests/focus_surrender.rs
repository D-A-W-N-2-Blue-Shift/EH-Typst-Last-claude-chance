// Reproduction headless de la CAUSE RACINE du bug « saisie à une seule lettre ».
//
// La mémoire de focus d'egui est GLOBALE au `Context` (partagée par tous les
// viewports/fenêtres OS). Le module éditeur, quand sa recherche est fermée,
// exécutait chaque frame :
//     if let Some(id) = ctx.memory(|m| m.focused()) {
//         ctx.memory_mut(|m| m.surrender_focus(id));
//     }
// et ce, même quand la fenêtre éditeur n'était PAS la fenêtre active. Il
// arrachait donc le focus de n'importe quel champ texte d'un AUTRE viewport
// (dialogue nouveau projet, police du cockpit…) : la 1re lettre passe, le
// champ perd le focus la frame suivante, les lettres suivantes sont perdues.
//
// Ce test pilote egui headless (`Context::run`) et démontre :
//   - surrender de focus chaque frame ⇒ saisie cassée ;
//   - sans surrender (le correctif ne l'exécute que si l'éditeur est actif)
//     ⇒ saisie complète.

fn type_with_background_surrender(surrender_every_frame: bool) -> String {
    let ctx = egui::Context::default();
    let mut text = String::new();
    let mut focus_asked = false;
    let chars = ["", "a", "b", "c"];
    for c in chars {
        let mut input = egui::RawInput::default();
        if !c.is_empty() {
            input.events.push(egui::Event::Text(c.to_string()));
        }
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let e = ui.text_edit_singleline(&mut text);
                if !focus_asked {
                    e.request_focus();
                    focus_asked = true;
                }
            });
            // Simule le module éditeur qui relâche le focus global chaque frame.
            if surrender_every_frame {
                if let Some(id) = ctx.memory(|m| m.focused()) {
                    ctx.memory_mut(|m| m.surrender_focus(id));
                }
            }
        });
    }
    text
}

#[test]
fn surrender_focus_chaque_frame_casse_la_saisie() {
    // Le bug : un tiers arrache le focus global chaque frame.
    assert_ne!(
        type_with_background_surrender(true),
        "abc",
        "le surrender de focus chaque frame doit casser la saisie"
    );
}

#[test]
fn sans_surrender_la_saisie_est_complete() {
    // Le correctif : l'éditeur ne relâche le focus que s'il est la fenêtre
    // active, donc jamais celui d'un champ d'un autre viewport.
    assert_eq!(type_with_background_surrender(false), "abc");
}
