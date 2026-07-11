// ============================================================================
// modules/editor/src/focus_mode.rs — Mode focus / plein écran / typewriter
//
// F11 (configurable dans keybinds.ron) :
//   - la fenêtre éditeur passe en plein écran OS (ViewportCommand),
//   - le file_tree se masque (ModuleResponse::FocusModeChanged → core →
//     CoreEvent vers file_tree ; l'éditeur ne touche JAMAIS aux autres
//     modules directement),
//   - status bar réduite à l'essentiel (focus_minimal_status),
//   - fond noir pur (déjà le thème), colonne centrée (déjà le rendu).
// Rappui → retour exact à l'état précédent.
//
// Le mode typewriter (ligne courante maintenue au centre) vit ici aussi :
// c'est le même registre de confort d'écriture. Le scroll souris reste
// libre — seul un déplacement clavier recentre.
// ============================================================================

#[derive(Default)]
pub struct FocusMode {
    pub active: bool,
    /// Commande fullscreen à pousser au viewport à la prochaine frame.
    pub pending_fullscreen: Option<bool>,
}

impl FocusMode {
    /// Bascule. Retourne le nouvel état (à relayer au core pour le file_tree).
    pub fn toggle(&mut self) -> bool {
        self.active = !self.active;
        self.pending_fullscreen = Some(self.active);
        self.active
    }
}

/// Offset de scroll qui maintient la ligne du curseur à la position
/// typewriter (0.4 = un peu au-dessus du centre).
pub fn typewriter_offset(cursor_y: f32, viewport_h: f32, position: f32) -> f32 {
    (cursor_y - viewport_h * position.clamp(0.05, 0.95)).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_roundtrip() {
        let mut f = FocusMode::default();
        assert!(f.toggle());
        assert_eq!(f.pending_fullscreen, Some(true));
        assert!(!f.toggle());
        assert_eq!(f.pending_fullscreen, Some(false));
    }

    #[test]
    fn typewriter_keeps_line_at_position() {
        assert_eq!(typewriter_offset(1000.0, 800.0, 0.4), 680.0);
        assert_eq!(typewriter_offset(10.0, 800.0, 0.4), 0.0); // jamais négatif
    }
}
